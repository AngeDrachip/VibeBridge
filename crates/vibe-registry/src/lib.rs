//! Vibe's mod registry: identity, namespace ownership, dependency ordering,
//! and a factual load plan. It deliberately does not claim that a mod is
//! compatible; it only plans what Vibe can attempt.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vibe_core::metadata::{Dependency, ModSide};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDependency {
    pub owner: ModId,
    pub dependency: String,
    pub mandatory: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateId {
    pub id: String,
    pub jars: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryPlan {
    pub mods: Vec<RegisteredMod>,
    pub load_order: Vec<ModId>,
    pub missing_dependencies: Vec<MissingDependency>,
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

/// Build a plan from inspected jars.
pub fn plan(inspections: &[JarInspection]) -> RegistryPlan {
    let mut mods = Vec::new();
    let mut seen: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

    for inspection in inspections {
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
            });
        }
    }

    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let duplicate_ids: Vec<DuplicateId> = seen
        .into_iter()
        .filter(|(_, jars)| jars.len() > 1)
        .map(|(id, jars)| DuplicateId { id, jars })
        .collect();

    let known: BTreeSet<String> = mods.iter().map(|m| m.id.to_string()).collect();
    let mut missing_dependencies = Vec::new();
    for owner in &mods {
        for dep in &owner.dependencies {
            if is_environment_dependency(&dep.mod_id) {
                continue;
            }
            if !known.contains(&dep.mod_id) {
                missing_dependencies.push(MissingDependency {
                    owner: owner.id.clone(),
                    dependency: dep.mod_id.clone(),
                    mandatory: dep.mandatory,
                    reason: "not present in the scanned mod set".into(),
                });
            }
        }
    }

    let (load_order, cycles) = topological_order(&mods);
    let (namespace_owners, namespace_conflicts) = namespace_index(&mods);

    RegistryPlan {
        mods,
        load_order,
        missing_dependencies,
        duplicate_ids,
        cycles,
        namespace_owners,
        namespace_conflicts,
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
        RegisteredMod {
            id: ModId::parse(id).unwrap(),
            version: "1".into(),
            display_name: id.into(),
            jar: PathBuf::from(format!("{id}.jar")),
            entrypoint: None,
            verdict: Verdict::RunnableSubset,
            side: ModSide::Both,
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
}
