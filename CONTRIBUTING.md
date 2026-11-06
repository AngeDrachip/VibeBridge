# Contributing

Vibe is a compatibility layer, so trust is part of the product. A contribution
that claims more than the tests prove is worse than no contribution. Please keep
that in mind for code, fixtures, and documentation.

## Ground rules

- **Clean-room.** Do not copy Forge, Minecraft, Horizon, or another loader's
  source into this repository. The Java shim must be written from public API
  shapes and observed behavior, not copied implementation.
- **Never modify input jars.** Inspection and export must open source archives
  read-only. Export writes a new file and refuses to target its input.
- **No invented APIs.** Do not add a Forge class or method just to make a stack
  trace disappear. If Vibe cannot implement the semantics, analysis should
  report a blocker.
- **No fake Horizon integration.** Use documented `horizon-ctl` commands and the
  documented `.hmod` format only.
- **No untested claims.** Update `COMPATIBILITY.md` when behavior changes, and
  state what was actually run.
- **Warnings are not errors.** If something is reported as a warning, make sure
  the documentation explains why it does not block.

## Prerequisites

- Rust 1.85 or newer.
- JDK 17 or newer. Set `JAVA_HOME` if `javac` on `PATH` is older.
- Git. Horizon is optional for export tests but useful for `horizon probe`.

## Build and test

Windows (PowerShell):

```powershell
$env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot"
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File java\test-mods\build.ps1
$env:VIBE_REQUIRE_JAVA_TESTS = "1"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Linux / macOS:

```bash
export JAVA_HOME=/path/to/jdk-17
bash java/build.sh
bash java/test-mods/build.sh
export VIBE_REQUIRE_JAVA_TESTS=1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build the Java side before the Rust tests: the integration tests launch the
built runtime and fixtures. `VIBE_REQUIRE_JAVA_TESTS=1` is required in CI so a
missing JDK cannot turn into a silent skip.

Useful manual checks:

```powershell
target\debug\vibe.exe doctor
target\debug\vibe.exe inspect java\test-mods\build
target\debug\vibe.exe run java\test-mods\build
target\debug\vibe.exe horizon export java\test-mods\build\hello-forge.jar --output dist\hello-forge.hmod --force
```

## Adding Forge API surface

Adding an API name is a compatibility promise. Do all of the following:

1. Implement the clean-room class or method in `java/vibe-forge-api` and wire
   any runtime behavior in `java/vibe-runtime`.
2. Add the exact class name to
   `crates/vibe-core/src/analysis.rs::SUPPORTED_FORGE_API`.
3. Add the same name to
   `java/vibe-runtime/src/main/java/dev/vibe/runtime/VibeRuntime.java::SUPPORTED_FORGE_API`.
4. Add or extend a fixture under `java/test-mods` that compiles against the API
   and exercises the behavior.
5. Add an assertion to the Rust integration test or Java-side behavior test.
6. Update `COMPATIBILITY.md` (API list, lifecycle, or known gaps) and
   `CHANGELOG.md`.
7. If the API cannot be implemented faithfully, do not add it. Add a blocker
   with evidence instead.

Keep analysis conservative: a false `blocked` is recoverable with
`--include-blocked` diagnostics; a false `runnable-subset` is a broken promise.

## Adding a fixture

- Put it under `java/test-mods/<name>`.
- Use `META-INF/mods.toml` with a unique mod id.
- Keep resources small and deterministic.
- Add it to both `java/test-mods/build.ps1` and `java/test-mods/build.sh`.
- Document what the fixture proves in `COMPATIBILITY.md`.
- Add it to the relevant integration test path discovery if the test does not
  already consume the whole fixture directory.

Fixtures must be readable, buildable, and free of copied third-party code.

## Code style

- Run `cargo fmt` before committing.
- Keep `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Java is compiled with `--release 17 -Xlint:all`; keep it warning-free.
- Prefer explicit types and clear error messages over clever shorthand.
- Comments should explain a boundary or a non-obvious decision, not restate the
  line below.
- Do not add dependencies without a reason; the workspace is intentionally
  small.

## Pull requests

A good pull request includes:

- a short description of the behavior change and its boundary;
- tests that fail before the change and pass after it;
- updated compatibility/docs text when user-visible behavior changes;
- a note about anything that was not tested;
- no build outputs, jars, `target/`, `dist/`, or editor files.

Run the full local check above before opening the PR. CI runs the same matrix on
Linux and Windows with JDK 17.

## Reporting compatibility

If you add a real-world mod result, record:

- jar name and version;
- SHA-256;
- `vibe inspect --json` output;
- `vibe run --json` output for a `runnable-subset` result;
- Java version from `vibe doctor`;
- the exact claim being made (lifecycle run vs. content export only).

Do not add a third-party mod to the compatibility table based on a screenshot or
a single successful load. Add a reproducible test or describe the result as an
anecdote outside the table.
