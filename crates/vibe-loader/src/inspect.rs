use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Take};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use vibe_core::analysis::{analyze, AnalysisFacts, CompatibilityReport};
use vibe_core::classfile::{parse_class, ClassFileFacts};
use vibe_core::error::{Result, VibeError};
use vibe_core::metadata::{
    inferred_metadata, parse_mcmod_info, parse_mods_toml, LoaderKind, ModFileMetadata,
};

use crate::manifest::{parse_manifest, Manifest};

const MAX_CLASS_BYTES: u64 = 32 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_NESTED_JAR_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JarInspection {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub manifest: BTreeMap<String, String>,
    pub metadata: ModFileMetadata,
    pub report: CompatibilityReport,
    pub class_count: usize,
    pub entrypoints: Vec<String>,
    pub entrypoint_mod_ids: BTreeMap<String, Vec<String>>,
    pub nested_jars: Vec<String>,
    pub mixin_configs: Vec<String>,
    pub access_transformers: Vec<String>,
    pub core_plugin: Option<String>,
    pub service_files: Vec<String>,
    pub has_assets: bool,
    pub has_data: bool,
    pub asset_namespaces: Vec<String>,
    pub data_namespaces: Vec<String>,
}

impl JarInspection {
    pub fn mod_ids(&self) -> Vec<String> {
        self.metadata
            .mods
            .iter()
            .map(|m| m.mod_id.to_string())
            .collect()
    }

    pub fn primary_mod_id(&self) -> String {
        self.metadata
            .single_id()
            .map(|id| id.to_string())
            .or_else(|| self.metadata.mods.first().map(|m| m.mod_id.to_string()))
            .unwrap_or_else(|| {
                vibe_core::ModId::sanitize_lossy(
                    self.path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown"),
                )
                .to_string()
            })
    }

