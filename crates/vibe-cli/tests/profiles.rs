//! Compatibility-profile behavior through the real CLI.
//!
//! The jars here carry metadata only. They are never executed; these tests
//! prove that Vibe detects declared Minecraft/Forge targets, refuses to guess
//! between profiles, and never starts a JVM for a metadata-only target.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn write_metadata_jar(path: &Path, id: &str, loader: &str, minecraft: Option<&str>) -> PathBuf {
    let mut mods_toml = format!(
        r#"modLoader = "javafml"
loaderVersion = "{loader}"
license = "MIT"

[[mods]]
modId = "{id}"
version = "1.0.0"
displayName = "{id}"
"#
    );
    if let Some(range) = minecraft {
        mods_toml.push_str(&format!(
            r#"
[[dependencies.{id}]]
modId = "minecraft"
mandatory = true
versionRange = "{range}"
ordering = "NONE"
side = "BOTH"
"#
        ));
    }

    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("META-INF/mods.toml", options).unwrap();
    zip.write_all(mods_toml.as_bytes()).unwrap();
    zip.finish().unwrap();
    path.to_path_buf()
}

fn jar(dir: &Path, name: &str, loader: &str, minecraft: Option<&str>) -> PathBuf {
    write_metadata_jar(&dir.join(format!("{name}.jar")), name, loader, minecraft)
}

fn run(args: &[&str]) -> (i32, Value) {
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
    (code, value)
}

#[test]
fn profiles_lists_the_embedded_matrix() {
    let (code, value) = run(&["--json", "profiles"]);
    assert_eq!(code, 0, "{value}");
    let ids: Vec<&str> = value["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["forge-1.20.1-47", "forge-1.21.1-52", "forge-26.3-66"]);
    assert_eq!(value["horizon_content_api"], "0.2");

    let (code, value) = run(&["--json", "profiles", "--runtime-only"]);
    assert_eq!(code, 0, "{value}");
    let ids: Vec<&str> = value["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["forge-1.20.1-47", "forge-1.21.1-52"]);
}

#[test]
fn inspect_resolves_a_declared_minecraft_target() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "modern_mod", "[52,53)", Some("[1.21.1,1.22)"));

    let (code, value) = run(&["--json", "inspect", jar.to_str().unwrap()]);
    assert_eq!(code, 0, "{value}");
    let resolution = &value["profiles"][0]["resolution"];
    assert_eq!(resolution["status"], "matched", "{resolution}");
    assert_eq!(resolution["selected"], "forge-1.21.1-52");
    assert_eq!(
        resolution["requirements"]["minecraft_range"],
        "[1.21.1,1.22)"
    );
    assert_eq!(resolution["requirements"]["loader_range"], "[52,53)");
}

#[test]
fn inspect_reports_an_unsupported_target() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "old_mod", "[47,48)", Some("[1.12.2]"));

    let (code, value) = run(&["--json", "inspect", jar.to_str().unwrap()]);
    assert_eq!(code, 0, "{value}");
    let resolution = &value["profiles"][0]["resolution"];
    assert_eq!(resolution["status"], "unsupported", "{resolution}");
    assert_eq!(resolution["selected"], Value::Null);
    assert!(resolution["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("no supported")));
}

#[test]
fn inspect_does_not_guess_between_two_matching_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "wide_mod", "[1,)", Some("[1.20,1.22)"));

    let (code, value) = run(&["--json", "inspect", jar.to_str().unwrap()]);
    assert_eq!(code, 0, "{value}");
    let resolution = &value["profiles"][0]["resolution"];
    assert_eq!(resolution["status"], "ambiguous", "{resolution}");
    assert!(resolution["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("--profile")));
}

#[test]
fn plan_reports_mixed_targets_as_a_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let old = jar(temp.path(), "old_mod", "[47,48)", Some("[1.20.1,1.20.2)"));
    let new = jar(temp.path(), "new_mod", "[52,53)", Some("[1.21.1,1.22)"));

    let (code, value) = run(&[
        "--json",
        "plan",
        old.to_str().unwrap(),
        new.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{value}");
    assert_eq!(value["plan"]["profile"], Value::Null, "{value}");
    let conflicts = value["plan"]["profile_conflicts"].as_array().unwrap();
    assert!(
        conflicts.iter().any(|conflict| conflict
            .as_str()
            .unwrap_or("")
            .contains("different profiles")),
        "{conflicts:?}"
    );
}

#[test]
fn an_explicit_profile_that_does_not_match_exits_five() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "new_mod", "[52,53)", Some("[1.21.1,1.22)"));

    let (code, value) = run(&[
        "--json",
        "plan",
        jar.to_str().unwrap(),
        "--profile",
        "forge-1.20.1-47",
    ]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["plan"]["profile"], Value::Null);
    assert!(!value["plan"]["profile_conflicts"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn run_refuses_a_metadata_only_profile_before_starting_java() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(
        temp.path(),
        "horizon_target",
        "[66,67)",
        Some("[26.3,26.4)"),
    );

    let (code, value) = run(&[
        "--json",
        "run",
        jar.to_str().unwrap(),
        "--profile",
        "forge-26.3-66",
    ]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["started"], false);
    assert!(value["reason"]
        .as_str()
        .unwrap()
        .contains("no runtime backend"));
}

#[test]
fn run_refuses_two_runtime_profiles_in_one_jvm() {
    let temp = tempfile::tempdir().unwrap();
    let old = jar(temp.path(), "old_mod", "[47,48)", Some("[1.20.1,1.20.2)"));
    let new = jar(temp.path(), "new_mod", "[52,53)", Some("[1.21.1,1.22)"));

    let (code, value) = run(&[
        "--json",
        "run",
        old.to_str().unwrap(),
        new.to_str().unwrap(),
        "--include-blocked",
    ]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["started"], false);
    assert!(value["reason"]
        .as_str()
        .unwrap()
        .contains("more than one compatibility profile"));
}

#[test]
fn run_without_a_profile_reports_undeclared_mods_instead_of_guessing() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "undeclared_mod", "[52,)", None);

    let (code, value) = run(&["--json", "run", jar.to_str().unwrap(), "--include-blocked"]);
    assert_eq!(code, 3, "{value}");
    assert_eq!(value["started"], false);
    let skipped = value["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert_eq!(skipped[0]["profile_status"], "undeclared");
    assert!(skipped[0]["reason"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("--profile")));
}

#[test]
fn run_rejects_a_runnable_mod_that_does_not_match_the_requested_profile() {
    let temp = tempfile::tempdir().unwrap();
    let jar = jar(temp.path(), "new_mod", "[52,53)", Some("[1.21.1,1.22)"));

    let (code, value) = run(&[
        "--json",
        "run",
        jar.to_str().unwrap(),
        "--include-blocked",
        "--profile",
        "forge-1.20.1-47",
    ]);
    assert_eq!(code, 5, "{value}");
    assert_eq!(value["started"], false);
    assert!(!value["profile_conflicts"].as_array().unwrap().is_empty());
}
