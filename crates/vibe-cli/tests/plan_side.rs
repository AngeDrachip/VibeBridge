//! Side-aware load planning through the real CLI.
//!
//! These jars are created at test time because they carry metadata only: no
//! class file is needed to prove how a dependency's declared side changes a
//! plan. The jars are never executed.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn write_metadata_jar(path: &Path, mods_toml: &str) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("META-INF/mods.toml", options).unwrap();
    zip.write_all(mods_toml.as_bytes()).unwrap();
    zip.finish().unwrap();
}

fn run_plan(paths: &[&PathBuf], side: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_vibe"))
        .arg("--json")
        .arg("plan")
        .args(paths)
        .arg("--side")
        .arg(side)
        .output()
        .expect("vibe must run");
    assert!(
        output.status.success(),
        "plan failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("plan prints JSON")
}

fn owner_jar(dir: &Path) -> PathBuf {
    let path = dir.join("side-owner.jar");
    write_metadata_jar(
        &path,
        r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"

[[mods]]
modId = "side_owner"
version = "1.0.0"
displayName = "Side owner"

[[dependencies.side_owner]]
modId = "minecraft"
mandatory = true
versionRange = "[1.21,1.22)"
ordering = "NONE"
side = "BOTH"

[[dependencies.side_owner]]
modId = "client_lib"
mandatory = true
versionRange = "[2,)"
ordering = "AFTER"
side = "CLIENT"
"#,
    );
    path
}

fn client_only_jar(dir: &Path) -> PathBuf {
    let path = dir.join("client-only.jar");
    write_metadata_jar(
        &path,
        r#"
modLoader = "javafml"
loaderVersion = "[47,)"
license = "MIT"
clientSideOnly = true

[[mods]]
modId = "client_only"
version = "1.0.0"
displayName = "Client only"
"#,
    );
    path
}

#[test]
fn a_client_only_dependency_is_missing_only_on_the_client_side() {
    let temp = tempfile::tempdir().unwrap();
    let owner = owner_jar(temp.path());

    let server = run_plan(&[&owner], "SERVER");
    assert_eq!(server["plan"]["side"], "server");
    assert_eq!(
        server["plan"]["missing_dependencies"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "{}",
        server["plan"]
    );

    let client = run_plan(&[&owner], "CLIENT");
    assert_eq!(client["plan"]["side"], "client");
    let missing = client["plan"]["missing_dependencies"].as_array().unwrap();
    assert_eq!(missing.len(), 1, "{}", client["plan"]);
    assert_eq!(missing[0]["dependency"], "client_lib");
    assert_eq!(missing[0]["side"], "client");
    assert_eq!(missing[0]["version_range"], "[2,)");
    assert_eq!(missing[0]["mandatory"], true);
}

#[test]
fn a_client_only_mod_is_excluded_from_a_server_plan() {
    let temp = tempfile::tempdir().unwrap();
    let owner = owner_jar(temp.path());
    let client = client_only_jar(temp.path());

    let server = run_plan(&[&owner, &client], "SERVER");
    let ids: Vec<&str> = server["plan"]["mods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|module| module["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["side_owner"]);
    assert_eq!(server["plan"]["excluded_mods"][0]["id"], "client_only");
    assert_eq!(server["plan"]["excluded_mods"][0]["side"], "client");

    let client_plan = run_plan(&[&owner, &client], "CLIENT");
    let ids: Vec<&str> = client_plan["plan"]["mods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|module| module["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["client_only", "side_owner"]);
    assert_eq!(
        client_plan["plan"]["excluded_mods"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