    pub fn summary(&self) -> JarSummary {
        JarSummary {
            path: self.path.clone(),
            file_name: self.file_name.clone(),
            mod_ids: self.mod_ids(),
            verdict: self.report.verdict,
            loader: self.metadata.loader.clone(),
            entrypoints: self.entrypoints.clone(),
            error_count: self.report.errors().count(),
            warning_count: self.report.warnings().count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JarSummary {
    pub path: PathBuf,
    pub file_name: String,
    pub mod_ids: Vec<String>,
    pub verdict: vibe_core::Verdict,
    pub loader: LoaderKind,
    pub entrypoints: Vec<String>,
    pub error_count: usize,
    pub warning_count: usize,
}

/// Inspect one jar read-only.
pub fn inspect_jar(path: &Path, runtime_java_release: Option<u16>) -> Result<JarInspection> {
    if !path.exists() {
        return Err(VibeError::MissingPath(path.to_path_buf()));
    }
    let size_bytes = std::fs::metadata(path)?.len();
    let sha256 = hash_file(path)?;
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| VibeError::Archive(format!("{}: {e}", path.display())))?;

    let mut manifest = Manifest::default();
    let mut mods_toml = None;
    let mut mcmod_info = None;
    let mut classes = Vec::new();
    let mut provided_classes = BTreeSet::new();
    let mut class_errors = Vec::new();
    let mut nested_jars = BTreeSet::new();
    let mut service_files = BTreeSet::new();
    let mut native_libraries = BTreeSet::new();
    let mut has_assets = false;
    let mut has_data = false;
    let mut asset_namespaces = BTreeSet::new();
    let mut data_namespaces = BTreeSet::new();
    let mut access_transformers = BTreeSet::new();
    let mut mixin_configs = BTreeSet::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| VibeError::Archive(format!("{}: {e}", path.display())))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let lower = name.to_ascii_lowercase();

        if lower == "meta-inf/manifest.mf" {
            let text = read_limited(&mut entry, MAX_METADATA_BYTES)?;
            manifest = parse_manifest(&String::from_utf8_lossy(&text));
            continue;
        }
        if lower == "meta-inf/neoforge.mods.toml"
            || lower == "meta-inf/mods.toml"
            || lower == "meta-inf/mods.toml"
        {
            let text = read_limited(&mut entry, MAX_METADATA_BYTES)?;
            // Prefer NeoForge metadata when both files exist; they describe the
            // same archive and Forge's loader would see only one of them.
            if lower == "meta-inf/neoforge.mods.toml" || mods_toml.is_none() {
                mods_toml = Some(String::from_utf8_lossy(&text).into_owned());
            }
            continue;
        }
        if lower == "mcmod.info" {
            let text = read_limited(&mut entry, MAX_METADATA_BYTES)?;
            mcmod_info = Some(String::from_utf8_lossy(&text).into_owned());
            continue;
        }
        if lower.ends_with(".class") {
            if is_ignored_class(&name) {
                continue;
            }
            if let Some(stem) = name.strip_suffix(".class") {
                provided_classes.insert(stem.replace('/', "."));
            }
            let bytes = read_limited(&mut entry, MAX_CLASS_BYTES)?;
            match parse_class(&bytes) {
                Ok(facts) => classes.push(facts),
                Err(err) => class_errors.push(format!("{name}: {err}")),
            }
            continue;
        }
        if (name.starts_with("META-INF/jarjar/") || name.starts_with("META-INF/jars/"))
            && lower.ends_with(".jar")
        {
            nested_jars.insert(name);
            // Nested archives are real dependencies at runtime, so classes
            // inside them must count as provided. Otherwise every reference to
            // a bundled library would be misreported as external.
            if let Ok(bytes) = read_limited(&mut entry, MAX_NESTED_JAR_BYTES) {
                if let Ok(mut nested) = ZipArchive::new(std::io::Cursor::new(bytes)) {
                    for nested_index in 0..nested.len() {
                        let Ok(nested_entry) = nested.by_index(nested_index) else {
                            continue;
                        };
                        let nested_name = nested_entry.name().to_string();
                        if !nested_name.ends_with(".class") || is_ignored_class(&nested_name) {
                            continue;
                        }
                        if let Some(stem) = nested_name.strip_suffix(".class") {
                            provided_classes.insert(stem.replace('/', "."));
                        }
                    }
                }
            }
            continue;
        }
        if name.starts_with("META-INF/services/") {
            service_files.insert(name);
            continue;
        }
        if lower == "meta-inf/accesstransformer.cfg" {
            access_transformers.insert(name);
            continue;
        }
        if is_native_library(&lower) {
            native_libraries.insert(name.clone());
        }
        if let Some(namespace) = namespace_of(&name, "assets/") {
            has_assets = true;
            asset_namespaces.insert(namespace);
        }
        if let Some(namespace) = namespace_of(&name, "data/") {
            has_data = true;
            data_namespaces.insert(namespace);
        }
    }

    for config in manifest.split_values("MixinConfigs") {
        mixin_configs.insert(config);
    }
    for config in manifest.split_values("FMLAT") {
        access_transformers.insert(config);
    }
    if let Some(value) = manifest.get("MixinConfigs") {
        for config in value.split(',') {
            if !config.trim().is_empty() {
                mixin_configs.insert(config.trim().to_string());
            }
        }
    }

    let jar_version = manifest
        .get("Implementation-Version")
        .or_else(|| manifest.get("Specification-Version"));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let metadata = if let Some(text) = &mods_toml {
        parse_mods_toml(text, jar_version)?
    } else if let Some(text) = &mcmod_info {
        parse_mcmod_info(text)?
    } else {
        inferred_metadata(stem, jar_version)
    };

    let core_plugin = manifest
        .get("FMLCorePlugin")
        .map(str::to_string)
        .or_else(|| {
            manifest
                .get("FMLCorePluginContainsFMLMod")
                .map(|_| "(declared via FMLCorePluginContainsFMLMod)".to_string())
        });

