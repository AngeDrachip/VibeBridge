# Vibe progress

Last updated: 2026-11-08. Vibe 0.4.0.

This file separates what is implemented from what is merely planned. A feature
is only "working" at the highest level where it has actually been exercised.

## Status key

- **Implemented** - code exists in the repository.
- **Compiles** - the Rust workspace and Java side build.
- **Unit-tested** - Rust unit tests cover it.
- **Integration-tested** - a test starts the real CLI or the real JVM.
- **Tested with a real Forge mod** - a third-party mod was run and the result
  recorded. **Nothing in this repository currently meets this level.**
- **Confirmed inside Horizon** - observed in a running Horizon game. **Nothing
  in this repository currently meets this level.**

## Current milestone: 0.4.0 development

| Area | Status |
| --- | --- |
| Functional Forge registry subset (`DeferredRegister`, `RegistryObject`, `RegisterEvent`, `ForgeRegistries`) | Implemented, Java compiles, integration-tested |
| Real Minecraft data subset (`ResourceLocation`, `ResourceKey`, `Item`, `ItemStack`, `Block`, `CreativeModeTab`, `Component`, `Registry`) | Implemented, Java compiles, integration-tested |
| Ordered in-memory registries reported by `runtime.registry_snapshot` | Implemented, integration-tested |
| Nested-JAR extraction (`META-INF/jarjar/`, `META-INF/jars/`) into the mod class loader | Implemented, integration-tested |
| Nested classes counted as provided during analysis | Implemented, integration-tested |
| Eight embedded compatibility profiles across MC 1.12.2–26.3 | Implemented, unit-tested |
| Embedded compatibility database (`crates/vibe-core/data/compat.toml`) | Implemented, unit-tested |
| `vibe setup` (folders + prerequisite detection) | Implemented, integration-tested manually |
| `vibe mods` (scan, versions, profiles, missing deps, enable/disable) | Implemented, integration-tested manually |
| `vibe logs` (list and tail run logs) | Implemented, integration-tested manually |
| `vibe bench` (startup/load/lifecycle timings) | Implemented, integration-tested manually |
| `vibe compat` (evidence-based compatibility database) | Implemented, integration-tested manually |
| `vibe run` writes run/crash logs | Implemented, integration-tested manually |
| Required docs: `INSTALL.md`, `docs/INSTALLATION.md`, `docs/TROUBLESHOOTING.md`, `docs/COMPATIBILITY.md`, `docs/FIRST_LAUNCH.md` | Written |

## Earlier milestones

| Area | Status |
| --- | --- |
| Rust workspace (seven crates) | Implemented, compiles, unit-tested |
| Read-only jar discovery and inspection | Implemented, unit-tested, integration-tested |
| Forge metadata, class facts, SHA-256 | Implemented, unit-tested |
| Compatibility verdicts and grouped blockers | Implemented, unit-tested |
| Dependency/namespace/cycle planning | Implemented, unit-tested, integration-tested |
| Clean-room Forge API subset | Implemented, Java compiles, integration-tested with fixtures |
| JVM sidecar, isolated class loaders, lifecycle | Implemented, integration-tested with fixtures |
| Horizon `.hmod` export/validate/install | Implemented, unit-tested, integration-tested |
| Independent compatibility profiles | Implemented, unit-tested, integration-tested |
| Windows and Linux CI | Configured; see the latest GitHub Actions run |

## Build and test status

Locally verified for this milestone:

- `cargo fmt --all -- --check` (exit 0)
- `cargo clippy --workspace --all-targets -- -D warnings` (no warnings or errors)
- `VIBE_REQUIRE_JAVA_TESTS=1 cargo test --workspace`: all suites pass across
  unit, CLI integration, profile, runtime end-to-end, and blocked-fixture tests
- Java sidecar compiled with `--release 17`
- Fixtures rebuilt: `hello-forge`, `legacy-hello`, `registry-demo`,
  `nested-demo`, and nine blocked cases
- `vibe run` materialized `registry_demo:gem`, `registry_demo:gem_block`, and
  `registry_demo:registry_demo` in the real registries
- `nested_demo` loaded a class from `META-INF/jarjar/nested-lib.jar`
- `vibe bench` measured startup ~103 ms, load ~33 ms, lifecycle ~11 ms on the
  development machine (one fixture, not a general performance claim)

## Known limitations

- Horizon has no JVM. Vibe cannot make Horizon execute Java/Forge gameplay code.
  It can run a clean-room Forge API subset in a sidecar and export assets/data
  as `.hmod` content.
- Only `forge-1.20.1-47` and `forge-1.21.1-52` have a runtime backend. Every
  other profile is metadata-only.
- The runtime backend is the tested clean-room subset, not Minecraft parity.
  Most Minecraft classes, Mixin, coremods, access transformers, capabilities,
  networking, world access, entities, and rendering are still absent or blocked.
- Registries are in-memory data. Nothing is observable in game.
- No third-party Forge mod has been tested. The fixtures prove the plumbing,
  not general compatibility.
- No integration with a running Horizon game has been tested.
- Mod code is not sandboxed.
- Vibe 1.0 Beta is **not released** because its real-mod, in-game, and clean
  install requirements have not been met.

## Next concrete tasks

1. Obtain legally redistributable third-party Forge jars and run the required
   category tests; record exact results in `compat.toml`.
2. Verify a clean install against a running Horizon installation and record the
   outcome.
3. Expand the Minecraft API subset only with compile-time fixtures and entries
   in `COMPATIBILITY.md`.
4. Add profile-specific backend selection inside the Java sidecar instead of one
   shared shim.
5. Add a structured Rust embedding API so hosts do not parse CLI JSON.
