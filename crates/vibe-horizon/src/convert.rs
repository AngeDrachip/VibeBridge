//! Convert one or more Java mod jars (or unpacked mod folders) into Horizon
//! content bundles (`.hmod`).
//!
//! A Horizon content bundle carries `assets/` and `data/` only. This module
//! exports each source with [`super::export_jar`], and can merge several
//! exports into one bundle with a single id: duplicate paths are checked, the
//! `data/<id>/horizon/...` declarations are rewritten to the merged id, and the
//! result is validated with [`super::validate_content_bundle`].
//!
//! It never executes mod code and never modifies a source. Java code, Mixins,
//! coremods and content registered in Java stay in the source jar; the report
//! says so instead of pretending the conversion is complete.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use vibe_core::error::{Result, VibeError};
use vibe_core::ModId;

use super::{
    export_jar, temp_path, toml_array, toml_string, valid_entry_path, validate_content_bundle,
    ContentExport, ExportOptions, MAX_FILE_BYTES, MAX_TOTAL_BYTES, MANIFEST_NAME,
};

/// How a merged conversion handles two sources shipping the same path with
/// different bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictPolicy {
    /// Stop: the caller must decide which file wins.
    #[default]
    Fail,
    /// Keep the first source's file and report the duplicate.
    KeepFirst,
}

/// Options for [`convert_sources`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConvertOptions {
    /// Merge every source into one bundle with this id. When unset, each source
    /// is written as its own `<id>.hmod`.
    pub merge: Option<String>,
    /// Bundle id for a single source. Required when that jar declares more than
    /// one mod. Refused with several sources (each keeps its own id).
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    /// `client`, `server`, or `both`. Defaults to each source's declared side,
    /// and to `both` when a merge mixes sides.
    pub side: Option<String>,
    /// Extra namespaces to claim in every source (repeatable).
    pub namespaces: Vec<String>,
    /// Replace an existing output bundle.
    pub force: bool,
    pub include_vanilla_tags: bool,
    pub include_convention_tags: bool,
    pub include_atlases: bool,
    /// `NAME=TEMPLATE` item declarations, applied to every source.
    pub item_templates: Vec<String>,
    /// What to do when two merged sources ship the same path with different
    /// bytes.
    pub conflict: ConflictPolicy,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            merge: None,
            id: None,
            name: None,
            version: None,
            side: None,
            namespaces: Vec::new(),
            force: false,
            include_vanilla_tags: true,
            include_convention_tags: true,
            include_atlases: true,
            item_templates: Vec::new(),
            conflict: ConflictPolicy::Fail,
        }
    }
}

/// What one source became.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConversion {
    pub source: PathBuf,
    /// The bundle this source was written to, or the merged bundle it went into.
    #[serde(default)]
    pub output: Option<PathBuf>,
    #[serde(default)]
    pub export: Option<ContentExport>,
    /// Why this source failed, when it did. Other sources are still converted.
    #[serde(default)]
    pub error: Option<String>,
}

/// The result of merging several sources into one bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedBundle {
    pub output: PathBuf,
    pub sha256: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub side: String,
    pub files: usize,
    pub bytes: u64,
    pub namespaces: Vec<String>,
    /// Mod ids that went into the bundle.
    pub sources: Vec<String>,
    /// Identical files two sources both shipped, kept once.
    pub deduplicated: usize,
    /// Paths kept from the first source because `--keep-first` was set.
    pub kept_first: Vec<String>,
    pub manifest: String,
}

/// The whole batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionReport {
    pub outputs: Vec<PathBuf>,
    pub sources: Vec<SourceConversion>,
    #[serde(default)]
    pub merged: Option<MergedBundle>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

struct Prepared {
    /// Index of this source in the caller's list, so its report entry can be
    /// updated without comparing paths.
    source_index: usize,
    export: ContentExport,
    /// The temporary bundle the export wrote, removed once merged or renamed.
    bundle: PathBuf,
}

