# Changelog

All notable changes to Vibe are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0-beta.1] - 2026-11-08

First beta. Adds a real Horizon integration on top of the 0.4.0 runtime.

### Added

- `vibe horizon connect`: verifies a live Horizon instance. Checks
  `horizon-ctl` discovery, control-socket reachability, player-in-world state,
  pending chunk count, Minecraft version, and the Horizon content API version.
  Exits `0` when ready and `1` otherwise, with one line per check.
- `horizon_ctl` discovery inside Horizon launcher installations
  (`versions/<tag>/horizon-ctl.exe`), so `VIBE_HORIZON_CTL` is optional.
- Horizon content export (`vibe horizon export`) validated against the
  documented `horizon-mod.toml` rules, plus `vibe horizon validate` and
  `vibe horizon install` with `--dry-run` and `--force`.
- `vibe horizon install` turns the mod on in `horizon-mods.txt`
  (`mod.<id>:true`), because Horizon leaves a content mod off until that line
  exists, and defaults `--game-dir` to the Horizon game home
  (`HORIZON_HOME`, then `%APPDATA%\horizon`), not the launcher root.
- 15 `vibe-horizon` unit tests covering discovery, connect checks, export,
  validation, and install.

### Changed

- README documents the beta status and the new `vibe horizon connect` command.
- `horizon-ctl` discovery prefers a launcher install that has prepared game
  files, so a newer but unprepared install no longer hides the one that can
  actually start the game.

### Fixed

- Launcher discovery reads the `selected` key the launcher writes, not only the
  older `selected_version`.
- `vibe horizon validate` reads bundles written by Windows tools: entry names
  with backslashes and a UTF-8 byte-order mark no longer make a declaration
  unreadable.

### Known limitations

- Horizon content mods carry assets and data only. Java entrypoints, Mixins,
  access transformers, and coremods are reported, never executed.
- Horizon targets Minecraft 26.3. A jar written for an older pack format is
  copied byte for byte and may not be readable by Horizon.

## [0.4.0] - 2026-11-08

### Added

- A functional Forge registry subset: `DeferredRegister`, `RegistryObject`,
  `RegisterEvent`, `ForgeRegistries`, `IForgeRegistry`, `VibeRegistry`, and
  `VibeRegistryManager`. Entries live in ordered in-memory registries and are
  reported by `runtime.registry_snapshot`.
- A real Minecraft data subset: `ResourceLocation`, `ResourceKey`, `Registry`,
  `Registries`, `Component`, `Item`/`Item.Properties`, `ItemStack`,
  `Block`/`Block.Properties`, `BlockBehaviour`/`Properties`, and
  `CreativeModeTab`/`Builder`. These are data types with real behavior, not
  no-op placeholders.
- A `register` lifecycle phase that posts `RegisterEvent` per supported registry
  and materializes deferred entries.
- Nested-JAR loading from `META-INF/jarjar/` and `META-INF/jars/`: archives are
  extracted to a temporary directory, added to the owning mod's class loader,
  and removed after the session.
- Analysis treats nested-archive classes as provided, so bundled libraries are
  no longer misreported as external.
- Eight embedded compatibility profiles: `forge-1.12.2-14`, `forge-1.16.5-36`,
  `forge-1.18.2-40`, `forge-1.19.2-43`, `forge-1.20.1-47`, `forge-1.20.4-49`,
  `forge-1.21.1-52`, and `forge-26.3-66`.
- An embedded compatibility database (`crates/vibe-core/data/compat.toml`) with
  evidence for every entry and statuses `FULL`, `HIGH`, `PARTIAL`,
  `CONTENT_ONLY`, `BLOCKED`, and `UNKNOWN`.
- CLI commands: `vibe setup`, `vibe mods` (scan, versions, profiles, missing
  dependencies, enable/disable), `vibe logs`, `vibe bench`, and `vibe compat`.
- `vibe run` now writes `logs/vibe-run-<session>.log`,
  `logs/vibe-run-<session>.json`, and `logs/vibe-crash-<session>.log` on failure.
- Required documentation: `INSTALL.md`, `docs/INSTALLATION.md`,
  `docs/TROUBLESHOOTING.md`, `docs/COMPATIBILITY.md`, and
  `docs/FIRST_LAUNCH.md`.
- Fixtures `registry-demo` and `nested-demo`, plus integration tests that prove
  registry materialization and nested-JAR class loading.

### Changed

- Only unsupported `net.minecraft.*` classes block a mod; the implemented
  Minecraft subset is whitelisted during analysis.
- `blocked-unsupported-forge` now references genuinely unsupported
  `net.minecraftforge.common.capabilities.Capability`.
- Vibe and the Java runtime report version `0.4.0`.

### Known limitations

- Only `forge-1.20.1-47` and `forge-1.21.1-52` have a runtime backend. All other
  profiles are metadata-only.
- Registries are in-memory only. There is no Minecraft world, so items, blocks,
  and creative tabs cannot be observed in game.
- Mixin, coremods, access transformers, capabilities, networking, entities, and
  rendering are not implemented.
- No third-party Forge mod has been validated. The compatibility database marks
  representative mods `UNKNOWN`.
- Vibe 1.0 Beta is not released because its real-mod, in-game, and clean-install
  requirements have not been met.

## [0.3.0] - 2026-11-08

### Added

- Version-independent compatibility profiles. Vibe's own release version is now
  separate from the Minecraft, Forge, and Horizon versions it targets.
- `crates/vibe-core/data/profiles.toml`: an embedded matrix with
  `forge-1.20.1-47`, `forge-1.21.1-52`, and `forge-26.3-66`, including real
  Minecraft ranges, Forge loader ranges, Java levels, backends, and statuses.
- `ProfileMatrix` resolution: a mod is matched from its declared Minecraft and
  Forge ranges. Missing declarations are `undeclared`, several matches are
  `ambiguous`, and no match is `unsupported`; Vibe never guesses.
- `vibe profiles [--runtime-only]` lists the embedded matrix in human or JSON
  form.
- `--profile <id>` for `inspect`, `plan`, and `run`. Explicit profiles are
  checked against declarations rather than assumed.
- Per-mod profile data in inspection and planning output (`target_profile`,
  `profile_status`, declared ranges, and reasons) plus plan-level
  `profile` / `profile_conflicts`.
- `vibe run` enforces one runtime-capable profile per JVM, skips mods that
  resolve to a metadata-only target with an explicit reason, and exits `5` on a
  profile conflict or an unsupported explicit profile.
- The bridge handshake and `runtime.info` now report `supported_profiles`,
  `active_profile`, and `backend`; `mods.load` carries and validates the active
  profile and backend. The Java runtime rejects a profile or backend it does not
  provide.
- Metadata parsing now records the declared Minecraft and Forge ranges from
  modern `mods.toml` dependencies and legacy `mcmod.info`, with conflict
  handling instead of guessing.
- CLI integration tests cover matched, unsupported, ambiguous, mixed-profile,
  explicit-profile, metadata-only, and undeclared-target cases.

### Changed

- `hello-forge` and `legacy-hello` now declare Minecraft `[1.21.1,1.22)` and
  Forge `[52,53)`, so they resolve to `forge-1.21.1-52`.
- Vibe and the Java runtime report version `0.3.0`.

### Known limitations

- `forge-26.3-66` is metadata-only: Vibe can inspect, plan, and export content
  for the Horizon-aligned target, but cannot execute Forge 66 Java code.
- A profile is a version target, not a compatibility guarantee. The runtime
  backend remains the tested clean-room Forge API subset.
- Horizon still has no JVM and cannot execute Java/Forge gameplay code.

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
