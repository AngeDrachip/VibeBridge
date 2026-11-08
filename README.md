# Vibe

**Run Forge mods on Horizon. Without porting.**

> [!IMPORTANT]
> Vibe is **not** a general-purpose Forge runtime, and it does **not** make
> Horizon run Java mods. Horizon is a Rust engine with no JVM. Vibe runs a
> small, clean-room subset of the Forge API in a separate Java process, reports
> when a mod needs anything outside that subset, and can export a jar's
> assets/data as a Horizon `.hmod` content mod. Most real Forge mods are blocked
> today. Vibe says so explicitly instead of pretending they work.

Vibe is an experimental compatibility layer written primarily in Rust, with a
Java sidecar that provides the Forge-facing API. It reads Forge mod jars without
modifying them, decides honestly whether it can execute their entrypoints,
runs the supported lifecycle in a real JVM, and can carry data-only content into
Horizon through Horizon's documented content-mod format.

Current release: **0.3.0** (independent Minecraft/Forge compatibility profiles).

## What Vibe does

- **Discovers and inspects** Forge jars and directories read-only. It parses
  `mods.toml` / `mcmod.info`, class files, annotations, referenced Minecraft and
  Forge classes, third-party library references, Mixin configs, access
  transformers, core plugins, and nested jars.
- **Classifies** each jar as `runnable-subset`, `content-only`, `blocked`, or
  `unknown`, with evidence for every blocker, grouped by package for large jars.
- **Plans a load** with duplicate detection, dependency ordering, missing
  dependency reporting, declared version ranges and mismatches, namespace
  conflict detection, cycle detection, and optional side-aware filtering.
- **Matches each mod to a compatibility profile** from the Minecraft and Forge
  versions it declares. Vibe's own release version is independent from those
  targets. A run uses exactly one profile, like a Proton prefix; mixed or
  undeclared targets are reported instead of guessed.
- **Runs the supported subset** in a real JVM: isolated class loaders,
  `@Mod` construction, Forge-shaped event buses, legacy `@Mod.EventHandler`
  callbacks, modern `@SubscribeEvent` listeners, sided setup filtering, and a
  lifecycle summary.
- **Exports content** from a jar to a Horizon `.hmod` bundle (manifest,
  `assets/`, `data/`) following Horizon's documented content format. Java code,
  code-registered content, Mixins, and coremods are not carried.
- **Never modifies the source jar.** Inspection and export open archives
  read-only. Export writes a new file and refuses to overwrite its source.

## What Vibe does not do

- It does not provide Minecraft classes (`net.minecraft.*`). Any mod that
  references them is blocked.
- It does not implement Mixin bytecode transformation, coremods, access
  transformers, or nested-jar loading.
- It does not extract or load native libraries.
- It does not implement the full Forge API, Forge registries, capabilities,
  networking, data generation, or resource reloading.
- It does not convert Minecraft pack formats during export.
- It does not patch Horizon, speak an undocumented Horizon protocol, or claim
  that Horizon can execute Java.
- It does not sandbox mod code. A mod runs with the same OS permissions as the
  Vibe process; only run jars you trust.

