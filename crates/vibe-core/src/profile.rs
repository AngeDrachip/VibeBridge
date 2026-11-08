//! Compatibility profiles: one Minecraft/Forge/backend target per run.
//!
//! Vibe's own version is independent from the Minecraft and Forge versions in
//! this matrix. A mod is matched to a profile from the version ranges it
//! declares in `mods.toml` or `mcmod.info`. When the declaration is missing or
//! matches more than one profile, Vibe reports the ambiguity instead of
//! guessing.
//!
//! The matrix is embedded with [`include_str!`] so a released binary always
//! carries the exact profiles it was tested with.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{Result, VibeError};
use crate::metadata::ModFileMetadata;
use crate::version::VersionRange;

/// The profile matrix compiled into this Vibe build.
pub const EMBEDDED_PROFILES: &str = include_str!("../data/profiles.toml");

/// How much of a profile Vibe can honestly provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileStatus {
    /// Metadata detection, planning, inspection, and content export only.
    MetadataOnly,
    /// A tested clean-room Forge API subset can run simple mods.
    ExperimentalSubset,
}

impl ProfileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata-only",
            Self::ExperimentalSubset => "experimental-subset",
        }
    }

    /// Whether `vibe run` may start a JVM for this profile.
    pub fn allows_runtime(self) -> bool {
        matches!(self, Self::ExperimentalSubset)
    }
}

/// One Minecraft/Forge target and the Vibe backend that handles it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub display_name: String,
    /// The exact Minecraft version this profile targets.
    pub minecraft: String,
    /// The Minecraft versions covered by this profile, as a Maven range.
    pub minecraft_range: String,
    /// A representative Forge version used to evaluate `loaderVersion`.
    pub forge_version: String,
    /// The Forge loader family, as a Maven range.
    pub forge_range: String,
    /// Minimum Java release needed by the target.
    pub java_release: u16,
    /// The Vibe API backend identifier, when one exists.
    #[serde(default)]
    pub backend: Option<String>,
    pub status: ProfileStatus,
    #[serde(default)]
    pub notes: Option<String>,
}

impl Profile {
    pub fn allows_runtime(&self) -> bool {
        self.status.allows_runtime() && self.backend.is_some()
    }
}

/// The machine-readable compatibility matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMatrix {
    pub schema: u32,
    /// Horizon content API this matrix exports to. Not a Horizon engine version.
    pub horizon_content_api: String,
    pub profiles: Vec<Profile>,
}

/// What a mod declares about its target, kept verbatim for reporting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRequirements {
    /// The metadata source that supplied the declarations.
    pub source: String,
    /// The declared Minecraft range, if one was found.
    pub minecraft_range: Option<String>,
    /// The declared Forge loader range, if one was found.
    pub loader_range: Option<String>,
    /// Human-readable declarations, for diagnostics.
    pub declared: Vec<String>,
}

impl ModRequirements {
    pub fn from_metadata(metadata: &ModFileMetadata) -> Self {
        let mut declared = Vec::new();
        if let Some(range) = &metadata.minecraft_version_range {
            declared.push(format!("Minecraft {range}"));
        }
        if let Some(range) = &metadata.forge_version_range {
            declared.push(format!("Forge {range}"));
        }
        Self {
            source: metadata.source.as_str().to_string(),
            minecraft_range: metadata.minecraft_version_range.clone(),
            loader_range: metadata.forge_version_range.clone(),
            declared,
        }
    }

    pub fn has_minecraft(&self) -> bool {
        self.minecraft_range
            .as_deref()
            .map(|range| !range.trim().is_empty())
            .unwrap_or(false)
    }
}

/// The outcome of matching one mod to the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionStatus {
    /// Exactly one profile matched.
    Matched,
    /// More than one profile matched; the user must choose.
    Ambiguous,
    /// No profile matched.
    Unsupported,
    /// The mod does not declare a Minecraft version.
    Undeclared,
}

