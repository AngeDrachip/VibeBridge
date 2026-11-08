//! Vibe's mod registry: identity, namespace ownership, dependency ordering,
//! and a factual load plan. It deliberately does not claim that a mod is
//! compatible; it only plans what Vibe can attempt.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vibe_core::metadata::{Dependency, ModSide};
use vibe_core::profile::{ModRequirements, ProfileMatrix, ResolutionStatus};
use vibe_core::version::VersionRange;
use vibe_core::{ModId, Verdict};
use vibe_loader::JarInspection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredMod {
    pub id: ModId,
    pub version: String,
    pub display_name: String,
    pub jar: PathBuf,
    pub entrypoint: Option<String>,
    pub verdict: Verdict,
    pub side: ModSide,
    pub dependencies: Vec<Dependency>,
    pub namespaces: Vec<String>,
    /// The compatibility profile this mod resolved to, if exactly one matched.
    #[serde(default)]
    pub target_profile: Option<String>,
    /// How the profile was resolved (`matched`, `ambiguous`, ...).
    #[serde(default)]
    pub profile_status: Option<ResolutionStatus>,
    /// The Minecraft range declared by the mod, kept verbatim.
    #[serde(default)]
    pub minecraft_range: Option<String>,
    /// The Forge loader range declared by the mod, kept verbatim.
    #[serde(default)]
    pub forge_range: Option<String>,
    /// Why the profile did not resolve, for diagnostics.
    #[serde(default)]
    pub profile_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDependency {
    pub owner: ModId,
    pub dependency: String,
    pub mandatory: bool,
    /// The side on which the dependency was declared. Only dependencies that
    /// apply to the planned side are reported.
    pub side: ModSide,
    /// The declared Maven range, kept verbatim.
    pub version_range: String,
    pub reason: String,
}

/// A present dependency whose version does not satisfy the declared range.
/// The range is reported as text; Vibe does not pick a version or download one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMismatch {
    pub owner: ModId,
    pub dependency: String,
    pub required: String,
    pub found: String,
    pub mandatory: bool,
    pub side: ModSide,
}

