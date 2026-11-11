//! `vibe horizon convert` through the real CLI.
//!
//! The jars here carry metadata, assets and data only. They are never
//! executed: the tests prove that several mods become separate bundles, that
//! `--merge` produces one validated bundle, and that a broken source is
//! reported without silently dropping it from a merge.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn write_jar(path: &Path, id: &str, extra: &[(&str, &[u8])]) -> PathBuf {
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
    path.to_path_buf()
}

fn source_jar(dir: &Path, id: &str) -> PathBuf {
    let asset = format!("assets/{id}/lang/en_us.json");
    let recipe = format!("data/{id}/recipe/example.json");
    let item = format!("data/{id}/horizon/item/ruby.json");
    write_jar(
        &dir.join(format!("{id}.jar")),
        id,
        &[
            (asset.as_str(), br#"{"item.example":"Example"}"#),
            (recipe.as_str(), br#"{"type":"minecraft:crafting_shapeless"}"#),
            (item.as_str(), br#"{"template":"minecraft:diamond"}"#),
        ],
    )
}

fn run(args: &[&str]) -> (i32, Value, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_vibe"))
        .args(args)
        .output()
        .expect("vibe must run");
    let code = output.status.code().expect("vibe exits with a code");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected JSON from {args:?}: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, value, stderr)
}

#[test]
fn converts_several_mods_into_separate_bundles() {
    let temp = tempfile::tempdir().unwrap();
    let first = source_jar(temp.path(), "firstmod");
    let second = source_jar(temp.path(), "secondmod");
    let out = temp.path().join("out");

    let (code, value, stderr) = run(&[
        "--json",
        "horizon",
        "convert",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{value} {stderr}");
    assert_eq!(value["outputs"].as_array().unwrap().len(), 2);
    assert!(out.join("firstmod.hmod").is_file());
    assert!(out.join("secondmod.hmod").is_file());
    let ids: Vec<&str> = value["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            assert!(source["error"].is_null(), "{source}");
            source["export"]["id"].as_str().unwrap()
        })
        .collect();
    assert_eq!(ids, ["firstmod", "secondmod"]);
    assert!(value["notes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|note| note.as_str().unwrap().contains("Only assets/ and data/")));

    // The bundles must satisfy the same rules `vibe horizon validate` checks.
    for output in value["outputs"].as_array().unwrap() {
        let (code, validation, stderr) = run(&[
            "--json",
            "horizon",
            "validate",
            output.as_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{validation} {stderr}");
        assert_eq!(validation["valid"], true, "{validation}");
    }
}

#[test]
fn merges_mods_into_one_validated_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let first = source_jar(temp.path(), "firstmod");
    let second = source_jar(temp.path(), "secondmod");
    let out = temp.path().join("out");

    let (code, value, stderr) = run(&[
        "--json",
        "horizon",
        "convert",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
        "--merge",
        "pack",
        "--name",
        "My pack",
        "--version",
        "2.0",
    ]);
    assert_eq!(code, 0, "{value} {stderr}");
    let merged = &value["merged"];
    assert_eq!(merged["id"], "pack");
    assert_eq!(merged["name"], "My pack");
    assert_eq!(merged["version"], "2.0");
    assert_eq!(merged["sources"].as_array().unwrap().len(), 2);
    assert_eq!(merged["deduplicated"], 1);
    assert_eq!(merged["files"], 5);
    assert!(out.join("pack.hmod").is_file());

    let (code, validation, stderr) = run(&[
        "--json",
        "horizon",
        "validate",
        out.join("pack.hmod").to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{validation} {stderr}");
    assert_eq!(validation["valid"], true, "{validation}");
    assert_eq!(validation["item_definitions"], 1);
}

#[test]
fn a_failed_source_is_reported_and_the_others_are_still_converted() {
    let temp = tempfile::tempdir().unwrap();
    let good = source_jar(temp.path(), "goodmod");
    let missing = temp.path().join("missing.jar");
    let out = temp.path().join("out");

    let (code, value, stderr) = run(&[
        "--json",
        "horizon",
        "convert",
        missing.to_str().unwrap(),
        good.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{value} {stderr}");
    assert!(value["sources"][0]["error"].is_string());
    assert!(value["sources"][1]["error"].is_null());
    assert!(out.join("goodmod.hmod").is_file());
}

#[test]
fn a_merge_refuses_to_silently_drop_a_failed_source() {
    let temp = tempfile::tempdir().unwrap();
    let good = source_jar(temp.path(), "goodmod");
    let missing = temp.path().join("missing.jar");
    let out = temp.path().join("out");

    let output = Command::new(env!("CARGO_BIN_EXE_vibe"))
        .args([
            "horizon",
            "convert",
            good.to_str().unwrap(),
            missing.to_str().unwrap(),
            "--output-dir",
            out.to_str().unwrap(),
            "--merge",
            "pack",
        ])
        .output()
        .expect("vibe must run");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("nothing was merged"), "{stderr}");
    assert!(!out.join("pack.hmod").exists());
}

#[test]
fn differing_files_stop_a_merge_unless_keep_first_is_given() {
    let temp = tempfile::tempdir().unwrap();
    let first = source_jar(temp.path(), "firstmod");
    let second = temp.path().join("secondmod.jar");
    write_jar(
        &second,
        "secondmod",
        &[(
            "data/secondmod/horizon/item/ruby.json",
            br#"{"template":"minecraft:emerald"}"#,
        )],
    );
    let out = temp.path().join("out");

    let output = Command::new(env!("CARGO_BIN_EXE_vibe"))
        .args([
            "horizon",
            "convert",
            first.to_str().unwrap(),
            second.to_str().unwrap(),
            "--output-dir",
            out.to_str().unwrap(),
            "--merge",
            "pack",
        ])
        .output()
        .expect("vibe must run");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("differ between sources"), "{stderr}");

    let (code, value, stderr) = run(&[
        "--json",
        "horizon",
        "convert",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
        "--merge",
        "pack",
        "--keep-first",
    ]);
    assert_eq!(code, 0, "{value} {stderr}");
    assert_eq!(value["merged"]["kept_first"].as_array().unwrap().len(), 1);
}