impl ResolutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Ambiguous => "ambiguous",
            Self::Unsupported => "unsupported",
            Self::Undeclared => "undeclared",
        }
    }
}

/// One profile considered for a mod, with the reason it did or did not match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileCandidate {
    pub id: String,
    pub minecraft: String,
    pub forge_version: String,
    pub status: ProfileStatus,
    pub compatible: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// A mod's target resolution, suitable for JSON output and human reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileResolution {
    pub status: ResolutionStatus,
    /// The chosen profile, only when `status` is `Matched`.
    pub selected: Option<String>,
    pub candidates: Vec<ProfileCandidate>,
    /// Why the resolution failed, or a note about how it succeeded.
    #[serde(default)]
    pub reasons: Vec<String>,
    pub requirements: ModRequirements,
}

impl ProfileResolution {
    pub fn is_matched(&self) -> bool {
        self.status == ResolutionStatus::Matched && self.selected.is_some()
    }
}

struct Evaluation {
    candidate: ProfileCandidate,
    minecraft_ok: bool,
    declared_minecraft: bool,
    loader_ok: bool,
}

impl ProfileMatrix {
    /// Parse and validate the matrix embedded in this build.
    pub fn embedded() -> Result<Self> {
        Self::parse(EMBEDDED_PROFILES)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let matrix: ProfileMatrix = toml::from_str(text)
            .map_err(|e| VibeError::Config(format!("could not parse profile matrix: {e}")))?;
        matrix.validate()?;
        Ok(matrix)
    }

    fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            return Err(VibeError::Config(format!(
                "unsupported profile matrix schema {}; this Vibe build understands schema 1",
                self.schema
            )));
        }
        if self.profiles.is_empty() {
            return Err(VibeError::Config(
                "profile matrix contains no profiles".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for profile in &self.profiles {
            if !ids.insert(profile.id.clone()) {
                return Err(VibeError::Config(format!(
                    "duplicate profile id {:?}",
                    profile.id
                )));
            }
            let minecraft_range = VersionRange::parse(&profile.minecraft_range).map_err(|e| {
                VibeError::Config(format!(
                    "profile {} has an invalid minecraft_range: {e}",
                    profile.id
                ))
            })?;
            if !minecraft_range.matches(&profile.minecraft) {
                return Err(VibeError::Config(format!(
                    "profile {} targets Minecraft {}, outside its declared range {}",
                    profile.id, profile.minecraft, profile.minecraft_range
                )));
            }
            let forge_range = VersionRange::parse(&profile.forge_range).map_err(|e| {
                VibeError::Config(format!(
                    "profile {} has an invalid forge_range: {e}",
                    profile.id
                ))
            })?;
            if !forge_range.matches(&profile.forge_version) {
                return Err(VibeError::Config(format!(
                    "profile {} names Forge {}, outside its declared range {}",
                    profile.id, profile.forge_version, profile.forge_range
                )));
            }
            if profile.status == ProfileStatus::ExperimentalSubset
                && profile
                    .backend
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
            {
                return Err(VibeError::Config(format!(
                    "profile {} is experimental-subset but names no backend",
                    profile.id
                )));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Resolve one mod against the matrix.
    ///
    /// `requested` is an explicit `--profile` choice. It is still checked
    /// against the mod's declarations; an explicit profile never overrides a
    /// declared mismatch.
    pub fn resolve(
        &self,
        requirements: &ModRequirements,
        requested: Option<&str>,
    ) -> ProfileResolution {
        let evaluations: Vec<Evaluation> = self
            .profiles
            .iter()
            .map(|profile| evaluate(profile, requirements))
            .collect();
        let candidates: Vec<ProfileCandidate> = evaluations
            .iter()
            .map(|evaluation| evaluation.candidate.clone())
            .collect();

        if let Some(id) = requested {
            let Some(profile) = self.get(id) else {
                return ProfileResolution {
                    status: ResolutionStatus::Unsupported,
                    selected: None,
                    candidates,
                    reasons: vec![format!(
                        "unknown profile {id:?}; run `vibe profiles` to list supported ids"
                    )],
                    requirements: requirements.clone(),
                };
            };
            let evaluation = evaluations
                .iter()
                .find(|evaluation| evaluation.candidate.id == profile.id)
                .expect("every profile was evaluated");
            let explicit_ok =
                (!evaluation.declared_minecraft || evaluation.minecraft_ok) && evaluation.loader_ok;
            if explicit_ok {
                let mut reasons = Vec::new();
                if !evaluation.declared_minecraft {
                    reasons.push(format!(
                        "profile {} was chosen explicitly; the mod does not declare a Minecraft version",
                        profile.id
                    ));
                }
                return ProfileResolution {
                    status: ResolutionStatus::Matched,
                    selected: Some(profile.id.clone()),
                    candidates,
                    reasons,
                    requirements: requirements.clone(),
                };
            }
            let mut reasons = vec![format!(
                "explicit profile {} is not compatible with this mod",
                profile.id
            )];
            reasons.extend(evaluation.candidate.reasons.clone());
            return ProfileResolution {
                status: ResolutionStatus::Unsupported,
                selected: None,
                candidates,
                reasons,
                requirements: requirements.clone(),
            };
        }

        if !requirements.has_minecraft() {
            let mut reasons = vec![
                "the mod does not declare a Minecraft version; pass --profile to choose explicitly"
                    .to_string(),
            ];
            if requirements.loader_range.is_some() {
                reasons.push(
                    "a Forge loader range alone is not enough to choose a profile".to_string(),
                );
            }
            return ProfileResolution {
                status: ResolutionStatus::Undeclared,
                selected: None,
                candidates,
                reasons,
                requirements: requirements.clone(),
            };
        }

        let matching: Vec<&Evaluation> = evaluations
            .iter()
            .filter(|evaluation| evaluation.candidate.compatible)
            .collect();
        match matching.len() {
            1 => {
                let evaluation = matching[0];
                let mut reasons = Vec::new();
                if !evaluation.candidate.reasons.is_empty() {
                    reasons.extend(evaluation.candidate.reasons.clone());
                }
                ProfileResolution {
                    status: ResolutionStatus::Matched,
                    selected: Some(evaluation.candidate.id.clone()),
                    candidates,
                    reasons,
                    requirements: requirements.clone(),
                }
            }
            0 => {
                let mut reasons = vec![
                    "no supported compatibility profile matches the declared target".to_string(),
                ];
                if !requirements.declared.is_empty() {
                    reasons.push(format!("declared: {}", requirements.declared.join(", ")));
                }
                reasons.push(format!(
                    "available profiles: {}",
                    self.profiles
                        .iter()
                        .map(|profile| format!(
                            "{} (Minecraft {}, Forge {})",
                            profile.id, profile.minecraft, profile.forge_range
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                ProfileResolution {
                    status: ResolutionStatus::Unsupported,
                    selected: None,
                    candidates,
                    reasons,
                    requirements: requirements.clone(),
                }
            }
            _ => {
                let ids: Vec<&str> = matching
                    .iter()
                    .map(|evaluation| evaluation.candidate.id.as_str())
                    .collect();
                ProfileResolution {
                    status: ResolutionStatus::Ambiguous,
                    selected: None,
                    candidates,
                    reasons: vec![format!(
                        "the declared target matches more than one profile ({}); pass --profile <id>",
                        ids.join(", ")
                    )],
                    requirements: requirements.clone(),
                }
            }
        }
    }
}

fn evaluate(profile: &Profile, requirements: &ModRequirements) -> Evaluation {
    let mut reasons = Vec::new();
    let declared_minecraft = requirements.has_minecraft();
    let mut minecraft_ok = false;

    if let Some(range) = &requirements.minecraft_range {
        if range.trim().is_empty() {
            reasons.push("the declared Minecraft range is empty".to_string());
        } else {
            match VersionRange::parse(range) {
                Ok(parsed) => {
                    minecraft_ok = parsed.matches(&profile.minecraft);
                    if !minecraft_ok {
                        reasons.push(format!(
                            "declares Minecraft {range}; the profile targets Minecraft {}",
                            profile.minecraft
                        ));
                    }
                }
                Err(_) => reasons.push(format!(
                    "declares Minecraft range {range:?}, which Vibe cannot parse"
                )),
            }
        }
    } else {
        reasons.push("does not declare a Minecraft version".to_string());
    }

    let mut loader_ok = true;
    if let Some(range) = &requirements.loader_range {
        if !range.trim().is_empty() {
            match VersionRange::parse(range) {
                Ok(parsed) => {
                    loader_ok = parsed.matches(&profile.forge_version);
                    if !loader_ok {
                        reasons.push(format!(
                            "requires Forge {range}; the profile provides Forge {}",
                            profile.forge_version
                        ));
                    }
                }
                Err(_) => {
                    loader_ok = false;
                    reasons.push(format!(
                        "declares Forge range {range:?}, which Vibe cannot parse"
                    ));
                }
            }
        }
    }

    let compatible = declared_minecraft && minecraft_ok && loader_ok;
    Evaluation {
        candidate: ProfileCandidate {
            id: profile.id.clone(),
            minecraft: profile.minecraft.clone(),
            forge_version: profile.forge_version.clone(),
            status: profile.status,
            compatible,
            reasons,
        },
        minecraft_ok,
        declared_minecraft,
        loader_ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{parse_mcmod_info, parse_mods_toml};

    fn requirements(minecraft: Option<&str>, loader: Option<&str>) -> ModRequirements {
        let mut declared = Vec::new();
        if let Some(range) = minecraft {
            declared.push(format!("Minecraft {range}"));
        }
        if let Some(range) = loader {
            declared.push(format!("Forge {range}"));
        }
        ModRequirements {
            source: "mods.toml".into(),
            minecraft_range: minecraft.map(str::to_string),
            loader_range: loader.map(str::to_string),
            declared,
        }
    }

    #[test]
    fn embedded_matrix_is_valid_and_has_the_expected_targets() {
        let matrix = ProfileMatrix::embedded().unwrap();
        assert_eq!(matrix.schema, 1);
        assert_eq!(matrix.horizon_content_api, "0.2");
        let ids: Vec<&str> = matrix.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["forge-1.20.1-47", "forge-1.21.1-52", "forge-26.3-66"]
        );
        assert!(matrix.get("forge-1.20.1-47").unwrap().allows_runtime());
        assert!(!matrix.get("forge-26.3-66").unwrap().allows_runtime());
    }

    #[test]
    fn resolves_an_exact_minecraft_target() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(Some("[1.21.1,1.22)"), Some("[52,)")), None);
        assert_eq!(resolution.status, ResolutionStatus::Matched);
        assert_eq!(resolution.selected.as_deref(), Some("forge-1.21.1-52"));
    }

    #[test]
    fn loader_range_disambiguates_a_wide_minecraft_range() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(Some("[1.20,)"), Some("[47,48)")), None);
        assert_eq!(resolution.status, ResolutionStatus::Matched);
        assert_eq!(resolution.selected.as_deref(), Some("forge-1.20.1-47"));
    }

    #[test]
    fn a_wide_range_without_a_loader_range_is_ambiguous() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(Some("[1.20,)"), None), None);
        assert_eq!(resolution.status, ResolutionStatus::Ambiguous);
        assert!(resolution.reasons[0].contains("--profile"));
    }

    #[test]
    fn an_unsupported_target_reports_the_declaration() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(Some("[1.12,1.13)"), None), None);
        assert_eq!(resolution.status, ResolutionStatus::Unsupported);
        assert!(resolution.reasons[0].contains("no supported"));
        assert!(resolution
            .reasons
            .iter()
            .any(|reason| reason.contains("1.12,1.13")));
    }

    #[test]
    fn missing_minecraft_is_undeclared_even_with_a_loader_range() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(None, Some("[52,)")), None);
        assert_eq!(resolution.status, ResolutionStatus::Undeclared);
        assert!(resolution.reasons.iter().any(|r| r.contains("--profile")));
    }

    #[test]
    fn an_explicit_profile_is_checked_not_assumed() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let mismatch = matrix.resolve(
            &requirements(Some("[1.21.1,1.22)"), Some("[52,)")),
            Some("forge-1.20.1-47"),
        );
        assert_eq!(mismatch.status, ResolutionStatus::Unsupported);
        assert!(mismatch.reasons[0].contains("explicit profile"));

        let match_ = matrix.resolve(
            &requirements(Some("[1.21.1,1.22)"), Some("[52,)")),
            Some("forge-1.21.1-52"),
        );
        assert_eq!(match_.status, ResolutionStatus::Matched);
        assert_eq!(match_.selected.as_deref(), Some("forge-1.21.1-52"));
    }

    #[test]
    fn an_explicit_profile_can_resolve_a_mod_without_a_minecraft_declaration() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution =
            matrix.resolve(&requirements(None, Some("[52,)")), Some("forge-1.21.1-52"));
        assert_eq!(resolution.status, ResolutionStatus::Matched);
        assert!(resolution
            .reasons
            .iter()
            .any(|r| r.contains("does not declare")));
    }

    #[test]
    fn unknown_explicit_profile_is_rejected() {
        let matrix = ProfileMatrix::embedded().unwrap();
        let resolution = matrix.resolve(&requirements(Some("[1.21.1]"), None), Some("nope"));
        assert_eq!(resolution.status, ResolutionStatus::Unsupported);
        assert!(resolution.reasons[0].contains("unknown profile"));
    }

    #[test]
    fn metadata_requirements_come_from_parsed_mods_toml() {
        let parsed = parse_mods_toml(
            r#"
modLoader = "javafml"
loaderVersion = "[52,)"
license = "MIT"
[[mods]]
modId = "example"
version = "1"
[[dependencies.example]]
modId = "minecraft"
mandatory = true
versionRange = "[1.21.1,1.22)"
"#,
            None,
        )
        .unwrap();
        let requirements = ModRequirements::from_metadata(&parsed);
        assert_eq!(requirements.source, "mods.toml");
        assert_eq!(
            requirements.minecraft_range.as_deref(),
            Some("[1.21.1,1.22)")
        );
        assert_eq!(requirements.loader_range.as_deref(), Some("[52,)"));
    }

    #[test]
    fn metadata_requirements_come_from_legacy_mcmod_info() {
        let parsed = parse_mcmod_info(
            r#"[{"modid":"legacy","name":"Legacy","version":"1","mcversion":"1.12.2"}]"#,
        )
        .unwrap();
        let requirements = ModRequirements::from_metadata(&parsed);
        assert_eq!(requirements.source, "mcmod.info");
        assert_eq!(requirements.minecraft_range.as_deref(), Some("1.12.2"));
        let resolution = ProfileMatrix::embedded()
            .unwrap()
            .resolve(&requirements, None);
        assert_eq!(resolution.status, ResolutionStatus::Unsupported);
    }

    #[test]
    fn conflicting_minecraft_declarations_are_not_guessed() {
        let parsed = parse_mods_toml(
            r#"
modLoader = "javafml"
loaderVersion = "[52,)"
[[mods]]
modId = "first"
version = "1"
[[mods]]
modId = "second"
version = "1"
[[dependencies.first]]
modId = "minecraft"
versionRange = "[1.21.1,1.22)"
[[dependencies.second]]
modId = "minecraft"
versionRange = "[1.20.1,1.20.2)"
"#,
            None,
        )
        .unwrap();
        assert_eq!(parsed.minecraft_version_range, None);
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("conflicting Minecraft")));
    }
}