/// Convert `sources` into `output_dir`.
///
/// A source may be a jar or an unpacked mod folder. A failed source is reported
/// in [`ConversionReport::sources`] and does not stop the others; with `merge`,
/// one failed source stops the merge so no mod is silently dropped.
pub fn convert_sources(
    sources: &[PathBuf],
    output_dir: &Path,
    options: &ConvertOptions,
) -> Result<ConversionReport> {
    if sources.is_empty() {
        return Err(VibeError::Horizon(
            "no source mod was given; pass one or more jars or mod folders".into(),
        ));
    }
    if options.merge.is_some() && options.id.is_some() {
        return Err(VibeError::Horizon(
            "--merge names the bundle id; do not also pass --id".into(),
        ));
    }
    if options.id.is_some() && sources.len() > 1 {
        return Err(VibeError::Horizon(
            "--id names one bundle; it cannot apply to several sources. Use --merge ID to put them in one bundle, or convert them one by one.".into(),
        ));
    }
    if let Some(id) = &options.merge {
        ModId::parse(id.clone())?;
    }
    fs::create_dir_all(output_dir)?;

    let mut report = ConversionReport {
        outputs: Vec::new(),
        sources: Vec::new(),
        merged: None,
        warnings: Vec::new(),
        notes: Vec::new(),
    };
    let mut prepared: Vec<Prepared> = Vec::new();
    let mut failures = 0usize;

    for (index, source) in sources.iter().enumerate() {
        match prepare_one(source, output_dir, index, options) {
            Ok(item) => {
                report.sources.push(SourceConversion {
                    source: source.clone(),
                    output: None,
                    export: Some(item.export.clone()),
                    error: None,
                });
                prepared.push(item);
            }
            Err(error) => {
                failures += 1;
                report.sources.push(SourceConversion {
                    source: source.clone(),
                    output: None,
                    export: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    let result = if let Some(merge_id) = &options.merge {
        if failures > 0 {
            Err(VibeError::Horizon(format!(
                "{failures} source(s) could not be converted; nothing was merged. Fix them or convert the others separately."
            )))
        } else {
            let merged = merge_prepared(&prepared, output_dir, merge_id, options)?;
            report.outputs.push(merged.output.clone());
            for source in &mut report.sources {
                source.output = Some(merged.output.clone());
                if let Some(export) = &mut source.export {
                    export.output = merged.output.clone();
                }
            }
            report.merged = Some(merged);
            Ok(())
        }
    } else {
        let mut used: BTreeSet<String> = BTreeSet::new();
        for item in &prepared {
            let final_path = output_dir.join(format!("{}.hmod", item.export.id));
            let source = &mut report.sources[item.source_index];
            if !used.insert(item.export.id.clone()) {
                failures += 1;
                source.error = Some(format!(
                    "another source already declared the id {}; Horizon needs one bundle per id. Use --merge ID to combine them, or rename one mod.",
                    item.export.id
                ));
                continue;
            }
            if final_path.exists() && !options.force {
                failures += 1;
                source.error = Some(format!(
                    "{} already exists; pass --force to replace it",
                    final_path.display()
                ));
                continue;
            }
            if final_path.exists() {
                fs::remove_file(&final_path)?;
            }
            fs::rename(&item.bundle, &final_path)?;
            source.output = Some(final_path.clone());
            if let Some(export) = &mut source.export {
                export.output = final_path.clone();
            }
            report.outputs.push(final_path);
        }
        Ok(())
    };

    // The temporary exports are gone once renamed or merged; a failure leaves
    // them behind, so clean them up here.
    for item in &prepared {
        let _ = fs::remove_file(&item.bundle);
    }

    result?;

    report.notes.push(
        "Only assets/ and data/ are converted. Java code, Mixins, coremods, access transformers and content registered in Java stay in the source jar; Horizon cannot execute them."
            .into(),
    );
    if options.merge.is_some() {
        report.notes.push(
            "The merged bundle keeps each mod's asset/data namespaces. Declarations under data/<id>/horizon/ were rewritten to the merged id."
                .into(),
        );
    }
    if failures > 0 {
        report.warnings.push(format!(
            "{failures} source(s) failed; see each source's error in the report."
        ));
    }
    Ok(report)
}

fn prepare_one(
    source: &Path,
    output_dir: &Path,
    index: usize,
    options: &ConvertOptions,
) -> Result<Prepared> {
    if !source.exists() {
        return Err(VibeError::MissingPath(source.to_path_buf()));
    }
    let mut temp_jar: Option<PathBuf> = None;
    let jar = if source.is_dir() {
        let jar = output_dir.join(format!(".vibe-convert-{}-{index}.jar", std::process::id()));
        let _ = fs::remove_file(&jar);
        // When the output folder sits inside the mod folder, its previous
        // bundles must not be packed back into the source.
        let skip = output_subfolder(source, output_dir);
        pack_directory(source, &jar, skip.as_deref())?;
        temp_jar = Some(jar.clone());
        jar
    } else if source.is_file() {
        source.to_path_buf()
    } else {
        return Err(VibeError::Horizon(format!(
            "{} is neither a jar nor a folder",
            source.display()
        )));
    };

    let bundle = output_dir.join(format!(".vibe-convert-{}-{index}.hmod", std::process::id()));
    let _ = fs::remove_file(&bundle);
    let merge = options.merge.is_some();
    let export_options = ExportOptions {
        // A merged source keeps its own id; the merged manifest is written later.
        id: if merge { None } else { options.id.clone() },
        name: if merge { None } else { options.name.clone() },
        version: if merge { None } else { options.version.clone() },
        side: if merge { None } else { options.side.clone() },
        namespaces: options.namespaces.clone(),
        force: true,
        include_vanilla_tags: options.include_vanilla_tags,
        include_convention_tags: options.include_convention_tags,
        include_atlases: options.include_atlases,
        item_templates: options.item_templates.clone(),
    };
    let result = export_jar(&jar, &bundle, &export_options);
    if let Some(jar) = temp_jar {
        let _ = fs::remove_file(jar);
    }
    let export = result?;
    Ok(Prepared {
        source_index: index,
        export,
        bundle,
    })
}

fn merge_prepared(
    prepared: &[Prepared],
    output_dir: &Path,
    merge_id: &str,
    options: &ConvertOptions,
) -> Result<MergedBundle> {
    let merge_id = ModId::parse(merge_id.to_string())?;
    let mut files: BTreeMap<String, (Vec<u8>, usize)> = BTreeMap::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut kept_first: Vec<String> = Vec::new();
    let mut deduplicated = 0usize;
    let mut total_bytes = 0u64;

    for (index, item) in prepared.iter().enumerate() {
        let source_id = item.export.id.as_str();
        let file = File::open(&item.bundle)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| VibeError::Archive(format!("{}: {error}", item.bundle.display())))?;
        for entry_index in 0..archive.len() {
            let mut entry = archive.by_index(entry_index).map_err(|error| {
                VibeError::Archive(format!("{}: {error}", item.bundle.display()))
            })?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            if name == MANIFEST_NAME {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(VibeError::Horizon(format!(
                    "{name} is {} bytes; Horizon's limit is {MAX_FILE_BYTES} bytes per file",
                    bytes.len()
                )));
            }
            let name = rewrite_declaration(&name, source_id, merge_id.as_str());
            if !valid_entry_path(&name) {
                return Err(VibeError::Horizon(format!(
                    "{name} is not a safe, lowercase resource path"
                )));
            }
            match files.get(&name) {
                None => {
                    total_bytes += bytes.len() as u64;
                    files.insert(name, (bytes, index));
                }
                Some((existing, first)) => {
                    if existing == &bytes {
                        deduplicated += 1;
                    } else if options.conflict == ConflictPolicy::KeepFirst {
                        kept_first.push(format!(
                            "{name} (kept {}, dropped {})",
                            prepared[*first].export.id, item.export.id
                        ));
                    } else {
                        conflicts.push(format!(
                            "{name} ({} and {})",
                            prepared[*first].export.id, item.export.id
                        ));
                    }
                }
            }
        }
    }

    if total_bytes > MAX_TOTAL_BYTES {
        return Err(VibeError::Horizon(format!(
            "the merged content is {total_bytes} bytes; Horizon's bundle limit is {MAX_TOTAL_BYTES} bytes"
        )));
    }
    if !conflicts.is_empty() {
        let shown: Vec<&str> = conflicts.iter().take(20).map(String::as_str).collect();
        return Err(VibeError::Horizon(format!(
            "{} file(s) differ between sources: {}{}. Pass --keep-first to keep the first source's copy, or convert the mods separately.",
            conflicts.len(),
            shown.join("; "),
            if conflicts.len() > shown.len() {
                format!("; and {} more", conflicts.len() - shown.len())
            } else {
                String::new()
            }
        )));
    }

    let mut namespaces: BTreeSet<String> = BTreeSet::new();
    namespaces.insert(merge_id.to_string());
    let mut authors: BTreeSet<String> = BTreeSet::new();
    let mut source_ids: Vec<String> = Vec::new();
    let mut sides: BTreeSet<String> = BTreeSet::new();
    for item in prepared {
        for namespace in &item.export.namespaces {
            namespaces.insert(namespace.clone());
        }
        sides.insert(item.export.side.clone());
        source_ids.push(item.export.id.clone());
        authors.extend(manifest_authors(&item.export.manifest));
    }

    let side = options.side.clone().unwrap_or_else(|| {
        if sides.len() == 1 {
            sides.iter().next().cloned().unwrap_or_else(|| "both".into())
        } else {
            "both".into()
        }
    });
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| format!("Converted pack ({} mods)", prepared.len()));
    let version = options
        .version
        .clone()
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| "1.0.0".into());
    let description = format!("Converted from: {}", source_ids.join(", "));
    let manifest = merged_manifest(
        merge_id.as_str(),
        &name,
        &version,
        &description,
        &authors.iter().cloned().collect::<Vec<_>>(),
        &side,
        &namespaces,
    );

    let output = output_dir.join(format!("{merge_id}.hmod"));
    if output.exists() && !options.force {
        return Err(VibeError::Horizon(format!(
            "{} already exists; pass --force to replace it",
            output.display()
        )));
    }
    let temp = temp_path(&output);
    let _ = fs::remove_file(&temp);

    let write = (|| -> Result<()> {
        let file = File::create(&temp)?;
        let mut writer = ZipWriter::new(file);
        let zip_options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        writer
            .start_file(MANIFEST_NAME, zip_options)
            .map_err(|error| VibeError::Archive(format!("could not write manifest: {error}")))?;
        writer.write_all(manifest.as_bytes())?;
        for (name, (bytes, _)) in &files {
            writer
                .start_file(name.as_str(), zip_options)
                .map_err(|error| VibeError::Archive(format!("could not write {name}: {error}")))?;
            writer.write_all(bytes)?;
        }
        writer
            .finish()
            .map_err(|error| VibeError::Archive(format!("could not finish bundle: {error}")))?;
        Ok(())
    })();
    if let Err(error) = write {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }

    let validation = match validate_content_bundle(&temp) {
        Ok(validation) => validation,
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
    };
    if !validation.valid {
        let _ = fs::remove_file(&temp);
        return Err(VibeError::Horizon(format!(
            "the merged bundle is not valid: {}",
            validation.errors.join("; ")
        )));
    }

    if output.exists() {
        fs::remove_file(&output)?;
    }
    fs::rename(&temp, &output)?;

    Ok(MergedBundle {
        output,
        sha256: validation.sha256,
        id: merge_id.to_string(),
        name,
        version,
        side,
        files: files.len(),
        bytes: total_bytes,
        namespaces: namespaces.into_iter().collect(),
        sources: source_ids,
        deduplicated,
        kept_first,
        manifest,
    })
}

/// `data/<source>/horizon/...` belongs to the source bundle's id. In a merged
/// bundle, Horizon reads declarations under the merged id only.
fn rewrite_declaration(name: &str, source_id: &str, merge_id: &str) -> String {
    let prefix = format!("data/{source_id}/horizon/");
    match name.strip_prefix(&prefix) {
        Some(rest) => format!("data/{merge_id}/horizon/{rest}"),
        None => name.to_string(),
    }
}

fn manifest_authors(manifest: &str) -> Vec<String> {
    manifest
        .parse::<toml::Value>()
        .ok()
        .and_then(|value| {
            value
                .get("authors")
                .and_then(toml::Value::as_array)
                .map(|authors| {
                    authors
                        .iter()
                        .filter_map(toml::Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
        })
        .unwrap_or_default()
}

fn merged_manifest(
    id: &str,
    name: &str,
    version: &str,
    description: &str,
    authors: &[String],
    side: &str,
    namespaces: &BTreeSet<String>,
) -> String {
    let mut manifest = String::new();
    manifest.push_str(&format!("id = {}\n", toml_string(id)));
    manifest.push_str(&format!("name = {}\n", toml_string(name)));
    manifest.push_str(&format!("version = {}\n", toml_string(version)));
    if !description.trim().is_empty() {
        manifest.push_str(&format!("description = {}\n", toml_string(description.trim())));
    }
    manifest.push_str(&format!("authors = {}\n", toml_array(authors)));
    manifest.push_str(&format!("api = {}\n", toml_string(super::CONTENT_API)));
    manifest.push_str(&format!("side = {}\n", toml_string(side)));
    manifest.push_str("capabilities = [\"registries\"]\n");
    let extra: Vec<String> = namespaces
        .iter()
        .filter(|namespace| namespace.as_str() != id)
        .cloned()
        .collect();
    if !extra.is_empty() {
        manifest.push_str(&format!("namespaces = {}\n", toml_array(&extra)));
    }
    manifest
}

/// The output folder as a path relative to `source`, when it is strictly
/// inside it. Used to leave previous bundles out of a folder source.
fn output_subfolder(source: &Path, output_dir: &Path) -> Option<String> {
    let source = fs::canonicalize(source).ok()?;
    let output = fs::canonicalize(output_dir).ok()?;
    let relative = output.strip_prefix(&source).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn pack_directory(dir: &Path, jar: &Path, skip: Option<&str>) -> Result<()> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    if let Some(skip) = skip {
        let prefix = format!("{skip}/");
        files.retain(|file| file != skip && !file.starts_with(&prefix));
    }
    files.sort();
    let file = File::create(jar)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for relative in files {
        writer
            .start_file(relative.as_str(), options)
            .map_err(|error| VibeError::Archive(format!("could not add {relative}: {error}")))?;
        let bytes = fs::read(dir.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))?;
        writer.write_all(&bytes)?;
    }
    writer
        .finish()
        .map_err(|error| VibeError::Archive(format!("could not finish {}: {error}", jar.display())))?;
    Ok(())
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(current)?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| VibeError::Horizon(format!("{}: {error}", path.display())))?;
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_jar(path: &Path, id: &str, extra: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        let metadata = format!(
            "modLoader = \"javafml\"\nloaderVersion = \"[47,)\"\nlicense = \"MIT\"\n\n[[mods]]\nmodId = \"{id}\"\nversion = \"1.2.3\"\ndisplayName = \"{id}\"\nauthors = \"Ada\"\n"
        );
        writer
            .start_file("META-INF/mods.toml", options)
            .unwrap();
        writer.write_all(metadata.as_bytes()).unwrap();
        for (name, bytes) in extra {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn source_jar(temp: &Path, id: &str) -> PathBuf {
        let jar = temp.join(format!("{id}.jar"));
        write_jar(
            &jar,
            id,
            &[
                (
                    Box::leak(format!("assets/{id}/lang/en_us.json").into_boxed_str()),
                    br#"{"item.example":"Example"}"#,
                ),
                (
                    Box::leak(format!("data/{id}/recipe/example.json").into_boxed_str()),
                    br#"{"type":"minecraft:crafting_shapeless"}"#,
                ),
                (
                    Box::leak(format!("data/{id}/horizon/item/ruby.json").into_boxed_str()),
                    br#"{"template":"minecraft:diamond"}"#,
                ),
                ("example/ExampleMod.class", b"not a class"),
            ],
        );
        jar
    }

    #[test]
    fn converts_several_jars_into_separate_bundles() {
        let temp = tempfile::tempdir().unwrap();
        let first = source_jar(temp.path(), "firstmod");
        let second = source_jar(temp.path(), "secondmod");
        let out = temp.path().join("out");
        let report = convert_sources(
            &[first, second],
            &out,
            &ConvertOptions::default(),
        )
        .unwrap();

        assert_eq!(report.outputs.len(), 2);
        assert!(out.join("firstmod.hmod").is_file());
        assert!(out.join("secondmod.hmod").is_file());
        assert!(report.sources.iter().all(|source| source.error.is_none()));
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("Only assets/ and data/ are converted")));
        for output in &report.outputs {
            let validation = validate_content_bundle(output).unwrap();
            assert!(validation.valid, "{:?}", validation.errors);
        }
    }

    #[test]
    fn merges_and_rewrites_item_declarations() {
        let temp = tempfile::tempdir().unwrap();
        let first = source_jar(temp.path(), "firstmod");
        let second = source_jar(temp.path(), "secondmod");
        let out = temp.path().join("out");
        let report = convert_sources(
            &[first, second],
            &out,
            &ConvertOptions {
                merge: Some("pack".into()),
                ..ConvertOptions::default()
            },
        )
        .unwrap();

        let merged = report.merged.as_ref().unwrap();
        assert_eq!(merged.id, "pack");
        assert_eq!(report.outputs, vec![out.join("pack.hmod")]);
        // Both sources declare the item under their own id; the rewrite puts
        // both at data/pack/horizon/item/ruby.json, and the identical bytes are
        // kept once. So: 2 assets + 2 recipes + 1 shared item declaration.
        assert_eq!(merged.files, 5);
        assert_eq!(merged.deduplicated, 1);
        assert_eq!(merged.namespaces, vec!["firstmod", "pack", "secondmod"]);

        let file = File::open(&merged.output).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert!(archive.by_name("data/pack/horizon/item/ruby.json").is_ok());
        assert!(archive.by_name("data/firstmod/horizon/item/ruby.json").is_err());
        assert!(archive.by_name("assets/firstmod/lang/en_us.json").is_ok());
        assert!(archive.by_name("assets/secondmod/lang/en_us.json").is_ok());

        let validation = validate_content_bundle(&merged.output).unwrap();
        assert!(validation.valid, "{:?}", validation.errors);
        assert_eq!(validation.id.as_deref(), Some("pack"));
        assert_eq!(validation.item_definitions, 1);
    }

    #[test]
    fn identical_paths_are_kept_once_and_differences_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let first = source_jar(temp.path(), "firstmod");
        let second = source_jar(temp.path(), "secondmod");
        // The same path with the same bytes is not a conflict.
        let out = temp.path().join("out");
        let report = convert_sources(
            &[first.clone(), second.clone()],
            &out,
            &ConvertOptions {
                merge: Some("pack".into()),
                conflict: ConflictPolicy::KeepFirst,
                ..ConvertOptions::default()
            },
        )
        .unwrap();
        // Each source declares the item `ruby` under its own id, so the rewrite
        // makes them collide with different bytes? They are identical here.
        assert_eq!(report.merged.as_ref().unwrap().deduplicated, 1);

        let conflicting = temp.path().join("conflict");
        fs::create_dir_all(&conflicting).unwrap();
        let third = conflicting.join("thirdmod.jar");
        write_jar(
            &third,
            "thirdmod",
            &[(
                "data/thirdmod/horizon/item/ruby.json",
                br#"{"template":"minecraft:emerald"}"#,
            )],
        );
        let error = convert_sources(
            &[first, third],
            &conflicting,
            &ConvertOptions {
                merge: Some("pack".into()),
                ..ConvertOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("differ between sources"), "{error}");
    }

    #[test]
    fn converts_an_unpacked_folder() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder_mod");
        fs::create_dir_all(folder.join("META-INF")).unwrap();
        fs::create_dir_all(folder.join("assets/foldermod/lang")).unwrap();
        fs::create_dir_all(folder.join("data/foldermod/recipe")).unwrap();
        fs::write(
            folder.join("META-INF/mods.toml"),
            "modLoader = \"javafml\"\nloaderVersion = \"[47,)\"\n\n[[mods]]\nmodId = \"foldermod\"\nversion = \"2.0\"\n",
        )
        .unwrap();
        fs::write(folder.join("assets/foldermod/lang/en_us.json"), b"{}").unwrap();
        fs::write(folder.join("data/foldermod/recipe/x.json"), b"{}").unwrap();

        let out = temp.path().join("out");
        let report = convert_sources(&[folder], &out, &ConvertOptions::default()).unwrap();
        assert!(out.join("foldermod.hmod").is_file());
        assert_eq!(report.sources[0].export.as_ref().unwrap().version, "2.0");
    }

    #[test]
    fn a_folder_source_leaves_its_output_folder_out() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("foldermod");
        fs::create_dir_all(folder.join("META-INF")).unwrap();
        fs::create_dir_all(folder.join("assets/foldermod/lang")).unwrap();
        fs::write(
            folder.join("META-INF/mods.toml"),
            "modLoader = \"javafml\"\nloaderVersion = \"[47,)\"\n\n[[mods]]\nmodId = \"foldermod\"\nversion = \"1.0\"\n",
        )
        .unwrap();
        fs::write(folder.join("assets/foldermod/lang/en_us.json"), b"{}").unwrap();

        let out = folder.join("out");
        convert_sources(&[folder.clone()], &out, &ConvertOptions::default()).unwrap();
        // A previous bundle and a stray asset in the output folder must not be
        // packed back into the source on the next conversion.
        fs::create_dir_all(out.join("assets/foldermod/lang")).unwrap();
        fs::write(out.join("assets/foldermod/lang/stray.json"), b"{}").unwrap();
        let report = convert_sources(
            &[folder],
            &out,
            &ConvertOptions {
                force: true,
                ..ConvertOptions::default()
            },
        )
        .unwrap();
        let export = report.sources[0].export.as_ref().unwrap();
        assert_eq!(export.files, 1);
        assert_eq!(export.entries[0].path, "assets/foldermod/lang/en_us.json");
    }

    #[test]
    fn one_failed_source_does_not_stop_the_others() {
        let temp = tempfile::tempdir().unwrap();
        let good = source_jar(temp.path(), "goodmod");
        let missing = temp.path().join("missing.jar");
        let out = temp.path().join("out");
        let report = convert_sources(&[missing, good], &out, &ConvertOptions::default()).unwrap();
        assert_eq!(report.outputs.len(), 1);
        assert!(report.sources[0].error.is_some());
        assert!(report.sources[1].error.is_none());
        assert!(out.join("goodmod.hmod").is_file());
    }

    #[test]
    fn a_merge_refuses_to_drop_a_failed_source() {
        let temp = tempfile::tempdir().unwrap();
        let good = source_jar(temp.path(), "goodmod");
        let missing = temp.path().join("missing.jar");
        let out = temp.path().join("out");
        let error = convert_sources(
            &[good, missing],
            &out,
            &ConvertOptions {
                merge: Some("pack".into()),
                ..ConvertOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("nothing was merged"), "{error}");
        assert!(!out.join("pack.hmod").exists());
    }
}