/// A mod left out of a side-specific plan because it declares the other side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedMod {
    pub id: ModId,
    pub side: ModSide,
    pub jar: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateId {
    pub id: String,
    pub jars: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryPlan {
    /// The side this plan was built for; `None` means every scanned jar.
    pub side: Option<ModSide>,
    /// The single compatibility profile shared by every planned mod, when one
    /// profile covers the whole set.
    #[serde(default)]
    pub profile: Option<String>,
    /// Mods that could not be assigned to one shared profile.
    #[serde(default)]
    pub profile_conflicts: Vec<String>,
    pub mods: Vec<RegisteredMod>,
    pub load_order: Vec<ModId>,
    pub missing_dependencies: Vec<MissingDependency>,
    pub version_mismatches: Vec<VersionMismatch>,
    /// Mods excluded because they declare the other side. They are still
    /// scanned and inspected; they are simply not planned on this side.
    pub excluded_mods: Vec<ExcludedMod>,
    pub duplicate_ids: Vec<DuplicateId>,
    pub cycles: Vec<Vec<ModId>>,
    pub namespace_owners: BTreeMap<String, Vec<ModId>>,
    pub namespace_conflicts: BTreeMap<String, Vec<ModId>>,
}

impl RegistryPlan {
    pub fn by_id(&self, id: &str) -> Option<&RegisteredMod> {
        self.mods.iter().find(|m| m.id.as_str() == id)
    }

    pub fn runnable_subset(&self) -> Vec<&RegisteredMod> {
        self.mods
            .iter()
            .filter(|m| m.verdict == Verdict::RunnableSubset)
            .collect()
    }

    pub fn blocked(&self) -> Vec<&RegisteredMod> {
        self.mods
            .iter()
            .filter(|m| m.verdict == Verdict::Blocked)
            .collect()
    }
}

/// Build a plan from inspected jars, covering both sides.
pub fn plan(inspections: &[JarInspection]) -> RegistryPlan {
    plan_for_side_with_profile(inspections, None, None)
}

/// Build a plan for one runtime side.
///
/// Mods that declare the other side are listed in `excluded_mods` rather than
/// dropped. Dependencies declared only for the other side are not reported as
/// missing. `None` plans every scanned jar, which is what `vibe plan` did
/// before side selection existed.
pub fn plan_for_side(inspections: &[JarInspection], side: Option<ModSide>) -> RegistryPlan {
    plan_for_side_with_profile(inspections, side, None)
}

/// Build a side-aware plan and resolve every mod against the embedded
/// compatibility matrix. `requested_profile` is an explicit `--profile` choice;
/// it is checked against each mod's declarations rather than assumed.
pub fn plan_for_side_with_profile(
    inspections: &[JarInspection],
    side: Option<ModSide>,
    requested_profile: Option<&str>,
) -> RegistryPlan {
    let matrix = ProfileMatrix::embedded();
    let (mods, duplicate_ids) = collect_mods(inspections, matrix.as_ref().ok(), requested_profile);
    build_plan(mods, duplicate_ids, side)
}

fn collect_mods(
    inspections: &[JarInspection],
    matrix: Option<&ProfileMatrix>,
    requested_profile: Option<&str>,
) -> (Vec<RegisteredMod>, Vec<DuplicateId>) {
    let mut mods = Vec::new();
    let mut seen: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for inspection in inspections {
        let resolution = matrix.map(|matrix| {
            matrix.resolve(
                &ModRequirements::from_metadata(&inspection.metadata),
                requested_profile,
            )
        });
        let mut namespaces: BTreeSet<String> = BTreeSet::new();
        namespaces.extend(inspection.asset_namespaces.iter().cloned());
        namespaces.extend(inspection.data_namespaces.iter().cloned());

        for metadata in &inspection.metadata.mods {
            let id = metadata.mod_id.clone();
            seen.entry(id.to_string())
                .or_default()
                .push(inspection.path.clone());

            let entrypoint = pick_entrypoint(inspection, &id);
            let dependencies = inspection
                .metadata
                .dependencies
                .get(id.as_str())
                .or_else(|| inspection.metadata.dependencies.get(id.as_str()))
                .cloned()
                .unwrap_or_default();

            let mut mod_namespaces: Vec<String> = namespaces.iter().cloned().collect();
            if !mod_namespaces.iter().any(|ns| ns == id.as_str()) {
                mod_namespaces.push(id.to_string());
            }
            mod_namespaces.sort();
            mod_namespaces.dedup();

            let (target_profile, profile_status, minecraft_range, forge_range, profile_reasons) =
                match &resolution {
                    Some(resolution) => (
                        resolution.selected.clone(),
                        Some(resolution.status),
                        resolution.requirements.minecraft_range.clone(),
                        resolution.requirements.loader_range.clone(),
                        if resolution.is_matched() {
                            Vec::new()
                        } else {
                            resolution.reasons.clone()
                        },
                    ),
                    None => (
                        None,
                        None,
                        inspection.metadata.minecraft_version_range.clone(),
                        inspection.metadata.forge_version_range.clone(),
                        vec!["the embedded profile matrix could not be loaded".to_string()],
                    ),
                };

            mods.push(RegisteredMod {
                id,
                version: metadata.version.clone(),
                display_name: metadata.display_name.clone(),
                jar: inspection.path.clone(),
                entrypoint,
                verdict: inspection.report.verdict,
                side: metadata.side,
                dependencies,
                namespaces: mod_namespaces,
                target_profile,
                profile_status,
                minecraft_range,
                forge_range,
                profile_reasons,
            });
        }
    }

    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let duplicate_ids: Vec<DuplicateId> = seen
        .into_iter()
        .filter(|(_, jars)| jars.len() > 1)
        .map(|(id, jars)| DuplicateId { id, jars })
        .collect();
    (mods, duplicate_ids)
}

fn build_plan(
    mut all_mods: Vec<RegisteredMod>,
    duplicate_ids: Vec<DuplicateId>,
    side: Option<ModSide>,
) -> RegistryPlan {
    all_mods.sort_by(|a, b| a.id.cmp(&b.id));
    let mut mods = Vec::new();
    let mut excluded_mods = Vec::new();
    for module in all_mods {
        if side
            .map(|planned| module.side.applies_to(planned))
            .unwrap_or(true)
        {
            mods.push(module);
        } else {
            let planned = side.unwrap_or(ModSide::Both);
            excluded_mods.push(ExcludedMod {
                id: module.id,
                side: module.side,
                jar: module.jar,
                reason: format!(
                    "declared side {} does not run on a {} plan",
                    module.side.as_str(),
                    planned.as_str()
                ),
            });
        }
    }

    let known: BTreeMap<String, &RegisteredMod> = mods
        .iter()
        .map(|module| (module.id.to_string(), module))
        .collect();
    let mut missing_dependencies = Vec::new();
    let mut version_mismatches = Vec::new();
    for owner in &mods {
        for dep in &owner.dependencies {
            if is_environment_dependency(&dep.mod_id) {
                continue;
            }
            if let Some(planned) = side {
                if !dep.side.applies_to(planned) {
                    continue;
                }
            }
            match known.get(&dep.mod_id) {
                None => missing_dependencies.push(MissingDependency {
                    owner: owner.id.clone(),
                    dependency: dep.mod_id.clone(),
                    mandatory: dep.mandatory,
                    side: dep.side,
                    version_range: dep.version_range.clone(),
                    reason: "not present in the scanned mod set".into(),
                }),
                Some(found) => {
                    if range_is_satisfied(&dep.version_range, &found.version) {
                        continue;
                    }
                    version_mismatches.push(VersionMismatch {
                        owner: owner.id.clone(),
                        dependency: dep.mod_id.clone(),
                        required: dep.version_range.clone(),
                        found: found.version.clone(),
                        mandatory: dep.mandatory,
                        side: dep.side,
                    });
                }
            }
        }
    }

    let (load_order, cycles) = topological_order(&mods);
    let (namespace_owners, namespace_conflicts) = namespace_index(&mods);

    let mut profiles: BTreeSet<String> = BTreeSet::new();
    let mut profile_conflicts = Vec::new();
    for module in &mods {
        match &module.target_profile {
            Some(profile) => {
                profiles.insert(profile.clone());
            }
            None => {
                let reasons = if module.profile_reasons.is_empty() {
                    "no compatibility profile was resolved".to_string()
                } else {
                    module.profile_reasons.join("; ")
                };
                profile_conflicts.push(format!("{}: {reasons}", module.id));
            }
        }
    }
    let profile = if profiles.len() == 1 && profile_conflicts.is_empty() {
        profiles.iter().next().cloned()
    } else {
        if profiles.len() > 1 {
            profile_conflicts.push(format!(
                "mods target different profiles: {}",
                profiles.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        None
    };

    RegistryPlan {
        side,
        profile,
        profile_conflicts,
        mods,
        load_order,
        missing_dependencies,
        version_mismatches,
        excluded_mods,
        duplicate_ids,
        cycles,
        namespace_owners,
        namespace_conflicts,
    }
}

/// An empty, `*`, `any`, or unparseable range never counts as a mismatch:
/// Vibe reports only what it can establish from the declared text.
fn range_is_satisfied(range: &str, version: &str) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" || range.eq_ignore_ascii_case("any") {
        return true;
    }
    match VersionRange::parse(range) {
        Ok(parsed) => parsed.matches(version),
        Err(_) => true,
    }
}

fn pick_entrypoint(inspection: &JarInspection, id: &ModId) -> Option<String> {
    if let Some(found) = inspection.entrypoints.iter().find(|class| {
        inspection
            .entrypoint_mod_ids
            .get(*class)
            .map(|ids| ids.iter().any(|candidate| candidate == id.as_str()))
            .unwrap_or(false)
    }) {
        return Some(found.clone());
    }
    if inspection.metadata.mods.len() == 1 {
        inspection.entrypoints.first().cloned()
    } else {
        None
    }
}

fn is_environment_dependency(id: &str) -> bool {
    matches!(id, "forge" | "minecraft" | "neoforge" | "fml" | "java")
}

fn record_edge(
    edges: &mut BTreeMap<String, BTreeSet<String>>,
    indegree: &mut BTreeMap<String, usize>,
    from: &str,
    to: &str,
) {
    if edges
        .entry(from.to_string())
        .or_default()
        .insert(to.to_string())
    {
        *indegree.entry(to.to_string()).or_default() += 1;
    }
}

fn topological_order(mods: &[RegisteredMod]) -> (Vec<ModId>, Vec<Vec<ModId>>) {
    let ids: BTreeSet<String> = mods.iter().map(|m| m.id.to_string()).collect();
    let mut indegree: BTreeMap<String, usize> = ids.iter().map(|id| (id.clone(), 0)).collect();
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for module in mods {
        for dep in &module.dependencies {
            if !ids.contains(&dep.mod_id) || dep.mod_id == module.id.as_str() {
                continue;
            }
            match dep.ordering.to_ascii_uppercase().as_str() {
                // module depends on dep: dep -> module
                "AFTER" => record_edge(
                    &mut edges,
                    &mut indegree,
                    dep.mod_id.as_str(),
                    module.id.as_str(),
                ),
                // module must load before dep: module -> dep
                "BEFORE" => record_edge(
                    &mut edges,
                    &mut indegree,
                    module.id.as_str(),
                    dep.mod_id.as_str(),
                ),
                _ => {}
            }
        }
    }

    let mut ready: VecDeque<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    // BTreeMap iteration is sorted, so this is deterministic.
    let mut order = Vec::new();
    while let Some(id) = ready.pop_front() {
        order.push(id.clone());
        if let Some(children) = edges.get(&id) {
            for child in children {
                if let Some(degree) = indegree.get_mut(child) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push_back(child.clone());
                    }
                }
            }
        }
    }

    let cycles = if order.len() == ids.len() {
        Vec::new()
    } else {
        let remaining: Vec<ModId> = indegree
            .iter()
            .filter(|(_, degree)| **degree > 0)
            .filter_map(|(id, _)| ModId::parse(id.clone()).ok())
            .collect();
        vec![remaining]
    };

    (
        order
            .into_iter()
            .filter_map(|id| ModId::parse(id).ok())
            .collect(),
        cycles,
    )
}