    let mut entrypoints: Vec<String> = classes
        .iter()
        .filter(|class| class.mod_annotation.is_some())
        .map(|class| class.name.clone())
        .collect();
    entrypoints.sort();
    let entrypoint_mod_ids: BTreeMap<String, Vec<String>> = classes
        .iter()
        .filter_map(|class| {
            let annotation = class.mod_annotation.as_ref()?;
            let mut ids = Vec::new();
            if let Some(id) = annotation.value.as_ref().filter(|id| !id.is_empty()) {
                ids.push(id.clone());
            }
            if let Some(id) = annotation.modid.as_ref().filter(|id| !id.is_empty()) {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            Some((class.name.clone(), ids))
        })
        .collect();

    let facts = AnalysisFacts {
        loader: if metadata.source == vibe_core::MetadataSource::Inferred {
            None
        } else {
            Some(metadata.loader.clone())
        },
        classes: classes.clone(),
        provided_classes,
        class_errors,
        nested_jars: nested_jars.iter().cloned().collect(),
        mixin_configs: mixin_configs.iter().cloned().collect(),
        access_transformers: access_transformers.iter().cloned().collect(),
        core_plugin: core_plugin.clone(),
        native_libraries: native_libraries.iter().cloned().collect(),
        has_assets,
        has_data,
        runtime_java_release,
    };
    let report = analyze(&facts);

    Ok(JarInspection {
        path: path.to_path_buf(),
        file_name: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string(),
        size_bytes,
        sha256,
        manifest: manifest.as_map().clone(),
        metadata,
        report,
        class_count: classes.len(),
        entrypoints,
        entrypoint_mod_ids,
        nested_jars: nested_jars.into_iter().collect(),
        mixin_configs: mixin_configs.into_iter().collect(),
        access_transformers: access_transformers.into_iter().collect(),
        core_plugin,
        service_files: service_files.into_iter().collect(),
        has_assets,
        has_data,
        asset_namespaces: asset_namespaces.into_iter().collect(),
        data_namespaces: data_namespaces.into_iter().collect(),
    })
}

fn is_ignored_class(name: &str) -> bool {
    name.ends_with("module-info.class")
        || name.ends_with("package-info.class")
        || name.starts_with("META-INF/versions/")
}

fn is_native_library(lower_name: &str) -> bool {
    [".dll", ".so", ".dylib", ".jnilib"]
        .iter()
        .any(|ext| lower_name.ends_with(ext))
}

fn namespace_of(name: &str, prefix: &str) -> Option<String> {
    let rest = name.strip_prefix(prefix)?;
    let mut parts = rest.split('/');
    let first = parts.next()?;
    let second = parts.next()?;
    if first.is_empty() || second.is_empty() {
        return None;
    }
    Some(first.to_string())
}

fn read_limited<R: Read>(reader: &mut R, limit: u64) -> Result<Vec<u8>> {
    let mut limited: Take<&mut R> = reader.take(limit + 1);
    let mut out = Vec::new();
    limited.read_to_end(&mut out)?;
    if out.len() as u64 > limit {
        return Err(VibeError::Archive(format!(
            "archive entry exceeds the {limit}-byte inspection limit"
        )));
    }
    Ok(out)
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// A compact map of parsed class facts, useful for tests and diagnostics.
pub fn class_facts(path: &Path) -> Result<Vec<ClassFileFacts>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| VibeError::Archive(format!("{}: {e}", path.display())))?;
    let mut out = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| VibeError::Archive(format!("{}: {e}", path.display())))?;
        let name = entry.name().to_string();
        if !name.ends_with(".class") || is_ignored_class(&name) {
            continue;
        }
        let bytes = read_limited(&mut entry, MAX_CLASS_BYTES)?;
        if let Ok(facts) = parse_class(&bytes) {
            out.push(facts);
        }
    }
    Ok(out)
}
