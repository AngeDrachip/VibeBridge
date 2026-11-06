use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, VibeError};
use crate::ids::ModId;

/// The Forge language loader declared by a mod file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderKind {
    JavaFml,
    LowCodeFml,
    Legacy,
    Unknown(String),
}

impl LoaderKind {
    pub fn from_mods_toml(value: Option<&str>) -> Self {
        match value {
            Some("javafml") => Self::JavaFml,
            Some("lowcodefml") | Some("nocodefml") => Self::LowCodeFml,
            Some(other) => Self::Unknown(other.to_string()),
            None => Self::Legacy,
        }
    }

    pub fn needs_java_entrypoint(&self) -> bool {
        matches!(self, Self::JavaFml | Self::Legacy)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModSide {
    Client,
    Server,
    #[default]
    Both,
    Unknown,
}

impl ModSide {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "CLIENT" => Self::Client,
            "SERVER" => Self::Server,
            "BOTH" => Self::Both,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    ModsToml,
    McModInfo,
    ManifestOnly,
    Inferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub mod_id: String,
    pub mandatory: bool,
    pub version_range: String,
    pub ordering: String,
    pub side: ModSide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModMetadata {
    pub mod_id: ModId,
    pub version: String,
    pub display_name: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub display_url: Option<String>,
    pub logo_file: Option<String>,
    pub side: ModSide,
    pub client_side_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModFileMetadata {
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub license: Option<String>,
    pub source: MetadataSource,
    pub mods: Vec<ModMetadata>,
    pub dependencies: BTreeMap<String, Vec<Dependency>>,
    pub warnings: Vec<String>,
}

impl ModFileMetadata {
    pub fn single_id(&self) -> Option<&ModId> {
        if self.mods.len() == 1 {
            self.mods.first().map(|m| &m.mod_id)
        } else {
            None
        }
    }
}

/// Parse a Forge `META-INF/mods.toml` (or `neoforge.mods.toml`).
///
/// `jar_version` is substituted for `${file.jarVersion}`, matching Forge's
/// documented behaviour.
pub fn parse_mods_toml(text: &str, jar_version: Option<&str>) -> Result<ModFileMetadata> {
    let value: toml::Value = text
        .parse()
        .map_err(|e| VibeError::Metadata(format!("could not parse mods.toml: {e}")))?;

    let table = value
        .as_table()
        .ok_or_else(|| VibeError::Metadata("mods.toml must be a TOML table".into()))?;

    let mod_loader = table
        .get("modLoader")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let loader = LoaderKind::from_mods_toml(mod_loader.as_deref());
    let loader_version = table
        .get("loaderVersion")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let license = table
        .get("license")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let client_side_only = table
        .get("clientSideOnly")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let file_properties: BTreeMap<String, String> = table
        .get("properties")
        .and_then(toml::Value::as_table)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let mut warnings = Vec::new();
    let mut mods = Vec::new();
    let mod_entries = table
        .get("mods")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| VibeError::Metadata("mods.toml has no [[mods]] array".into()))?;

    for (index, entry) in mod_entries.iter().enumerate() {
        let entry = entry.as_table().ok_or_else(|| {
            VibeError::Metadata(format!("mods.toml [[mods]] entry {index} is not a table"))
        })?;
        let raw_id = entry
            .get("modId")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| {
                VibeError::Metadata(format!("mods.toml [[mods]] entry {index} has no modId"))
            })?;
        let mod_id = match ModId::parse(raw_id) {
            Ok(id) => id,
            Err(_) => {
                warnings.push(format!(
                    "mod id {raw_id:?} does not match Forge's documented id pattern; it is preserved as-is for reporting"
                ));
                ModId::sanitize_lossy(raw_id)
            }
        };

        let raw_version = entry
            .get("version")
            .and_then(toml::Value::as_str)
            .unwrap_or("1");
        let version = substitute(raw_version, jar_version, &file_properties);

        let description = entry
            .get("description")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        let authors = entry
            .get("authors")
            .and_then(toml::Value::as_str)
            .map(|s| {
                s.split(',')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let side = entry
            .get("side")
            .and_then(toml::Value::as_str)
            .map(ModSide::parse)
            .unwrap_or(if client_side_only {
                ModSide::Client
            } else {
                ModSide::Both
            });

        mods.push(ModMetadata {
            mod_id,
            version,
            display_name: entry
                .get("displayName")
                .and_then(toml::Value::as_str)
                .unwrap_or(raw_id)
                .to_string(),
            description,
            authors,
            display_url: entry
                .get("displayURL")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            logo_file: entry
                .get("logoFile")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            side,
            client_side_only,
        });
    }

    let mut dependencies: BTreeMap<String, Vec<Dependency>> = BTreeMap::new();
    if let Some(deps_table) = table.get("dependencies").and_then(toml::Value::as_table) {
        for (owner, deps) in deps_table {
            let Some(entries) = deps.as_array() else {
                warnings.push(format!("dependencies.{owner} is not an array of tables"));
                continue;
            };
            let mut parsed = Vec::new();
            for dep in entries {
                let Some(dep) = dep.as_table() else {
                    warnings.push(format!("dependencies.{owner} contains a non-table entry"));
                    continue;
                };
                let Some(mod_id) = dep.get("modId").and_then(toml::Value::as_str) else {
                    warnings.push(format!("dependencies.{owner} entry has no modId"));
                    continue;
                };
                parsed.push(Dependency {
                    mod_id: mod_id.to_string(),
                    mandatory: dep
                        .get("mandatory")
                        .and_then(toml::Value::as_bool)
                        .unwrap_or(true),
                    version_range: dep
                        .get("versionRange")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    ordering: dep
                        .get("ordering")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("NONE")
                        .to_string(),
                    side: dep
                        .get("side")
                        .and_then(toml::Value::as_str)
                        .map(ModSide::parse)
                        .unwrap_or(ModSide::Both),
                });
            }
            dependencies.insert(owner.clone(), parsed);
        }
    }

    Ok(ModFileMetadata {
        loader,
        loader_version,
        license,
        source: MetadataSource::ModsToml,
        mods,
        dependencies,
        warnings,
    })
}

/// Parse the legacy `mcmod.info` format (Minecraft Forge 1.12 and earlier).
/// Both a bare array and an object with a `modList` array are accepted.
pub fn parse_mcmod_info(text: &str) -> Result<ModFileMetadata> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| VibeError::Metadata(format!("could not parse mcmod.info: {e}")))?;
    let entries = if let Some(array) = value.as_array() {
        array.as_slice()
    } else if let Some(array) = value.get("modList").and_then(|v| v.as_array()) {
        array.as_slice()
    } else {
        return Err(VibeError::Metadata(
            "mcmod.info is neither an array nor an object with modList".into(),
        ));
    };

    let mut warnings = Vec::new();
    let mut mods = Vec::new();
    for entry in entries {
        let Some(obj) = entry.as_object() else {
            warnings.push("mcmod.info contains a non-object entry".into());
            continue;
        };
        let raw_id = obj
            .get("modid")
            .or_else(|| obj.get("modId"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let Ok(mod_id) = ModId::parse(raw_id) else {
            warnings.push(format!(
                "mcmod.info mod id {raw_id:?} is not a valid Forge id"
            ));
            continue;
        };
        let authors = obj
            .get("authorList")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .or_else(|| {
                obj.get("authors")
                    .and_then(|v| v.as_str())
                    .map(|s| vec![s.to_string()])
            })
            .unwrap_or_default();
        mods.push(ModMetadata {
            mod_id,
            version: obj
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1")
                .to_string(),
            display_name: obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(raw_id)
                .to_string(),
            description: obj
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            authors,
            display_url: obj.get("url").and_then(|v| v.as_str()).map(str::to_string),
            logo_file: obj
                .get("logoFile")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            side: if obj
                .get("clientSideOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                ModSide::Client
            } else {
                ModSide::Both
            },
            client_side_only: obj
                .get("clientSideOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    }

    Ok(ModFileMetadata {
        loader: LoaderKind::Legacy,
        loader_version: None,
        license: None,
        source: MetadataSource::McModInfo,
        mods,
        dependencies: BTreeMap::new(),
        warnings,
    })
}

/// Build a best-effort metadata record for a jar that has no Forge metadata.
pub fn inferred_metadata(name: &str, manifest_version: Option<&str>) -> ModFileMetadata {
    let id = ModId::sanitize_lossy(name);
    ModFileMetadata {
        loader: LoaderKind::Unknown("none".into()),
        loader_version: None,
        license: None,
        source: MetadataSource::Inferred,
        mods: vec![ModMetadata {
            mod_id: id.clone(),
            version: manifest_version.unwrap_or("0.0.0").to_string(),
            display_name: name.to_string(),
            description: None,
            authors: Vec::new(),
            display_url: None,
            logo_file: None,
            side: ModSide::Both,
            client_side_only: false,
        }],
        dependencies: BTreeMap::new(),
        warnings: vec![format!(
            "no Forge metadata found; inferred mod id {id} from the file name"
        )],
    }
}

fn substitute(
    template: &str,
    jar_version: Option<&str>,
    properties: &BTreeMap<String, String>,
) -> String {
    let mut out = template.to_string();
    if let Some(version) = jar_version {
        out = out.replace("${file.jarVersion}", version);
    }
    for (key, value) in properties {
        out = out.replace(&format!("${{file.{key}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODS_TOML: &str = r#"
modLoader = "javafml"
loaderVersion = "[52,)"
license = "MIT"
clientSideOnly = false

[[mods]]
modId = "examplemod"
version = "${file.jarVersion}"
displayName = "Example Mod"
authors = "Ada, Grace"
description = "A test mod"

[[dependencies.examplemod]]
modId = "forge"
mandatory = true
versionRange = "[52,)"
ordering = "NONE"
side = "BOTH"

[[dependencies.examplemod]]
modId = "minecraft"
mandatory = true
versionRange = "[1.21.1,)"
ordering = "NONE"
side = "BOTH"
"#;

    #[test]
    fn parses_mods_toml_and_dependencies() {
        let parsed = parse_mods_toml(MODS_TOML, Some("1.2.3")).unwrap();
        assert_eq!(parsed.loader, LoaderKind::JavaFml);
        assert_eq!(parsed.license.as_deref(), Some("MIT"));
        assert_eq!(parsed.mods.len(), 1);
        assert_eq!(parsed.mods[0].mod_id.as_str(), "examplemod");
        assert_eq!(parsed.mods[0].version, "1.2.3");
        assert_eq!(parsed.mods[0].authors, vec!["Ada", "Grace"]);
        let deps = &parsed.dependencies["examplemod"];
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().all(|d| d.mandatory));
    }

    #[test]
    fn parses_legacy_mcmod_info() {
        let parsed = parse_mcmod_info(
            r#"[{"modid":"legacy_mod","name":"Legacy","version":"1.0","authorList":["A"]}]"#,
        )
        .unwrap();
        assert_eq!(parsed.loader, LoaderKind::Legacy);
        assert_eq!(parsed.mods[0].mod_id.as_str(), "legacy_mod");
    }

    #[test]
    fn rejects_mods_toml_without_mods() {
        assert!(parse_mods_toml("modLoader = \"javafml\"", None).is_err());
    }
}
