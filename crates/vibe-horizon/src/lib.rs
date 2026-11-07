//! Documented Horizon integration.
//!
//! Two independent surfaces are implemented here, both based only on
//! Horizon's public documentation:
//!
//! * [`probe`] locates `horizon-ctl` and asks it for its help text and game
//!   state. It never speaks an undocumented protocol.
//! * [`export_content`] copies a Forge jar's resources into a content mod
//!   (`.hmod`) bundle: a zip holding `horizon-mod.toml`, `assets/` and `data/`.
//!   The source jar is opened read-only and is never modified.
//!
//! Horizon does **not** run Java mods. A content export carries data and assets
//! only; code, registrations written in Java, Mixins and coremods are outside
//! what a content mod can represent. This crate reports that boundary instead
//! of pretending the original jar can run.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use vibe_core::error::{Result, VibeError};
use vibe_core::metadata::ModSide;
use vibe_core::ModId;
use vibe_loader::{inspect_jar, JarInspection};

/// Horizon's content-mod API version implemented by this exporter.
pub const CONTENT_API: &str = "0.2";
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 65_536;
const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PACK_META_BYTES: u64 = 1024 * 1024;
/// Enough detail for a user to understand a large jar without turning one
/// export report into tens of thousands of lines.
const MAX_SKIPPED_DETAILS: usize = 512;
const MANIFEST_NAME: &str = "horizon-mod.toml";
/// Every key Horizon's content-mod manifest documents. Unknown keys are refused
/// by Horizon, so validation reports them too.
const MANIFEST_KEYS: &[&str] = &[
    "id",
    "name",
    "version",
    "description",
    "authors",
    "api",
    "side",
    "capabilities",
    "namespaces",
];

// ---------------------------------------------------------------------------
// horizon-ctl discovery and probing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizonProbe {
    /// The discovered `horizon-ctl` executable, if any.
    pub ctl: Option<PathBuf>,
    /// Whether `horizon-ctl --help` ran successfully.
    pub help_ok: bool,
    /// First non-empty help line, useful for identifying the build.
    pub help_summary: Option<String>,
    /// Whether a running game answered `horizon-ctl state`.
    pub game_reachable: bool,
    /// Parsed state reply when the game answered with JSON.
    pub game_state: Option<serde_json::Value>,
    /// Human-readable observations. These are diagnostics, not compatibility
    /// claims.
    pub notes: Vec<String>,
}