See [COMPATIBILITY.md](COMPATIBILITY.md) for the exact implemented surface and
[ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together.

## Requirements

- Rust 1.85 or newer (edition 2021).
- A JDK 17 or newer. Vibe compiles and runs its Java sidecar with `--release 17`.
- Optional: a Horizon installation with `horizon-ctl` for `vibe horizon probe`.
  Export works without a running game.

On Windows, make sure `JAVA_HOME` points at JDK 17+ before building. A Java 8
`javac` earlier on `PATH` is rejected.

## Build

```powershell
# Windows
$env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot"
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File java\test-mods\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File java\test-mods\blocked\build.ps1
cargo build --release
```

```bash
# Linux / macOS
export JAVA_HOME=/path/to/jdk-17
bash java/build.sh
bash java/test-mods/build.sh
bash java/test-mods/blocked/build.sh
cargo build --release
```

The binary is `target/release/vibe` (`target\release\vibe.exe` on Windows).

## Quick start

```powershell
target\release\vibe.exe doctor

# Inspect without executing any mod code.
target\release\vibe.exe inspect java\test-mods\build

# List the Minecraft/Forge compatibility profiles in this build.
target\release\vibe.exe profiles

# Build a dependency-ordered plan.
target\release\vibe.exe plan java\test-mods\build

# Plan a server only: client-only mods and dependencies are not required.
target\release\vibe.exe plan java\test-mods\build --side SERVER

# Require one profile and fail (exit 5) if a mod declares another target.
target\release\vibe.exe plan java\test-mods\build --profile forge-1.21.1-52

# Run the fixtures in the Java sidecar and print the lifecycle report.
target\release\vibe.exe run java\test-mods\build

# Run one explicit profile.
target\release\vibe.exe run java\test-mods\build --profile forge-1.21.1-52

# Export only the fixture's assets/data as a Horizon content mod.
target\release\vibe.exe horizon export java\test-mods\build\hello-forge.jar `
  --output dist\hello-forge.hmod

# Check the bundle against the documented Horizon content rules.
target\release\vibe.exe horizon validate dist\hello-forge.hmod

# Copy a validated bundle into <game-dir>\horizon-mods (dry run first).
target\release\vibe.exe horizon install dist\hello-forge.hmod `
  --game-dir "%APPDATA%\.minecraft" --dry-run

# Probe a local Horizon control surface (optional).
target\release\vibe.exe horizon probe
```

Every command accepts `--json` for machine-readable output.

### Environment variables

| Variable | Effect |
| --- | --- |
| `JAVA_HOME` | Preferred JDK/JRE when `--java` is not passed. |
| `VIBE_JAVA` | Explicit Java executable or home; takes precedence over `JAVA_HOME`. |
| `VIBE_RUNTIME_JAR` | Vibe runtime jar or classes directory; used when `--runtime` is not passed. |
| `VIBE_HORIZON_CTL` / `HORIZON_CTL` | Explicit `horizon-ctl` executable or folder. |
| `VIBE_REQUIRE_JAVA_TESTS` | Set to `1` to make integration tests fail instead of skip when Java or fixtures are missing. |

## Commands

| Command | Purpose | Exit codes |
| --- | --- | --- |
| `vibe doctor` | Check Java 17+, the Vibe runtime, and `horizon-ctl` availability. | `0` healthy, `1` not healthy |
| `vibe inspect <paths...>` | Read-only analysis of jars and directories. | `0` no inspection errors, `1` errors |
| `vibe plan <paths...>` | Dependency/namespace plan without executing code; `--side CLIENT\|SERVER` plans one side; `--profile <id>` requires one target. | `0` no inspection errors, `1` errors, `5` requested profile not satisfied |
| `vibe run <paths...>` | Start the JVM sidecar, load runnable jars under one profile, run the lifecycle. | `0` lifecycle ok, `3` nothing runnable, `4` runtime failure, `5` profile conflict or no runtime backend |
| `vibe profiles` | List the embedded Minecraft/Forge compatibility profiles. `--runtime-only` hides metadata-only targets. | `0` success |
| `vibe horizon probe` | Locate `horizon-ctl` and ask for state. | `0` found, `1` not found |
| `vibe horizon export <jar>` | Write a `.hmod` content bundle from a jar. | `0` success, `1` error |
| `vibe horizon validate <hmod>` | Check a bundle against the documented Horizon rules Vibe can verify. | `0` valid, `2` invalid, `1` unreadable |
| `vibe horizon install <hmod>` | Copy a validated bundle into `<game-dir>/horizon-mods/`. | `0` success, `1` error |

Useful `run` options:

- `--side SERVER|CLIENT` selects the lifecycle side (default `SERVER`).
- `--profile <id>` requires one compatibility profile. Vibe fails with exit `5`
  instead of starting a JVM when a runnable mod declares another target or when
  the profile is metadata-only.
- `--java <file-or-home>` selects a JDK/JRE.
- `--runtime <jar-or-classes>` selects the Vibe Java runtime.
- `--heap-mb <n>` sets the JVM maximum heap (default 2048).
- `--timeout-ms <n>` sets startup/request timeouts (default 30000).
- `--include-blocked` sends blocked jars to the runtime for diagnostics only.
  It does not make unsupported classes exist.

Useful `plan` options:

- `--side CLIENT|SERVER` keeps mods declared for that side, lists mods declared
  for the other side as excluded, and ignores dependencies that only apply to
  the other side. Without `--side`, every scanned mod is planned. A mod with no
  declared side is kept on both sides.
- `--profile <id>` requires every planned mod to match that profile. Without it,
  Vibe reports mods that resolve to different profiles as a conflict.

Useful export options:

- `--id`, `--name`, `--version`, `--side` override manifest values.
- `--namespace <ns>` claims an extra namespace present in the jar. Horizon
  rejects files in namespaces the content mod does not own. Only claim
  namespaces the original mod really owns.
- `--force` replaces an existing output file.
- `--no-vanilla-tags`, `--no-convention-tags`, `--no-atlases` exclude the
  corresponding optional content groups.

## Compatibility profiles

Vibe's release version is independent from Minecraft, Forge, and Horizon
versions. Each profile fixes one Minecraft version, one Forge loader family, a
Java level, and the Vibe backend that serves it. The matrix is embedded in the
binary from `crates/vibe-core/data/profiles.toml`.

| Profile | Minecraft | Forge | Java | Status |
| --- | --- | --- | --- | --- |
| `forge-1.20.1-47` | 1.20.1 | 47.x | 17 | `experimental-subset` |
| `forge-1.21.1-52` | 1.21.1 | 52.x | 21 | `experimental-subset` |
| `forge-26.3-66` | 26.3 | 66.x | 21 | `metadata-only` |

`experimental-subset` means Vibe has a tested clean-room Forge API subset for
that loader family; it does not mean Minecraft gameplay works.
`metadata-only` means Vibe can detect, plan, inspect, and export assets/data for
that target but has no runtime backend.

Selection rules:

- A modern mod declares its target through a `minecraft` dependency and
  `loaderVersion` in `mods.toml`; a legacy mod uses `mcmod.info`
  (`acceptedMinecraftVersions` or `mcversion`).
- Vibe resolves a profile only when exactly one profile matches both the
  Minecraft range and the Forge loader range.
- A missing declaration is reported as `undeclared`, never guessed. Pass
  `--profile <id>` to choose explicitly.
- An explicit profile is still checked against the mod's declarations; it does
  not override a declared mismatch.
- One JVM run uses one profile. Mods targeting different runtime profiles cause
  exit `5` instead of being silently mixed.
- A metadata-only profile is inspectable and exportable but `vibe run` refuses
  it with exit `5`.

`vibe inspect` reports each jar's resolution, `vibe plan` reports a plan-level
profile and conflicts, and `vibe run` passes the active profile and backend to
the Java sidecar. The sidecar rejects a profile or backend it does not provide.

## Compatibility verdicts

| Verdict | Meaning |
| --- | --- |
| `runnable-subset` | No hard blocker was found, and Vibe's Forge API subset can construct the declared `@Mod` entrypoint. This is **not** a promise of general Forge compatibility. |
| `content-only` | There is no Java entrypoint to execute (for example a low-code or resource pack style mod). Assets/data may still be exportable. |
| `blocked` | At least one hard blocker exists (Minecraft classes, unsupported Forge API, Mixin, coremod, access transformer, missing entrypoint, unsupported loader, or Java version mismatch). |
| `unknown` | The archive is not recognisably a Forge mod. |

Inspection never executes mod code. `vibe run` only sends `runnable-subset`
jars by default. See [COMPATIBILITY.md](COMPATIBILITY.md) for the exact API list.

## Horizon export

A `.hmod` produced by Vibe is a zip containing:

```text
horizon-mod.toml
assets/<namespace>/...
data/<namespace>/...
```

The manifest declares Horizon content API `0.2` and the `registries`
capability, plus id, name, version, side, authors, description, and any extra
claimed namespaces. Export:

- copies resource bytes unchanged;
- skips Java classes, `META-INF`, and unrelated resources;
- includes `data/minecraft/tags/` additions and `data/c/tags/` convention tags
  by default (both can be disabled);
- enforces Horizon's documented limits (65,536 files, 16 MiB per file, 512 MiB
  total);
