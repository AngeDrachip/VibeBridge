//! Inspection tests for jars shaped like real Forge failure modes.
//!
//! Each fixture is compiled by `java/test-mods/blocked/build.ps1` (or `.sh`).
//! The test is skipped when the fixtures have not been built; CI sets
//! `VIBE_REQUIRE_JAVA_TESTS=1` so a missing build fails instead.

use std::path::{Path, PathBuf};

use vibe_core::analysis::BlockerKind;
use vibe_core::{Severity, Verdict};
use vibe_loader::inspect_jar;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives under crates/")
        .to_path_buf()
}

fn fixture(name: &str) -> PathBuf {
    repo_root()
        .join("java")
        .join("test-mods")
        .join("build")
        .join(format!("blocked-{name}.jar"))
}

fn required() -> bool {
    std::env::var("VIBE_REQUIRE_JAVA_TESTS")
        .map(|value| value == "1")
        .unwrap_or(false)
}

#[test]
fn each_blocker_kind_has_a_compiled_fixture() {
    let cases = [
        (
            "minecraft-ref",
            BlockerKind::MinecraftClasses,
            "net.minecraft.world.level.Level",
        ),
        (
            "unsupported-forge",
            BlockerKind::UnsupportedForgeApi,
            "net.minecraftforge.common.capabilities.Capability",
        ),
        (
            "external-lib",
            BlockerKind::ExternalClasses,
            "com.google.gson.Gson",
        ),
        (
            "mixin",
            BlockerKind::MixinTransformer,
            "mixin_fixture.mixins.json",
        ),
        (
            "access-transformer",
            BlockerKind::AccessTransformer,
            "META-INF/accesstransformer.cfg",
        ),
        (
            "coremod",
            BlockerKind::Coremod,
            "com.example.blocked.CorePlugin",
        ),
        ("no-entrypoint", BlockerKind::MissingEntrypoint, ""),
        (
            "unsupported-loader",
            BlockerKind::UnsupportedLoader,
            "kotlinfml",
        ),
    ];

    let missing: Vec<PathBuf> = cases
        .iter()
        .map(|(name, _, _)| fixture(name))
        .filter(|jar| !jar.is_file())
        .collect();
    if !missing.is_empty() {
        let message = format!(
            "blocked-case fixtures are missing: {missing:?}; run java/test-mods/blocked/build.ps1 (or .sh)"
        );
        if required() {
            panic!("{message}");
        }
        eprintln!("skipping: {message}");
        return;
    }

    for (name, kind, evidence) in cases {
        let jar = fixture(name);
        let inspection = inspect_jar(&jar, None).expect("fixture must inspect");
        assert_eq!(
            inspection.report.verdict,
            Verdict::Blocked,
            "{name}: {:?}",
            inspection.report.blockers
        );
        let blocker = inspection
            .report
            .blockers
            .iter()
            .find(|blocker| blocker.kind == kind)
            .unwrap_or_else(|| {
                panic!(
                    "{name}: missing {kind:?} in {:?}",
                    inspection.report.blockers
                )
            });
        assert_eq!(blocker.severity, Severity::Error, "{name}");
        if !evidence.is_empty() {
            assert!(
                blocker.evidence.iter().any(|item| item.contains(evidence))
                    || blocker.message.contains(evidence),
                "{name}: evidence {evidence:?} not in {:?} or {:?}",
                blocker.evidence,
                blocker.message
            );
        }
        assert_eq!(
            inspection.report.errors().count(),
            1,
            "{name}: exactly one hard blocker expected, got {:?}",
            inspection.report.blockers
        );
    }
}

#[test]
fn nested_jar_is_a_warning_not_a_hard_blocker() {
    let jar = fixture("nested-jar");
    if !jar.is_file() {
        let message =
            "nested-jar fixture is missing; run java/test-mods/blocked/build.ps1 (or .sh)";
        if required() {
            panic!("{message}");
        }
        eprintln!("skipping: {message}");
        return;
    }

    let inspection = inspect_jar(&jar, None).expect("fixture must inspect");
    assert_eq!(inspection.report.verdict, Verdict::RunnableSubset);
    assert_eq!(inspection.report.errors().count(), 0);
    let warning = inspection
        .report
        .blockers
        .iter()
        .find(|blocker| blocker.kind == BlockerKind::NestedJars)
        .expect("nested jar warning");
    assert_eq!(warning.severity, Severity::Warning);
    assert!(inspection
        .nested_jars
        .contains(&"META-INF/jarjar/nested.jar".to_string()));
}