/// Locate `horizon-ctl` from an explicit path, environment variables, `PATH`,
/// or a few conventional install folders.
pub fn locate_ctl(preferred: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = preferred {
        return resolve_executable(path);
    }
    for variable in ["VIBE_HORIZON_CTL", "HORIZON_CTL"] {
        if let Ok(value) = std::env::var(variable) {
            if !value.trim().is_empty() {
                if let Some(path) = resolve_executable(Path::new(value.trim())) {
                    return Some(path);
                }
            }
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for name in executable_names("horizon-ctl") {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut roots = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(local.clone()).join("horizon"));
        roots.push(PathBuf::from(local).join("Programs").join("horizon"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        roots.push(PathBuf::from(appdata).join("horizon"));
    }
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        roots.push(PathBuf::from(home).join(".horizon"));
    }
    for root in roots {
        for candidate in [
            root.join(exe_name("horizon-ctl")),
            root.join("bin").join(exe_name("horizon-ctl")),
            root.join("game").join(exe_name("horizon-ctl")),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Probe `horizon-ctl` without requiring a running game.
pub fn probe(preferred: Option<&Path>) -> HorizonProbe {
    let mut result = HorizonProbe {
        ctl: None,
        help_ok: false,
        help_summary: None,
        game_reachable: false,
        game_state: None,
        notes: Vec::new(),
    };
    let Some(ctl) = locate_ctl(preferred) else {
        result.notes.push(
            "horizon-ctl was not found. Install Horizon, put horizon-ctl on PATH, or pass --ctl."
                .into(),
        );
        return result;
    };
    result.ctl = Some(ctl.clone());

    match std::process::Command::new(&ctl).arg("--help").output() {
        Ok(output) => {
            result.help_ok = output.status.success();
            let text = String::from_utf8_lossy(&output.stdout);
            result.help_summary = text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_string);
            if !result.help_ok {
                let error = String::from_utf8_lossy(&output.stderr);
                result.notes.push(format!(
                    "`horizon-ctl --help` exited with {}: {}",
                    output.status,
                    error.trim()
                ));
            }
        }
        Err(error) => {
            result
                .notes
                .push(format!("could not run {} --help: {error}", ctl.display()));
            return result;
        }
    }

    match std::process::Command::new(&ctl).arg("state").output() {
        Ok(output) if output.status.success() => {
            result.game_reachable = true;
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim()) {
                result.game_state = Some(value);
            }
        }
        Ok(output) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let hint = serde_json::from_str::<serde_json::Value>(text.trim())
                .ok()
                .and_then(|value| {
                    value
                        .get("kind")
                        .and_then(|kind| kind.as_str())
                        .map(str::to_string)
                });
            result.notes.push(match hint.as_deref() {
                Some("connection") => {
                    "horizon-ctl is installed but no running game answered; start a game under control to use the control surface.".into()
                }
                Some(kind) => format!("horizon-ctl state failed (kind: {kind})."),
                None => "horizon-ctl state failed; no running game answered.".into(),
            });
        }
        Err(error) => result
            .notes
            .push(format!("could not run {} state: {error}", ctl.display())),
    }
    result
}

// ---------------------------------------------------------------------------
// Content-mod export
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    /// Override the id written to the manifest. Required when the jar declares
    /// more than one mod.
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    /// `client`, `server` or `both`. Defaults to the jar's declared side.
    pub side: Option<String>,
    /// Extra namespaces to claim besides the mod id. Horizon refuses a content
    /// mod whose files sit in a namespace it does not own.
    pub namespaces: Vec<String>,
    /// Overwrite an existing output file.
    pub force: bool,
    /// Include `data/minecraft/tags/` additions (Horizon permits these).
    pub include_vanilla_tags: bool,
    /// Include `data/c/tags/` convention-tag additions.
    pub include_convention_tags: bool,
    /// Include `assets/minecraft/atlases/` sources.
    pub include_atlases: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            id: None,
            name: None,
            version: None,
            side: None,
            namespaces: Vec::new(),
            force: false,
            include_vanilla_tags: true,
            include_convention_tags: true,
            include_atlases: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentExport {
    pub source: PathBuf,
    pub source_sha256: String,
    pub output: PathBuf,
    pub id: String,
    pub name: String,
    pub version: String,
    pub side: String,
    /// The namespaces actually written into the bundle, mod id first.
    pub namespaces: Vec<String>,
    /// Namespaces present in the jar that were not claimed and were left out.
    pub outside_namespaces: Vec<String>,
    pub files: usize,
    pub bytes: u64,
    /// Content files left out because their namespace is not claimed.
    pub excluded_files: usize,
    /// Non-content entries (classes, META-INF, other resources) ignored.
    pub ignored_entries: usize,
    pub file_counts: BTreeMap<String, usize>,
    /// Every file written into the bundle, with its kind and size.
    pub entries: Vec<ExportEntry>,
    /// Up to 512 entries that were not carried, with why. Counts above are
    /// complete even when this list is truncated.
    #[serde(default)]
    pub skipped: Vec<SkippedEntry>,
    #[serde(default)]
    pub skipped_truncated: bool,
    /// `pack.mcmeta` declarations found at the jar root, copied or not.
    #[serde(default)]
    pub source_pack_formats: Vec<SourcePackFormat>,
    /// The mod's declared `minecraft` dependency range, when it has one.
    #[serde(default)]
    pub minecraft_version_range: Option<String>,
    pub manifest: String,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEntry {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub path: String,
    /// Why the file was not carried: `unclaimed_namespace`, `unsafe_path`, or
    /// `not_content`.
    pub reason: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePackFormat {
    pub path: String,
    #[serde(default)]
    pub pack_format: Option<i64>,
    #[serde(default)]
    pub supported_formats: Option<serde_json::Value>,
}

/// The result of checking an existing `.hmod` bundle against the documented
/// Horizon content-mod rules Vibe knows how to verify.
///
/// This is not a replacement for Horizon's own `cargo horizon-mod check`; it
/// covers the manifest, namespace, path, size, and first-level block/item
/// declaration rules documented by Horizon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleValidation {
    pub path: PathBuf,
    pub sha256: String,
    pub valid: bool,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub side: Option<String>,
    pub api: Option<String>,
    pub namespaces: Vec<String>,
    pub files: usize,
    pub bytes: u64,
    pub block_definitions: usize,
    pub item_definitions: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The result of installing a validated bundle into a Horizon game folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    pub bundle: PathBuf,
    pub game_dir: PathBuf,
    pub destination: PathBuf,
    pub dry_run: bool,
    pub bytes: u64,
    pub sha256: String,
    pub validation: BundleValidation,
}

/// Check an existing `.hmod` without changing it.
pub fn validate_content_bundle(path: &Path) -> Result<BundleValidation> {
    if !path.is_file() {
        return Err(VibeError::MissingPath(path.to_path_buf()));
    }
    let sha256 = hash_file(path)?;
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| VibeError::Archive(format!("{}: {error}", path.display())))?;

    let mut names = Vec::new();
    let mut manifest_text = None;
    let mut total_bytes = 0u64;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total_over = false;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| VibeError::Archive(format!("{}: {error}", path.display())))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let size = entry.size();
        if !seen.insert(name.clone()) {
            errors.push(format!("duplicate archive entry {name:?}"));
        }
        if size > MAX_FILE_BYTES {
            errors.push(format!(
                "{name} is {size} bytes; Horizon's limit is {MAX_FILE_BYTES} bytes per file"
            ));
        }
        total_bytes = total_bytes.saturating_add(size);
        if total_bytes > MAX_TOTAL_BYTES && !total_over {
            errors.push(format!(
                "the bundle exceeds Horizon's {MAX_TOTAL_BYTES}-byte total limit"
            ));
            total_over = true;
        }
        if name == MANIFEST_NAME {
            if size > MAX_METADATA_BYTES {
                errors.push(format!(
                    "{MANIFEST_NAME} is larger than the {MAX_METADATA_BYTES}-byte inspection limit"
                ));
            } else {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                manifest_text = Some(text);
            }
        }
        names.push(name);
    }

    if names.len() > MAX_FILES {
        errors.push(format!(
            "the bundle has {} files; Horizon's limit is {MAX_FILES}",
            names.len()
        ));
    }

    let mut id = None;
    let mut name = None;
    let mut version = None;
    let mut side = None;
    let mut api = None;
    let mut advertised = BTreeSet::new();

    match manifest_text {
        None => errors.push(format!("bundle has no {MANIFEST_NAME}")),
        Some(text) => {
            let manifest = match text.parse::<toml::Value>() {
                Ok(toml::Value::Table(table)) => Some(table),
                Ok(_) => {
                    errors.push(format!("{MANIFEST_NAME} must be a TOML table"));
                    None
                }
                Err(error) => {
                    errors.push(format!("{MANIFEST_NAME} does not parse: {error}"));
                    None
                }
            };
            if let Some(table) = manifest {
                for key in table.keys() {
                    if !MANIFEST_KEYS.contains(&key.as_str()) {
                        errors.push(format!("unknown manifest key {key:?}"));
                    }
                }

                id = manifest_string(&table, "id", &mut errors, true);
                name = manifest_string(&table, "name", &mut errors, false);
                version = manifest_string(&table, "version", &mut errors, true);
                api = manifest_string(&table, "api", &mut errors, true);
                side = manifest_string(&table, "side", &mut errors, false);

                if let Some(value) = &id {
                    if !valid_horizon_id(value) {
                        errors.push(format!(
                            "manifest id {value:?} is not Horizon's [a-z0-9_]{{1,64}} pattern"
                        ));
                    }
                }
                if let Some(value) = &version {
                    if value.trim().is_empty() {
                        errors.push("manifest version is empty".into());
                    }
                }
                if let Some(value) = &api {
                    if value != CONTENT_API {
                        errors.push(format!(
                            "manifest api {value:?} is not Horizon's content API {CONTENT_API}"
                        ));
                    }
                }
                if let Some(value) = &side {
                    if !matches!(value.as_str(), "client" | "server" | "both") {
                        errors.push(format!(
                            "manifest side {value:?} is not client, server, or both"
                        ));
                    }
                } else {
                    warnings.push(
                        "manifest has no side; Horizon treats a missing side as \"client\"".into(),
                    );
                }
                if name.is_none() {
                    warnings.push("manifest has no name".into());
                }

                match table.get("capabilities") {
                    Some(toml::Value::Array(values)) => {
                        let has_registries = values
                            .iter()
                            .filter_map(toml::Value::as_str)
                            .any(|value| value == "registries");
                        if !has_registries {
                            errors.push(
                                "manifest capabilities does not include \"registries\"".into(),
                            );
                        }
                        for value in values {
                            if let Some(value) = value.as_str() {
                                if value != "registries" {
                                    warnings.push(format!(
                                        "manifest has unrecognised capability {value:?}"
                                    ));
                                }
                            } else {
                                errors.push(
                                    "manifest capabilities must be an array of strings".into(),
                                );
                            }
                        }
                    }
                    Some(_) => {
                        errors.push("manifest capabilities must be an array of strings".into())
                    }
                    None => errors.push("manifest is missing capabilities".into()),
                }

                match table.get("authors") {
                    Some(toml::Value::Array(values)) => {
                        if values.iter().any(|value| !value.is_str()) {
                            errors.push("manifest authors must be an array of strings".into());
                        }
                    }
                    Some(toml::Value::String(_)) | None => {
                        if table.get("authors").is_some() {
                            errors.push("manifest authors must be an array of strings".into());
                        }
                    }
                    Some(_) => errors.push("manifest authors must be an array of strings".into()),
                }
                match table.get("description") {
                    Some(toml::Value::String(_)) | None => {}
                    Some(_) => errors.push("manifest description must be a string".into()),
                }

                if let Some(extra) = table.get("namespaces") {
                    match extra {
                        toml::Value::Array(values) => {
                            for value in values {
                                let Some(namespace) = value.as_str() else {
                                    errors.push(
                                        "manifest namespaces must be an array of strings".into(),
                                    );
                                    continue;
                                };
                                if !valid_namespace(namespace) {
                                    errors.push(format!(
                                        "manifest namespace {namespace:?} is invalid"
                                    ));
                                } else if matches!(namespace, "minecraft" | "c" | "horizon") {
                                    errors.push(format!(
                                        "manifest namespace {namespace:?} is reserved"
                                    ));
                                } else if !advertised.insert(namespace.to_string()) {
                                    warnings.push(format!(
                                        "manifest namespace {namespace:?} is listed more than once"
                                    ));
                                }
                            }
                        }
                        _ => errors.push("manifest namespaces must be an array of strings".into()),
                    }
                }
            }
        }
    }

    if let Some(value) = &id {
        advertised.insert(value.clone());
    }
    let owned = advertised.clone();

    let mut block_definitions = 0;
    let mut item_definitions = 0;
    for entry_name in &names {
        if entry_name == MANIFEST_NAME {
            continue;
        }
        if !valid_entry_path(entry_name) {
            errors.push(format!(
                "entry {entry_name:?} is not a safe, lowercase resource path"
            ));
            continue;
        }
        if entry_name.ends_with(".class") {
            warnings.push(format!(
                "{entry_name} is Java bytecode; Horizon content mods ignore it"
            ));
            continue;
        }
        let Some((prefix, rest)) = entry_name
            .strip_prefix("assets/")
            .map(|rest| ("assets", rest))
            .or_else(|| entry_name.strip_prefix("data/").map(|rest| ("data", rest)))
        else {
            warnings.push(format!(
                "{entry_name} is neither a manifest, an asset, nor a data file; Horizon ignores it"
            ));
            continue;
        };
        let namespace = rest.split('/').next().unwrap_or("");
        if namespace.is_empty() {
            errors.push(format!("{entry_name} has no namespace"));
            continue;
        }
        if namespace == "horizon" {
            errors.push(format!(
                "{entry_name} uses the reserved namespace \"horizon\""
            ));
            continue;
        }
        if namespace == "minecraft" {
            let allowed = entry_name.starts_with("data/minecraft/tags/")
                || entry_name.starts_with("assets/minecraft/atlases/");
            if !allowed {
                errors.push(format!(
                    "{entry_name} is under minecraft; a content mod may only add tags or atlas sources there"
                ));
            }
        } else if namespace == "c" {
            if !entry_name.starts_with("data/c/tags/") {
                errors.push(format!(
                    "{entry_name} is under c; a content mod may only add convention tags there"
                ));
            }
        } else if !owned.contains(namespace) {
            errors.push(format!(
                "{entry_name} uses namespace {namespace:?}, which the manifest does not own"
            ));
        }

        if let Some(id) = id.as_deref() {
            let block_prefix = format!("data/{id}/horizon/block/");
            let item_prefix = format!("data/{id}/horizon/item/");
            let kind = if entry_name.starts_with(&block_prefix) && entry_name.ends_with(".json") {
                Some(("block", &mut block_definitions))
            } else if entry_name.starts_with(&item_prefix) && entry_name.ends_with(".json") {
                Some(("item", &mut item_definitions))
            } else {
                None
            };
            if let Some((kind, count)) = kind {
                *count += 1;
                match archive.by_name(entry_name) {
                    Ok(mut entry) => {
                        let mut text = String::new();
                        if let Err(error) = entry.read_to_string(&mut text) {
                            errors.push(format!("{entry_name} could not be read: {error}"));
                        } else {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(serde_json::Value::Object(object)) => {
                                    match object.get("template").and_then(|v| v.as_str()) {
                                        Some(template) if template.starts_with("minecraft:") => {}
                                        Some(template) => errors.push(format!(
                                            "{entry_name} template {template:?} is not a minecraft: template"
                                        )),
                                        None => errors.push(format!(
                                            "{entry_name} is a {kind} declaration without a string template"
                                        )),
                                    }
                                }
                                Ok(_) => errors.push(format!(
                                    "{entry_name} must be a JSON object with a template"
                                )),
                                Err(error) => {
                                    errors.push(format!("{entry_name} is not valid JSON: {error}"))
                                }
                            }
                        }
                    }
                    Err(error) => errors.push(format!("{entry_name} could not be opened: {error}")),
                }
            }
        }
        let _ = prefix;
    }

    let valid = errors.is_empty();
    Ok(BundleValidation {
        path: path.to_path_buf(),
        sha256,
        valid,
        id,
        name,
        version,
        side,
        api,
        namespaces: advertised.into_iter().collect(),
        files: names.len(),
        bytes: total_bytes,
        block_definitions,
        item_definitions,
        errors,
        warnings,
    })
}

