//! End-to-end proof that the Rust client and the real Java runtime agree.
//!
//! This test runs compiled fixture jars, not synthetic protocol messages. It is
//! skipped when the Java side has not been built; CI builds it first and sets
//! `VIBE_REQUIRE_JAVA_TESTS=1` so a missing build fails instead of skipping.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vibe_bridge::{default_runtime_path, locate_java, RuntimeClient, RuntimeOptions};
use vibe_core::Verdict;
use vibe_horizon::{export_jar, ExportOptions};
use vibe_loader::inspect_jar;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives under crates/")
        .to_path_buf()
}

fn fixtures() -> Vec<PathBuf> {
    let root = repo_root().join("java").join("test-mods").join("build");
    vec![root.join("hello-forge.jar"), root.join("legacy-hello.jar")]
}

fn required() -> bool {
    std::env::var("VIBE_REQUIRE_JAVA_TESTS")
        .map(|value| value == "1")
        .unwrap_or(false)
}

#[test]
fn runs_real_compiled_forge_shaped_fixtures() {
    let root = repo_root();
    let jars = fixtures();
    let runtime = default_runtime_path(&root);
    if runtime.is_none() || jars.iter().any(|jar| !jar.is_file()) {
        let message = "Java runtime or fixture jars are missing; run java/build.ps1 and \
                       java/test-mods/build.ps1 (or the .sh equivalents) first";
        if required() {
            panic!("{message}");
        }
        eprintln!("skipping: {message}");
        return;
    }

    for jar in &jars {
        let inspection = inspect_jar(jar, None).expect("fixture jar must inspect");
        assert_eq!(
            inspection.report.verdict,
            Verdict::RunnableSubset,
            "{jar:?}: {:?}",
            inspection.report.blockers
        );
    }

    let java = locate_java(None).expect("Java 17+ is required for the bridge test");
    let mut options = RuntimeOptions::new(java, runtime.unwrap());
    options.side = "SERVER".into();
    options.start_timeout = Duration::from_secs(30);
    options.request_timeout = Duration::from_secs(30);

    let mut client = RuntimeClient::spawn(options).expect("runtime should start and handshake");
    assert!(
        client.hello().runtime.starts_with("vibe-runtime/"),
        "unexpected hello: {:?}",
        client.hello()
    );
    assert_eq!(client.hello().protocol, 1);

    let load = client.load_mods(&jars, "SERVER").expect("mods.load");
    assert_eq!(load["ok"], true, "load failed: {load}");
    let loaded = load["mods"].as_array().expect("mods array");
    assert_eq!(loaded.len(), 2, "load: {load}");

    let lifecycle = client
        .start_lifecycle()
        .expect("lifecycle.start should complete");
    assert_eq!(lifecycle["ok"], true, "lifecycle failed: {lifecycle}");
    assert_eq!(lifecycle["phase"], "complete");
    let mods = lifecycle["mods"].as_array().expect("mod statuses");
    assert_eq!(mods.len(), 2);
    for module in mods {
        assert_eq!(
            module["status"], "initialized",
            "mod did not finish: {module}"
        );
        assert_eq!(module["constructed"], true);
    }

    let records = lifecycle["events"].as_array().expect("event records");
    let listeners: Vec<&str> = records
        .iter()
        .filter_map(|record| record["listener"].as_str())
        .collect();
    for expected in [
        "Handlers#onPing",
        "Handlers#onCommonSetup",
        "lambda#1",
        "hello_forge#preInit",
        "legacy_hello#construction",
        "legacy_hello#preInit",
        "legacy_hello#init",
        "legacy_hello#postInit",
        "legacy_hello#loadComplete",
    ] {
        assert!(
            listeners.contains(&expected),
            "missing listener {expected:?} in {listeners:?}"
        );
    }
    assert!(
        records.iter().all(|record| record["ok"] == true),
        "a lifecycle listener failed: {records:?}"
    );

    let status = client.mod_status().expect("mods.status");
    assert_eq!(status["phase"], "complete");

    let registry = client.registry_snapshot().expect("registry.snapshot");
    let ids: Vec<&str> = registry["mods"]
        .as_array()
        .expect("registry mods")
        .iter()
        .filter_map(|id| id.as_str())
        .collect();
    assert!(ids.contains(&"hello_forge"));
    assert!(ids.contains(&"legacy_hello"));

    let events = client.take_events();
    assert!(
        events.iter().any(|event| event.name == "runtime.ready"),
        "runtime should announce readiness: {events:?}"
    );

    client.shutdown().expect("clean shutdown");
}

#[test]
fn exports_fixture_resources_as_horizon_content() {
    let jar = repo_root()
        .join("java")
        .join("test-mods")
        .join("build")
        .join("hello-forge.jar");
    if !jar.is_file() {
        let message = "fixture jar is missing; run java/test-mods/build.ps1 (or .sh) first";
        if required() {
            panic!("{message}");
        }
        eprintln!("skipping: {message}");
        return;
    }

    let before = std::fs::read(&jar).expect("read source jar");
    let temp = tempfile::tempdir().expect("temp dir");
    let output = temp.path().join("hello_forge.hmod");
    let exported = export_jar(&jar, &output, &ExportOptions::default()).expect("export");
    assert_eq!(exported.id, "hello_forge");
    assert_eq!(exported.namespaces, vec!["hello_forge"]);
    assert_eq!(std::fs::read(&jar).expect("read source jar"), before);

    let file = std::fs::File::open(&output).expect("open bundle");
    let mut archive = zip::ZipArchive::new(file).expect("bundle is a zip");
    let names: Vec<String> = (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect();
    assert!(names.contains(&"horizon-mod.toml".to_string()));
    assert!(names.contains(&"assets/hello_forge/lang/en_us.json".to_string()));
    assert!(names.contains(&"data/hello_forge/recipe/example.json".to_string()));
}
