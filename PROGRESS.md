# Vibe progress

Last updated: 2026-11-08. Vibe 0.3.0.

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

## Current milestone: independent compatibility profiles

| Area | Status |
| --- | --- |
| Vibe release version independent from Minecraft/Forge | Implemented, unit-tested, integration-tested |
| Embedded profile matrix (`crates/vibe-core/data/profiles.toml`) | Implemented, unit-tested |
| Modern `mods.toml` target detection | Implemented, unit-tested |
| Legacy `mcmod.info` target detection | Implemented, unit-tested |
| Matched / ambiguous / unsupported / undeclared resolution | Implemented, unit-tested, integration-tested |
| `vibe profiles` | Implemented, integration-tested |
| `--profile` for `inspect` / `plan` / `run` | Implemented, integration-tested |
| Per-mod profile data in inspection/planning JSON | Implemented, integration-tested |
| One runtime-capable profile per JVM | Implemented, integration-tested |
| Profile/backend validation in the bridge | Implemented, integration-tested with the real runtime |
| Metadata-only profile refused by `run` | Implemented, integration-tested |

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
| Windows and Linux CI | Configured; see the latest GitHub Actions run |

## Build and test status

Locally verified for this milestone:

- `cargo fmt --all -- --check` (exit 0)
- `cargo clippy --workspace --all-targets -- -D warnings` (no warnings or errors)
- `VIBE_REQUIRE_JAVA_TESTS=1 cargo test --workspace`: **77 tests passed**, 0
  failed across the unit, CLI integration, profile, and runtime end-to-end
  suites
- Java sidecar compiled with `--release 17`
- Both fixture jars rebuilt with declared Minecraft 1.21.1 / Forge 52 metadata
- `vibe run` loaded both fixtures under `forge-1.21.1-52`, phase `complete`,
  `9/9` event records succeeded
- New profile CLI suite: 10 tests passed

## Known limitations

- Horizon has no JVM. Vibe cannot make Horizon execute Java/Forge gameplay code.
  It can run a clean-room Forge API subset in a sidecar and export assets/data
  as `.hmod` content.
- `forge-26.3-66` is metadata-only: no runtime backend.
- The runtime backend is the tested clean-room subset, not Minecraft parity.
  Minecraft classes, Mixin, coremods, access transformers, nested jars, and most
  Forge APIs are still absent or blocked.
- No third-party Forge mod has been tested. The fixtures prove the plumbing,
  not general compatibility.
- No integration with a running Horizon game has been tested.
- Mod code is not sandboxed.

## Next concrete tasks

1. Add profile-specific backend selection inside the Java sidecar instead of one
   shared shim, starting with the 1.20.1/47 and 1.21.1/52 differences that are
   already known.
2. Test one small, permissively licensed third-party Forge mod that does not
   reference Minecraft classes, and record the exact result.
3. Add a structured Rust embedding API so hosts do not parse CLI JSON.
4. Continue expanding the Forge API subset only with compile-time fixtures and
   entries in `COMPATIBILITY.md`.