/// Validate a bundle and copy it into `<game_dir>/horizon-mods/`.
///
/// `dry_run` performs every check and reports the destination without writing.
pub fn install_content_bundle(
    bundle: &Path,
    game_dir: &Path,
    force: bool,
    dry_run: bool,
) -> Result<InstallReport> {
    if !game_dir.is_dir() {
        return Err(VibeError::Horizon(format!(
            "game directory {} does not exist or is not a directory",
            game_dir.display()
        )));
    }
    let validation = validate_content_bundle(bundle)?;
    if !validation.valid {
        let details = validation
            .errors
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        return Err(VibeError::Horizon(format!(
            "{} is not a valid Horizon content bundle: {details}",
            bundle.display()
        )));
    }
    let file_name = bundle
        .file_name()
        .ok_or_else(|| VibeError::Horizon("bundle path has no file name".into()))?;
    let mods_dir = game_dir.join("horizon-mods");
    let destination = mods_dir.join(file_name);
    if same_path(bundle, &destination) {
        return Err(VibeError::Horizon(
            "refusing to install a bundle over itself".into(),
        ));
    }
    if destination.exists() && !force {
        return Err(VibeError::Horizon(format!(
            "{} already exists; pass --force to replace it",
            destination.display()
        )));
    }

    let source_bytes = std::fs::metadata(bundle)?.len();
    if !dry_run {
        std::fs::create_dir_all(&mods_dir)?;
        let temp = temp_path(&destination);
        let _ = std::fs::remove_file(&temp);
        let copied = (|| -> Result<()> {
            std::fs::copy(bundle, &temp)?;
            if destination.exists() {
                std::fs::remove_file(&destination)?;
            }
            std::fs::rename(&temp, &destination)?;
            Ok(())
        })();
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
    }

    Ok(InstallReport {
        bundle: bundle.to_path_buf(),
        game_dir: game_dir.to_path_buf(),
        destination,
        dry_run,
        bytes: source_bytes,
        sha256: validation.sha256.clone(),
        validation,
    })
}