fn namespace_index(
    mods: &[RegisteredMod],
) -> (BTreeMap<String, Vec<ModId>>, BTreeMap<String, Vec<ModId>>) {
    let mut owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
    for module in mods {
        for namespace in &module.namespaces {
            if namespace == "minecraft" || namespace == "c" || namespace == "horizon" {
                continue;
            }
            owners
                .entry(namespace.clone())
                .or_default()
                .push(module.id.clone());
        }
    }
    let conflicts = owners
        .iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(namespace, ids)| (namespace.clone(), ids.clone()))
        .collect();
    (owners, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &str, deps: &[(&str, &str)]) -> RegisteredMod {
        module_on(id, ModSide::Both, deps)
    }

    fn module_on(id: &str, side: ModSide, deps: &[(&str, &str)]) -> RegisteredMod {
        RegisteredMod {
            id: ModId::parse(id).unwrap(),
            version: "1".into(),
            display_name: id.into(),
            jar: PathBuf::from(format!("{id}.jar")),
            entrypoint: None,
            verdict: Verdict::RunnableSubset,
            side,
            dependencies: deps
                .iter()
                .map(|(mod_id, ordering)| Dependency {
                    mod_id: mod_id.to_string(),
                    mandatory: true,
                    version_range: "*".into(),
                    ordering: ordering.to_string(),
                    side: ModSide::Both,
                })
                .collect(),
            namespaces: vec![id.into()],
            target_profile: Some("forge-1.21.1-52".into()),
            profile_status: Some(ResolutionStatus::Matched),
            minecraft_range: Some("[1.21.1,1.22)".into()),
            forge_range: Some("[52,)".into()),
            profile_reasons: Vec::new(),
        }
    }

    #[test]
    fn orders_after_dependencies() {
        let mods = vec![module("aaa", &[("zzz", "AFTER")]), module("zzz", &[])];
        let (order, cycles) = topological_order(&mods);
        assert!(cycles.is_empty());
        assert_eq!(
            order.iter().map(ModId::as_str).collect::<Vec<_>>(),
            vec!["zzz", "aaa"]
        );
    }

    #[test]
    fn detects_cycles() {
        let mods = vec![
            module("aaa", &[("bbb", "AFTER")]),
            module("bbb", &[("aaa", "AFTER")]),
        ];
        let (_, cycles) = topological_order(&mods);
        assert_eq!(cycles.len(), 1);
    }

    #[test]
    fn reports_namespace_conflicts() {
        let mut a = module("aaa", &[]);
        let mut b = module("bbb", &[]);
        a.namespaces.push("shared".into());
        b.namespaces.push("shared".into());
        let (_, conflicts) = namespace_index(&[a, b]);
        assert!(conflicts.contains_key("shared"));
    }

    fn plan_of(mods: Vec<RegisteredMod>, side: Option<ModSide>) -> RegistryPlan {
        build_plan(mods, Vec::new(), side)
    }

    fn owner_with_dependency(side: ModSide, dependency_side: ModSide) -> RegisteredMod {
        let mut owner = module_on("owner", side, &[("client_lib", "AFTER")]);
        owner.dependencies[0].side = dependency_side;
        owner
    }

    #[test]
    fn side_specific_missing_dependencies_follow_the_plan() {
        let client_missing = plan_of(
            vec![owner_with_dependency(ModSide::Both, ModSide::Client)],
            Some(ModSide::Client),
        );
        assert_eq!(client_missing.missing_dependencies.len(), 1);
        assert_eq!(client_missing.missing_dependencies[0].side, ModSide::Client);

        let server = plan_of(
            vec![owner_with_dependency(ModSide::Both, ModSide::Client)],
            Some(ModSide::Server),
        );
        assert!(
            server.missing_dependencies.is_empty(),
            "{:?}",
            server.missing_dependencies
        );

        let both = plan_of(
            vec![owner_with_dependency(ModSide::Both, ModSide::Client)],
            None,
        );
        assert_eq!(both.missing_dependencies.len(), 1);
    }

    #[test]
    fn a_mod_for_the_other_side_is_excluded_not_dropped() {
        let plan = plan_of(
            vec![
                module_on("client_only", ModSide::Client, &[]),
                module_on("server_only", ModSide::Server, &[]),
                module_on("either", ModSide::Both, &[]),
            ],
            Some(ModSide::Server),
        );
        assert_eq!(
            plan.mods
                .iter()
                .map(|module| module.id.as_str())
                .collect::<Vec<_>>(),
            vec!["either", "server_only"]
        );
        assert_eq!(plan.excluded_mods.len(), 1);
        assert_eq!(plan.excluded_mods[0].id.as_str(), "client_only");
        assert_check_json_round_trip(&plan);
    }

    #[test]
    fn an_unknown_side_is_kept_on_both_sides() {
        for side in [ModSide::Client, ModSide::Server] {
            let plan = plan_of(
                vec![module_on("unclear", ModSide::Unknown, &[])],
                Some(side),
            );
            assert_eq!(plan.mods.len(), 1, "{side:?}");
            assert!(plan.excluded_mods.is_empty(), "{side:?}");
        }
    }

    fn assert_check_json_round_trip(plan: &RegistryPlan) {
        let text = serde_json::to_string(plan).unwrap();
        let parsed: RegistryPlan = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.mods.len(), plan.mods.len());
        assert_eq!(parsed.excluded_mods.len(), plan.excluded_mods.len());
    }

    #[test]
    fn reports_present_but_out_of_range_dependencies() {
        let mut owner = module_on("owner", ModSide::Both, &[("lib", "AFTER")]);
        owner.dependencies[0].version_range = "[2,)".into();
        let mut lib = module("lib", &[]);
        lib.version = "1.5".into();

        let plan = plan_of(vec![owner, lib], Some(ModSide::Server));
        assert!(plan.missing_dependencies.is_empty());
        assert_eq!(plan.version_mismatches.len(), 1);
        assert_eq!(plan.version_mismatches[0].required, "[2,)");
        assert_eq!(plan.version_mismatches[0].found, "1.5");
    }

    #[test]
    fn a_satisfied_or_unconstrained_range_is_not_a_mismatch() {
        let mut owner = module_on("owner", ModSide::Both, &[("lib", "AFTER")]);
        owner.dependencies[0].version_range = "[1,2)".into();
        let mut lib = module("lib", &[]);
        lib.version = "1.5".into();
        let plan = plan_of(vec![owner, lib], None);
        assert!(plan.version_mismatches.is_empty());

        let mut wildcard = module_on("other", ModSide::Both, &[("lib", "AFTER")]);
        wildcard.dependencies[0].version_range = String::new();
        let plan = plan_of(vec![wildcard, module("lib", &[])], None);
        assert!(plan.version_mismatches.is_empty());
    }
}
