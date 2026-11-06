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
const MANIFEST_NAME: &str = "horizon-mod.toml";

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
    pub manifest: String,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
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

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| VibeError::Archive(format!("{}: {error}", jar.display())))?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name().to_string();
        let name = raw_name.replace('\\', "/");
        match classify_entry(&name, &namespaces, options) {
            Some(kind) => {
                if !valid_entry_path(&name) {
                    excluded_files += 1;
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
                    outside_namespaces.insert(namespace);
                    excluded_files += 1;
                } else {
                    ignored_entries += 1;
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

    let mut warnings = Vec::new();
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
        manifest,
        warnings,
        notes,
    })
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
    fn entry_paths_are_lowercase_and_relative() {
        assert!(valid_entry_path("assets/examplemod/textures/a.png"));
        assert!(!valid_entry_path("assets/Examplemod/a.png"));
        assert!(!valid_entry_path("../escape"));
        assert!(!valid_entry_path("assets//a.png"));
    }
}