fn manifest_string(
    table: &toml::Table,
    key: &str,
    errors: &mut Vec<String>,
    required: bool,
) -> Option<String> {
    match table.get(key) {
        Some(toml::Value::String(value)) => Some(value.clone()),
        Some(_) => {
            errors.push(format!("manifest key {key:?} must be a string"));
            None
        }
        None => {
            if required {
                errors.push(format!("manifest is missing {key:?}"));
            }
            None
        }
    }
}

fn valid_horizon_id(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn hash_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Assets,
    Data,
    VanillaTag,
    ConventionTag,
    Atlas,
}

impl EntryKind {
    fn label(self) -> &'static str {
        match self {
            Self::Assets => "assets",
            Self::Data => "data",
            Self::VanillaTag => "vanilla_tags",
            Self::ConventionTag => "convention_tags",
            Self::Atlas => "atlases",
        }
    }
}

/// Inspect `jar` and write a Horizon content-mod bundle to `output`.
pub fn export_jar(jar: &Path, output: &Path, options: &ExportOptions) -> Result<ContentExport> {
    let inspection = inspect_jar(jar, None)?;
    export_content(jar, &inspection, output, options)
}

/// Write a Horizon content-mod bundle from an already inspected jar.
pub fn export_content(
    jar: &Path,
    inspection: &JarInspection,
    output: &Path,
    options: &ExportOptions,
) -> Result<ContentExport> {
    if !jar.is_file() {
        return Err(VibeError::MissingPath(jar.to_path_buf()));
    }
    if output.exists() && !options.force {
        return Err(VibeError::Horizon(format!(
            "{} already exists; pass --force to replace it",
            output.display()
        )));
    }
    if same_path(jar, output) {
        return Err(VibeError::Horizon(
            "refusing to overwrite the source jar with the exported bundle".into(),
        ));
    }

    let id = match &options.id {
        Some(id) => ModId::parse(id.clone())?,
        None => {
            let metadata = &inspection.metadata;
            if metadata.mods.len() != 1 {
                return Err(VibeError::Horizon(format!(
                    "{} declares {} mods; pass --id to choose the bundle's id",
                    jar.display(),
                    metadata.mods.len()
                )));
            }
            ModId::parse(metadata.mods[0].mod_id.to_string())?
        }
    };

    let mut namespaces: BTreeSet<String> = BTreeSet::new();
    namespaces.insert(id.to_string());
    for namespace in &options.namespaces {
        let namespace = namespace.trim();
        if !valid_namespace(namespace) {
            return Err(VibeError::Horizon(format!(
                "invalid namespace {namespace:?}; expected [a-z0-9_.-]+"
            )));
        }
        if matches!(namespace, "minecraft" | "c" | "horizon") {
            return Err(VibeError::Horizon(format!(
                "namespace {namespace:?} is reserved; only the mod id and extra mod namespaces may be claimed"
            )));
        }
        namespaces.insert(namespace.to_string());
    }

    let first_mod = inspection.metadata.mods.first();
    let name = options
        .name
        .clone()
        .or_else(|| first_mod.map(|m| m.display_name.clone()))
        .unwrap_or_else(|| id.to_string());
    let version = options
        .version
        .clone()
        .or_else(|| first_mod.map(|m| m.version.clone()))
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| "0.0.0".to_string());
    let side = options
        .side
        .clone()
        .map(|side| normalize_side(&side))
        .transpose()?
        .unwrap_or_else(|| {
            first_mod
                .map(|m| match m.side {
                    ModSide::Client => "client",
                    ModSide::Server => "server",
                    ModSide::Both | ModSide::Unknown => "both",
                })
                .unwrap_or("both")
                .to_string()
        });
    let authors: Vec<String> = {
        let mut seen = BTreeSet::new();
        inspection
            .metadata
            .mods
            .iter()
            .flat_map(|m| m.authors.iter().cloned())
            .filter(|author| !author.trim().is_empty())
            .filter(|author| seen.insert(author.clone()))
            .collect()
    };
    let description = first_mod
        .and_then(|m| m.description.clone())
        .unwrap_or_default();

    let mut manifest = String::new();
    manifest.push_str(&format!("id = {}\n", toml_string(&id.to_string())));
    manifest.push_str(&format!("name = {}\n", toml_string(&name)));
    manifest.push_str(&format!("version = {}\n", toml_string(&version)));
    if !description.trim().is_empty() {
        manifest.push_str(&format!(
            "description = {}\n",
            toml_string(description.trim())
        ));
    }
    manifest.push_str(&format!("authors = {}\n", toml_array(&authors)));
    manifest.push_str(&format!("api = {}\n", toml_string(CONTENT_API)));
    manifest.push_str(&format!("side = {}\n", toml_string(&side)));
    manifest.push_str("capabilities = [\"registries\"]\n");
    let extra_namespaces: Vec<String> = namespaces
        .iter()
        .filter(|namespace| namespace.as_str() != id.as_str())
        .cloned()
        .collect();
    if !extra_namespaces.is_empty() {
        manifest.push_str(&format!("namespaces = {}\n", toml_array(&extra_namespaces)));
    }

    // First pass: classify every entry, keeping the archive order for reading
    // and a sorted list for reproducible output.
    let source = File::open(jar)?;
    let mut archive = ZipArchive::new(source)
        .map_err(|error| VibeError::Archive(format!("{}: {error}", jar.display())))?;
    let mut selected: Vec<(String, EntryKind, u64)> = Vec::new();
    let mut outside_namespaces: BTreeSet<String> = BTreeSet::new();
    let mut excluded_files = 0usize;
    let mut ignored_entries = 0usize;
    let mut total_bytes = 0u64;
    let mut warnings: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    let mut skipped_truncated = false;
    let mut source_pack_formats: Vec<SourcePackFormat> = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| VibeError::Archive(format!("{}: {error}", jar.display())))?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name().to_string();
        let name = raw_name.replace('\\', "/");
        if name == "pack.mcmeta" {
            ignored_entries += 1;
            if entry.size() > MAX_PACK_META_BYTES {
                warnings.push(format!(
                    "pack.mcmeta is larger than {MAX_PACK_META_BYTES} bytes; its pack format was not read"
                ));
            } else {
                let mut text = String::new();
                match entry.read_to_string(&mut text) {
                    Ok(_) => match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(value) => {
                            let pack = value.get("pack");
                            source_pack_formats.push(SourcePackFormat {
                                path: name.clone(),
                                pack_format: pack
                                    .and_then(|pack| pack.get("pack_format"))
                                    .and_then(serde_json::Value::as_i64),
                                supported_formats: pack
                                    .and_then(|pack| pack.get("supported_formats"))
                                    .cloned(),
                            });
                        }
                        Err(error) => {
                            warnings.push(format!("pack.mcmeta is not valid JSON: {error}"));
                        }
                    },
                    Err(error) => {
                        warnings.push(format!("pack.mcmeta could not be read as text: {error}"));
                    }
                }
            }
            record_skip(
                &mut skipped,
                &mut skipped_truncated,
                &name,
                "not_content",
                Some("pack metadata is not carried by a content mod".to_string()),
            );
            continue;
        }
        match classify_entry(&name, &namespaces, options) {
            Some(kind) => {
                if !valid_entry_path(&name) {
                    excluded_files += 1;
                    record_skip(
                        &mut skipped,
                        &mut skipped_truncated,
                        &name,
                        "unsafe_path",
                        Some("not a safe, lowercase resource path".to_string()),
                    );
                    continue;
                }
                let size = entry.size();
                if size > MAX_FILE_BYTES {
                    return Err(VibeError::Horizon(format!(
                        "{name} is {size} bytes; Horizon's limit is {MAX_FILE_BYTES} bytes per file"
                    )));
                }
                total_bytes += size;
                if total_bytes > MAX_TOTAL_BYTES {
                    return Err(VibeError::Horizon(format!(
                        "the selected content exceeds Horizon's {MAX_TOTAL_BYTES}-byte bundle limit"
                    )));
                }
                selected.push((name, kind, size));
            }
            None => {
                if let Some(namespace) = outside_namespace(&name, &namespaces) {
                    outside_namespaces.insert(namespace.clone());
                    excluded_files += 1;
                    record_skip(
                        &mut skipped,
                        &mut skipped_truncated,
                        &name,
                        "unclaimed_namespace",
                        Some(namespace),
                    );
                } else {
                    ignored_entries += 1;
                    record_skip(
                        &mut skipped,
                        &mut skipped_truncated,
                        &name,
                        "not_content",
                        None,
                    );
                }
            }
        }
    }

    if selected.len() > MAX_FILES {
        return Err(VibeError::Horizon(format!(
            "the selected content has {} files; Horizon's limit is {MAX_FILES}",
            selected.len()
        )));
    }
    if selected.is_empty() {
        let candidates = outside_namespaces.iter().cloned().collect::<Vec<_>>();
        let detail = if candidates.is_empty() {
            "the jar has no assets/ or data/ resources".to_string()
        } else {
            format!(
                "its resources are under namespace(s) {}; pass --namespace for each one Horizon may have",
                candidates.join(", ")
            )
        };
        return Err(VibeError::Horizon(format!(
            "{} has nothing a Horizon content mod may carry: {detail}",
            jar.display()
        )));
    }

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = temp_path(output);
    let mut file_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, kind, _) in &selected {
        *file_counts.entry(kind.label().to_string()).or_default() += 1;
    }

    let write_result = (|| -> Result<()> {
        let target = File::create(&temp)?;
        let mut writer = ZipWriter::new(target);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        writer
            .start_file(MANIFEST_NAME, options)
            .map_err(zip_error)?;
        writer.write_all(manifest.as_bytes())?;

        let mut names: Vec<&(String, EntryKind, u64)> = selected.iter().collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, _, expected) in names {
            let mut entry = archive
                .by_name(name)
                .map_err(|error| VibeError::Archive(format!("{name}: {error}")))?;
            let mut bytes = Vec::with_capacity((*expected).min(MAX_FILE_BYTES) as usize);
            entry.read_to_end(&mut bytes)?;
            if bytes.len() as u64 != *expected {
                return Err(VibeError::Archive(format!(
                    "{name} changed size while reading ({} != {expected})",
                    bytes.len()
                )));
            }
            writer
                .start_file(name.as_str(), options)
                .map_err(zip_error)?;
            writer.write_all(&bytes)?;
        }
        writer.finish().map_err(zip_error)?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    if let Err(error) = validate_bundle(&temp, &id) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    if output.exists() {
        std::fs::remove_file(output)?;
    }
    std::fs::rename(&temp, output)?;

    let mut notes = Vec::new();
    if !outside_namespaces.is_empty() {
        warnings.push(format!(
            "left {} file(s) under unclaimed namespace(s) {}; pass --namespace to claim namespaces the original mod really owns",
            excluded_files,
            outside_namespaces
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if inspection.has_assets || inspection.has_data {
        notes.push(
            "Resources are copied byte for byte. Vibe does not upgrade Minecraft pack formats; a jar written for an older Minecraft version may contain files Horizon cannot read."
                .into(),
        );
    }
    if !inspection.entrypoints.is_empty() || !inspection.report.entrypoints.is_empty() {
        notes.push(format!(
            "Java code is not carried. {} entrypoint class(es) stay in the source jar; any block, item or behaviour they registered in code has no content declaration in the bundle.",
            inspection.entrypoints.len().max(inspection.report.entrypoints.len())
        ));
    }
    if !inspection.mixin_configs.is_empty()
        || !inspection.access_transformers.is_empty()
        || inspection.core_plugin.is_some()
    {
        notes.push(
            "This jar declares Mixins, access transformers, or a coremod. Horizon content mods cannot execute any of them."
                .into(),
        );
    }
    if side == "client" {
        notes.push(
            "The manifest says side = \"client\" because the jar declares no server side. A content mod for both halves must say side = \"both\"."
                .into(),
        );
    }

    let minecraft_version_range = inspection
        .metadata
        .dependencies
        .values()
        .flat_map(|deps| deps.iter())
        .find(|dep| dep.mod_id == "minecraft" && !dep.version_range.trim().is_empty())
        .map(|dep| dep.version_range.clone());
    if let Some(range) = &minecraft_version_range {
        notes.push(format!(
            "The jar declares a Minecraft dependency range {range}; Vibe reports it but does not translate it into a Horizon version check."
        ));
    }
    if !source_pack_formats.is_empty() {
        let formats: Vec<String> = source_pack_formats
            .iter()
            .map(|format| match format.pack_format {
                Some(value) => format!("{}: pack_format {value}", format.path),
                None => format!("{}: no pack_format", format.path),
            })
            .collect();
        notes.push(format!(
            "pack.mcmeta declares {}; Horizon content mods are read as 26.3 content, Vibe copies resource bytes unchanged, and pack.mcmeta itself is not part of a content mod.",
            formats.join(", ")
        ));
    }

    let mut entries: Vec<ExportEntry> = selected
        .iter()
        .map(|(path, kind, bytes)| ExportEntry {
            path: path.clone(),
            kind: kind.label().to_string(),
            bytes: *bytes,
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(ContentExport {
        source: jar.to_path_buf(),
        source_sha256: inspection.sha256.clone(),
        output: output.to_path_buf(),
        id: id.to_string(),
        name,
        version,
        side,
        namespaces: namespaces.into_iter().collect(),
        outside_namespaces: outside_namespaces.into_iter().collect(),
        files: selected.len(),
        bytes: total_bytes,
        excluded_files,
        ignored_entries,
        file_counts,
        entries,
        skipped,
        skipped_truncated,
        source_pack_formats,
        minecraft_version_range,
        manifest,
        warnings,
        notes,
    })
}

fn record_skip(
    skipped: &mut Vec<SkippedEntry>,
    truncated: &mut bool,
    path: &str,
    reason: &str,
    detail: Option<String>,
) {
    if skipped.len() >= MAX_SKIPPED_DETAILS {
        *truncated = true;
        return;
    }
    skipped.push(SkippedEntry {
        path: path.to_string(),
        reason: reason.to_string(),
        detail,
    });
}

fn classify_entry(
    name: &str,
    namespaces: &BTreeSet<String>,
    options: &ExportOptions,
) -> Option<EntryKind> {
    if name.starts_with("assets/minecraft/atlases/") {
        return options.include_atlases.then_some(EntryKind::Atlas);
    }
    if let Some(rest) = name.strip_prefix("assets/") {
        let namespace = rest.split('/').next()?;
        if namespaces.contains(namespace) {
            return Some(EntryKind::Assets);
        }
        return None;
    }
    if let Some(rest) = name.strip_prefix("data/") {
        let namespace = rest.split('/').next()?;
        if namespaces.contains(namespace) {
            return Some(EntryKind::Data);
        }
        if namespace == "minecraft"
            && name.starts_with("data/minecraft/tags/")
            && options.include_vanilla_tags
        {
            return Some(EntryKind::VanillaTag);
        }
        if namespace == "c" && options.include_convention_tags {
            return Some(EntryKind::ConventionTag);
        }
    }
    None
}

fn outside_namespace(name: &str, namespaces: &BTreeSet<String>) -> Option<String> {
    for prefix in ["assets/", "data/"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let namespace = rest.split('/').next()?;
            if !namespace.is_empty()
                && !namespaces.contains(namespace)
                && namespace != "minecraft"
                && namespace != "c"
                && namespace != "horizon"
            {
                return Some(namespace.to_string());
            }
        }
    }
    None
}

fn valid_entry_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.')
                })
        })
}