- validates the generated manifest and id before moving the finished bundle
  into place;
- reports namespaces it left out and files left outside claimed namespaces;
- records every included file with its kind and size, and up to 512 skipped
  files with a reason (`unclaimed_namespace`, `unsafe_path`, `not_content`) in
  `--json`;
- reports `pack.mcmeta` pack formats and the jar's declared Minecraft
  dependency range, without translating or upgrading either;
- never upgrades old Minecraft pack formats.

The original jar is not changed. The export result includes its SHA-256 so the
source can be identified.

`vibe horizon validate` opens an existing `.hmod` read-only and checks the
manifest, id/side/API values, claimed namespaces, entry paths, documented size
limits, and first-level block/item declarations. It returns exit code `2` when
the bundle is invalid. This is not a replacement for Horizon's own
`cargo horizon-mod check`; it covers what Vibe can verify from the documented
format.

`vibe horizon install` validates a bundle and copies it into
`<game-dir>/horizon-mods/`, the folder Horizon reads content mods from. It does
not enable the mod: Horizon keeps that switch in `horizon-mods.txt`, and Vibe
never edits a game's settings. Use `--dry-run` to see the destination and hash
without writing, and `--force` to replace a file with the same name.

## Repository layout

```text
crates/
  vibe-core/      domain types, metadata, class parser, version ranges, profiles, analysis
  vibe-loader/    recursive discovery and read-only jar inspection
  vibe-registry/  mod registration, dependency order, namespace conflicts
  vibe-events/    typed host event bus and lifecycle state machine
  vibe-bridge/    JSON-lines protocol, JVM discovery, runtime process client
  vibe-horizon/   horizon-ctl probe and .hmod content export
  vibe-cli/       the `vibe` command
java/
  vibe-forge-api/ clean-room Forge-shaped API subset used by mods
  vibe-runtime/   Java bridge process and mod lifecycle manager
  test-mods/      two compiled integration fixtures
```

## Development

```powershell
$env:PATH = "$env:USERPROFILE\.vibe-rust\bin;C:\Users\angen\.vibe-tools\w64devkit\bin;$env:PATH"
$env:VIBE_REQUIRE_JAVA_TESTS = "1"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`VIBE_REQUIRE_JAVA_TESTS=1` turns missing Java or missing fixture jars into test
failures instead of skips. The integration tests start a real JVM, load both
runnable fixtures, run the lifecycle, and export the fixture resources as a
Horizon content bundle. A second suite inspects compiled blocked-case jars and
asserts one hard blocker per fixture (Minecraft classes, unsupported Forge API,
external library, Mixin, access transformer, coremod, missing entrypoint,
unsupported loader) plus the nested-jar warning. The CI workflow is configured
to run the same checks on Linux and Windows with JDK 17.

See [CONTRIBUTING.md](CONTRIBUTING.md) before adding API surface. The project is
clean-room: do not copy Forge, Minecraft, or Horizon source into this repository.

## Roadmap

The short version: deepen honest diagnostics and content export first, expand
the Java subset only with tests, and use only documented Horizon surfaces.
Details are in [ROADMAP.md](ROADMAP.md).

## License

MIT. See [LICENSE](LICENSE).
