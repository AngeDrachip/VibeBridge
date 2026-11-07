# Changelog

All notable changes to Vibe are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-11-07

### Added

- `vibe horizon validate <hmod>`: read-only checks of a content bundle's
  manifest, namespaces, entry paths, documented limits, and block/item
  declarations.
- `vibe horizon install <hmod> --game-dir <dir>`: validate a bundle and copy it
  into Horizon's `horizon-mods/` folder, with `--dry-run` and `--force`.
- SHA-256 of every validated or installed bundle.
- Inspection now compares every class the jar references with the classes it
  ships and the JDK/platform/Vibe shims, and reports missing third-party
  libraries as an `ExternalClasses` blocker with the classes and library groups
  in `--json`.
- Blocker evidence is grouped by Java package or path prefix, with a bounded
  sample, full `evidence_total`, and explicit truncation notes for large jars.
- Content export now records every included file (path, kind, bytes) and up to
  512 skipped files with a reason, reports `pack.mcmeta` pack formats and the
  declared Minecraft dependency range, and prints a skip summary in the human
  output.
- Nine compiled blocked-case fixtures with an integration test asserting one
  hard blocker per case (Minecraft classes, unsupported Forge API, external
  library, Mixin, access transformer, coremod, missing entrypoint, unsupported
  loader) and the nested-jar warning. CI builds them on Linux and Windows.
- Side-aware load planning: `vibe plan --side CLIENT|SERVER` excludes mods
  declared for the other side (listing them with a reason), ignores
  dependencies that only apply to the other side, and reports each missing
  dependency's declared side and version range.
- Present dependencies are checked against their declared Maven range and
  reported as version mismatches when they do not match.

### Changed

- Repository and homepage URLs now point at
  `https://github.com/AngeDrachip/VibeBridge`.

## [0.1.0] - 2026-11-06

### Added

- Rust workspace with `vibe-core`, `vibe-loader`, `vibe-registry`,
  `vibe-events`, `vibe-bridge`, `vibe-horizon`, and `vibe-cli`.
- Read-only discovery and inspection of Forge jars: `mods.toml`,
  `mcmod.info`, class files, Java release, annotations, referenced Minecraft
  and Forge classes, Mixin configs, access transformers, core plugins, nested
  jars, native libraries, and assets/data namespaces.
- Compatibility verdicts with evidence: `runnable-subset`, `content-only`,
  `blocked`, `unknown`.
- Maven version and range matching for Forge metadata.
- Load planning with duplicate mod ids, missing dependencies, namespace
  conflicts, cycles, and deterministic dependency ordering.
- Typed Rust host event bus and lifecycle state machine.
- JSON-lines Rust <-> Java bridge with protocol handshake, request timeouts,
  event collection, bounded stderr capture, and process cleanup.
- Java 17+ discovery, including `JAVA_HOME`, PATH, and common JDK locations.
- Clean-room Forge API subset covering mod annotations, event buses, lifecycle
  events, mod container/list/context, and a log4j logging shim.
- Java runtime with per-jar class loaders, `@Mod` construction, legacy
  `@Mod.EventHandler` dispatch, reflective and typed `@SubscribeEvent`
  listeners, side-aware `@Mod.EventBusSubscriber` registration, and a
  constrained lifecycle.
- Horizon `horizon-ctl` discovery and probe using `--help` and `state`.
- Horizon `.hmod` content export with manifest generation, namespace claiming,
  optional vanilla/convention tags and atlases, documented size limits,
  manifest validation, deterministic entry order, atomic replacement, and
  source-jar preservation.
- CLI commands: `doctor`, `inspect`, `plan`, `run`, `horizon probe`,
  `horizon export`; all report results in human-readable or `--json` form.
- Java and fixture build scripts for Windows and Unix-like systems.
- Real-JVM end-to-end tests that load both fixtures, run the lifecycle, and
  export resources as a Horizon content bundle.
- Documentation: README, architecture, compatibility, roadmap, contributing,
  and MIT license.
- GitHub Actions CI matrix for Linux and Windows with JDK 17, formatting,
  clippy, and tests.

### Known limitations

- Most real Forge mods reference Minecraft classes and are reported as blocked.
- Mixin, coremods, access transformers, and nested jars are not executed.
- Native libraries are not extracted or loaded.
- No Minecraft registries, capabilities, networking, commands, world access, or
  client rendering.
- Third-party library references are not exhaustively analysed.
- Export copies resource bytes unchanged and does not upgrade pack formats.
- Mod code is not sandboxed.