fn valid_namespace(namespace: &str) -> bool {
    !namespace.is_empty()
        && namespace.len() <= 64
        && namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
}

fn normalize_side(side: &str) -> Result<String> {
    match side.trim().to_ascii_lowercase().as_str() {
        "client" => Ok("client".into()),
        "server" => Ok("server".into()),
        "both" => Ok("both".into()),
        other => Err(VibeError::Horizon(format!(
            "invalid side {other:?}; expected client, server, or both"
        ))),
    }
}

fn validate_bundle(path: &Path, expected_id: &ModId) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| VibeError::Archive(format!("export validation: {error}")))?;
    {
        let mut manifest = archive.by_name(MANIFEST_NAME).map_err(|error| {
            VibeError::Horizon(format!("bundle has no {MANIFEST_NAME}: {error}"))
        })?;
        let mut text = String::new();
        manifest.read_to_string(&mut text)?;
        let value: toml::Value = text.parse().map_err(|error| {
            VibeError::Horizon(format!("bundle manifest does not parse: {error}"))
        })?;
        let id = value
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| VibeError::Horizon("bundle manifest has no id".into()))?;
        if id != expected_id.as_str() {
            return Err(VibeError::Horizon(format!(
                "bundle manifest id {id:?} does not match {expected_id}"
            )));
        }
        let api = value.get("api").and_then(toml::Value::as_str).unwrap_or("");
        if api != CONTENT_API {
            return Err(VibeError::Horizon(format!(
                "bundle manifest api {api:?} is not {CONTENT_API}"
            )));
        }
        let capabilities = value
            .get("capabilities")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !capabilities
            .iter()
            .any(|capability| capability.as_str() == Some("registries"))
        {
            return Err(VibeError::Horizon(
                "bundle manifest is missing the registries capability".into(),
            ));
        }
    }
    Ok(())
}

fn toml_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn toml_array(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|value| toml_string(value)).collect();
    format!("[{}]", items.join(", "))
}

fn temp_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bundle")
        .to_string();
    name.push_str(&format!(".tmp-{}", std::process::id()));
    output.with_file_name(name)
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn zip_error(error: zip::result::ZipError) -> VibeError {
    VibeError::Archive(format!("could not write content bundle: {error}"))
}

fn executable_names(stem: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{stem}.exe"), stem.to_string()]
    } else {
        vec![stem.to_string()]
    }
}

fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn resolve_executable(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.is_dir() {
        for name in executable_names("horizon-ctl") {
            let candidate = path.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
            let candidate = path.join("bin").join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        return None;
    }
    let raw = path.to_string_lossy();
    if !raw.contains(std::path::MAIN_SEPARATOR) {
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            for name in executable_names(&raw) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn fixture_jar(path: &Path) {
        let metadata = r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"

[[mods]]
modId = "examplemod"
version = "1.2.3"
displayName = "Example Mod"
description = "A test fixture"
authors = "Ada, Grace"
"#;
        write_zip(
            path,
            &[
                ("META-INF/mods.toml", metadata.as_bytes()),
                ("assets/examplemod/lang/en_us.json", b"{}"),
                ("assets/examplemod/textures/block/example.png", b"png"),
                ("data/examplemod/recipe/example.json", b"{}"),
                ("data/minecraft/tags/block/example.json", b"{}"),
                ("assets/othermod/lang/en_us.json", b"{}"),
                ("example/ExampleMod.class", b"not a class"),
            ],
        );
    }

    #[test]
    fn exports_content_and_preserves_the_source() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("examplemod.jar");
        let output = temp.path().join("examplemod.hmod");
        fixture_jar(&jar);
        let before = std::fs::read(&jar).unwrap();

        let exported = export_jar(&jar, &output, &ExportOptions::default()).unwrap();

        assert_eq!(exported.id, "examplemod");
        assert_eq!(exported.version, "1.2.3");
        assert_eq!(exported.files, 4);
        assert_eq!(exported.outside_namespaces, vec!["othermod"]);
        assert_eq!(std::fs::read(&jar).unwrap(), before);

        let archive_file = File::open(&output).unwrap();
        let mut archive = ZipArchive::new(archive_file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"horizon-mod.toml".to_string()));
        assert!(names.contains(&"assets/examplemod/textures/block/example.png".to_string()));
        assert!(names.contains(&"data/minecraft/tags/block/example.json".to_string()));
        assert!(!names.iter().any(|name| name.contains("othermod")));

        let mut manifest = String::new();
        archive
            .by_name("horizon-mod.toml")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(manifest.contains("id = \"examplemod\""));
        assert!(manifest.contains("api = \"0.2\""));
        assert!(manifest.contains("capabilities = [\"registries\"]"));
        assert!(manifest.contains("authors = [\"Ada\", \"Grace\"]"));
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("examplemod.jar");
        let output = temp.path().join("examplemod.hmod");
        fixture_jar(&jar);
        std::fs::write(&output, b"existing").unwrap();
        let error = export_jar(&jar, &output, &ExportOptions::default()).unwrap_err();
        assert!(error.to_string().contains("--force"));
        assert_eq!(std::fs::read(&output).unwrap(), b"existing");
    }

    #[test]
    fn claims_extra_namespaces_on_request() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("examplemod.jar");
        let output = temp.path().join("examplemod.hmod");
        fixture_jar(&jar);
        let options = ExportOptions {
            namespaces: vec!["othermod".into()],
            ..ExportOptions::default()
        };
        let exported = export_jar(&jar, &output, &options).unwrap();
        assert_eq!(exported.files, 5);
        assert!(exported.manifest.contains("namespaces = [\"othermod\"]"));
    }

    #[test]
    fn rejects_bad_ids_and_namespaces() {
        assert!(valid_namespace("example_mod"));
        assert!(!valid_namespace("Example"));
        assert!(!valid_namespace("has/slash"));
        let options = ExportOptions {
            namespaces: vec!["minecraft".into()],
            ..ExportOptions::default()
        };
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("examplemod.jar");
        fixture_jar(&jar);
        let error = export_jar(&jar, &temp.path().join("x.hmod"), &options).unwrap_err();
        assert!(error.to_string().contains("reserved"));
    }

    #[test]
    fn explains_entries_skips_and_pack_format() {
        let temp = tempfile::tempdir().unwrap();
        let jar = temp.path().join("explained.jar");
        let output = temp.path().join("explained.hmod");
        let metadata = r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"

[[mods]]
modId = "examplemod"
version = "1.2.3"
displayName = "Example Mod"

[[dependencies.examplemod]]
modId = "minecraft"
mandatory = true
versionRange = "[1.21,1.22)"
ordering = "NONE"
side = "BOTH"
"#;
        write_zip(
            &jar,
            &[
                ("META-INF/mods.toml", metadata.as_bytes()),
                (
                    "pack.mcmeta",
                    br#"{"pack":{"pack_format":34,"supported_formats":[34,45]}}"#,
                ),
                ("assets/examplemod/lang/en_us.json", b"{}"),
                ("assets/othermod/lang/en_us.json", b"{}"),
                (
                    "data/examplemod/horizon/block/ruby_block.json",
                    br#"{"template": "minecraft:iron_block"}"#,
                ),
                ("example/ExampleMod.class", b"not a class"),
                ("docs/readme.txt", b"notes"),
            ],
        );

        let exported = export_jar(&jar, &output, &ExportOptions::default()).unwrap();
        assert_eq!(exported.files, 2);
        assert_eq!(exported.entries.len(), 2);
        assert!(exported.entries.iter().any(|entry| entry.path
            == "assets/examplemod/lang/en_us.json"
            && entry.kind == "assets"));
        assert!(exported.entries.iter().any(|entry| entry.path
            == "data/examplemod/horizon/block/ruby_block.json"
            && entry.kind == "data"));

        let skipped = |path: &str| {
            exported
                .skipped
                .iter()
                .find(|entry| entry.path == path)
                .unwrap_or_else(|| panic!("{path} was not explained: {:?}", exported.skipped))
        };
        assert_eq!(
            skipped("assets/othermod/lang/en_us.json").reason,
            "unclaimed_namespace"
        );
        assert_eq!(
            skipped("assets/othermod/lang/en_us.json").detail.as_deref(),
            Some("othermod")
        );
        assert_eq!(skipped("example/ExampleMod.class").reason, "not_content");
        assert_eq!(skipped("docs/readme.txt").reason, "not_content");
        assert_eq!(skipped("pack.mcmeta").reason, "not_content");
        assert!(!exported.skipped_truncated);

        assert_eq!(exported.source_pack_formats.len(), 1);
        assert_eq!(exported.source_pack_formats[0].pack_format, Some(34));
        assert_eq!(
            exported.minecraft_version_range.as_deref(),
            Some("[1.21,1.22)")
        );
        assert!(exported
            .notes
            .iter()
            .any(|note| note.contains("pack.mcmeta")));
    }

    #[test]
    fn entry_paths_are_lowercase_and_relative() {
        assert!(valid_entry_path("assets/examplemod/textures/a.png"));
        assert!(!valid_entry_path("assets/Examplemod/a.png"));
        assert!(!valid_entry_path("../escape"));
        assert!(!valid_entry_path("assets//a.png"));
    }

    fn exported_bundle(temp: &Path) -> (PathBuf, PathBuf) {
        let jar = temp.join("examplemod.jar");
        let bundle = temp.join("examplemod.hmod");
        fixture_jar(&jar);
        export_jar(&jar, &bundle, &ExportOptions::default()).unwrap();
        (jar, bundle)
    }

    #[test]
    fn validates_a_bundle_that_export_produced() {
        let temp = tempfile::tempdir().unwrap();
        let (_, bundle) = exported_bundle(temp.path());
        let report = validate_content_bundle(&bundle).unwrap();
        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(report.id.as_deref(), Some("examplemod"));
        assert_eq!(report.version.as_deref(), Some("1.2.3"));
        assert_eq!(report.api.as_deref(), Some(CONTENT_API));
        assert_eq!(report.namespaces, vec!["examplemod".to_string()]);
        assert_eq!(report.files, 5);
        assert_eq!(report.block_definitions, 0);
        assert_eq!(report.item_definitions, 0);
        assert_eq!(report.sha256.len(), 64);
    }

    #[test]
    fn validation_reports_manifest_and_namespace_errors() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("broken.hmod");
        write_zip(
            &bundle,
            &[
                (
                    MANIFEST_NAME,
                    br#"
id = "broken_mod"
version = "1"
api = "0.2"
capabilities = ["registries"]
typo = "refused"
"#,
                ),
                ("assets/other_mod/lang/en_us.json", b"{}"),
                ("data/minecraft/not_a_tag/x.json", b"{}"),
                ("assets/broken_mod/horizon/block/ruby.json", b"{}"),
            ],
        );

        let report = validate_content_bundle(&bundle).unwrap();
        assert!(!report.valid);
        let errors = report.errors.join("\n");
        assert!(errors.contains("unknown manifest key \"typo\""), "{errors}");
        assert!(errors.contains("\"other_mod\""), "{errors}");
        assert!(
            errors.contains("data/minecraft/not_a_tag/x.json"),
            "{errors}"
        );
        assert!(report.warnings.iter().any(|w| w.contains("side")));

        let missing_capability = temp.path().join("no_capability.hmod");
        write_zip(
            &missing_capability,
            &[
                (
                    MANIFEST_NAME,
                    b"id = \"broken_mod\"\nversion = \"1\"\napi = \"0.2\"\n",
                ),
                ("assets/broken_mod/lang/en_us.json", b"{}"),
            ],
        );
        let report = validate_content_bundle(&missing_capability).unwrap();
        assert!(!report.valid);
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("capabilities")));

        let bad_api = temp.path().join("bad_api.hmod");
        write_zip(
            &bad_api,
            &[(
                MANIFEST_NAME,
                b"id = \"broken_mod\"\nversion = \"1\"\napi = \"0.9\"\ncapabilities = [\"registries\"]\n",
            )],
        );
        let report = validate_content_bundle(&bad_api).unwrap();
        assert!(!report.valid);
        assert!(report.errors.iter().any(|error| error.contains("api")));
    }

    #[test]
    fn validation_reads_block_and_item_declarations() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("content.hmod");
        write_zip(
            &bundle,
            &[
                (
                    MANIFEST_NAME,
                    b"id = \"content_mod\"\nversion = \"1\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n",
                ),
                (
                    "data/content_mod/horizon/block/ruby_block.json",
                    br#"{"template": "minecraft:iron_block", "item": true}"#,
                ),
                (
                    "data/content_mod/horizon/item/ruby.json",
                    br#"{"template": "minecraft:iron_ingot"}"#,
                ),
            ],
        );

        let report = validate_content_bundle(&bundle).unwrap();
        assert!(report.valid, "{:?}", report.errors);
        assert_eq!(report.block_definitions, 1);
        assert_eq!(report.item_definitions, 1);

        let broken = temp.path().join("broken_declaration.hmod");
        write_zip(
            &broken,
            &[
                (
                    MANIFEST_NAME,
                    b"id = \"content_mod\"\nversion = \"1\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n",
                ),
                (
                    "data/content_mod/horizon/block/ruby_block.json",
                    br#"{"template": "not_a_template"}"#,
                ),
            ],
        );
        let report = validate_content_bundle(&broken).unwrap();
        assert!(!report.valid);
        assert!(report.errors.iter().any(|error| error.contains("template")));
    }

    #[test]
    fn installs_a_validated_bundle_into_horizon_mods() {
        let temp = tempfile::tempdir().unwrap();
        let (_, bundle) = exported_bundle(temp.path());
        let game = temp.path().join("game");
        std::fs::create_dir(&game).unwrap();

        let dry = install_content_bundle(&bundle, &game, false, true).unwrap();
        assert!(dry.dry_run);
        assert_eq!(
            dry.destination,
            game.join("horizon-mods").join("examplemod.hmod")
        );
        assert!(!game.join("horizon-mods").exists());

        let installed = install_content_bundle(&bundle, &game, false, false).unwrap();
        assert!(!installed.dry_run);
        assert!(installed.destination.is_file());
        assert_eq!(
            std::fs::read(&installed.destination).unwrap(),
            std::fs::read(&bundle).unwrap()
        );

        let error = install_content_bundle(&bundle, &game, false, false).unwrap_err();
        assert!(error.to_string().contains("--force"), "{error}");

        let replaced = install_content_bundle(&bundle, &game, true, false).unwrap();
        assert!(replaced.destination.is_file());
    }

    #[test]
    fn install_refuses_an_invalid_bundle_and_a_missing_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("bad.hmod");
        write_zip(
            &bundle,
            &[(
                MANIFEST_NAME,
                b"id = \"bad_mod\"\nversion = \"1\"\napi = \"0.2\"\n",
            )],
        );
        let game = temp.path().join("game");
        std::fs::create_dir(&game).unwrap();
        let error = install_content_bundle(&bundle, &game, false, false).unwrap_err();
        assert!(error
            .to_string()
            .contains("not a valid Horizon content bundle"));

        let other = temp.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let valid = exported_bundle(&other).1;
        let missing = temp.path().join("no_such_game");
        let error = install_content_bundle(&valid, &missing, false, true).unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }
}
