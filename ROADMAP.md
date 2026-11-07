# Roadmap

Vibe's roadmap is ordered by honesty and usefulness, not by the number of Forge
classes it can imitate. Each milestone is complete only when behavior is tested
and documented.

## Guiding constraints

- Horizon does not run Java. Vibe can run a JVM sidecar and export content, but
  it cannot make Horizon execute Java mods.
- No invented Horizon IPC. Use `horizon-ctl` and the documented `.hmod` format.
- No silent fallbacks. Unsupported code is reported as blocked.
- No copied Forge/Minecraft code. The API shim is clean-room.
- No compatibility claim without a test that can be inspected and rerun.

## 0.1.0 - Proof of concept (done)

- [x] Rust workspace: core, loader, registry, events, bridge, Horizon, CLI.
- [x] Read-only Forge metadata and class-file inspection.
- [x] Verdicts with evidence: `runnable-subset`, `content-only`, `blocked`,
      `unknown`.
- [x] Dependency/load plan with duplicates, missing dependencies, namespace
      conflicts, and cycles.
- [x] Clean-room Forge API subset and Java sidecar.
- [x] Isolated class loaders, `@Mod` construction, mod/Forge event buses,
      legacy handlers, side filtering.
- [x] Real-JVM integration tests with two compiled fixtures.
- [x] Horizon `horizon-ctl` probe and validated `.hmod` content export.
- [x] CLI with human and JSON output.
- [x] CI workflow for Linux + Windows with JDK 17, fmt, clippy, and tests.
- [x] Documentation, compatibility matrix, and license.

## 0.2 - Better diagnosis before execution

Goal: make a `blocked` result more actionable, and make a `runnable-subset`
result easier to trust.

- [x] Resolve more of the mod's own class dependencies during inspection and
      report missing third-party libraries as their own blocker instead of
      discovering them only at construction time.
- [x] Parse and report `mods.toml` dependency ranges more thoroughly, including
      side-specific dependencies and side-aware `vibe plan --side` filtering.
- [x] Report pack-format/version hints during content export so users can see
      when resources target a different Minecraft era.
- [x] Add per-entry export explanations in `--json` (why each file was included
      or skipped).
- [ ] Improve `inspect` evidence grouping for large jars.
- [x] Add fixtures for blocked cases: Minecraft reference, Mixin, access
      transformer, coremod, unsupported Forge API, nested jar.

Exit criteria: each blocker kind has a fixture and an asserted report; no new
runtime API is added.

## 0.3 - Expand the runtime subset carefully

Goal: support more Forge-shaped mods that do not touch Minecraft classes.

- [ ] Add lifecycle events only when their semantics can be represented
      honestly (for example, a clearly documented synthetic server-start phase
      if Vibe gains a meaningful server concept).
- [ ] Support additional listener registration patterns used by event-only
      mods.
- [ ] Improve class-loader isolation and cycle diagnostics for multi-jar mods.
- [ ] Expose a stable Rust embedding API with structured reports instead of
      JSON-only results.
- [ ] Add stress tests for timing, timeouts, malformed frames, and runtime
      crashes.

Exit criteria: every new API name appears in both implementation lists, has a
compile-time fixture, and appears in `COMPATIBILITY.md`. No Minecraft class is
implemented.

## 0.4 - Horizon workflow integration

Goal: reduce manual steps while staying on documented surfaces.

- [ ] Adopt additional documented `horizon-ctl` commands as Horizon publishes
      them (deploy/install/query), with tests that skip cleanly when no game is
      running.
- [ ] Detect and report a Horizon-managed game directory in `doctor`.
- [ ] Add a dry-run mode that shows exactly what would be installed without
      changing a game directory.
- [ ] Add bundle reproducibility checks (stable entry order, manifest hash).
- [ ] Document a complete "export, install, verify" workflow.

Exit criteria: Vibe never invokes an undocumented endpoint, and every Horizon
operation has a dry run and an offline test.

## Later / exploratory

- Content export fidelity: pack-format-aware warnings, atlas/tag merging rules,
  namespace ownership checks against a real Horizon install.
- Packaging: prebuilt binaries, reproducible releases, signed checksums.
- Diagnostics: HTML/JSON compatibility reports, SHA-256-keyed result cache.
- Optional performance work on very large mod packs.

Nothing in this section is a commitment.

## Explicit non-goals

- Implementing Minecraft (`net.minecraft.*`) or a general Forge runtime.
- Mixin transformation, coremods, access transformers, or nested-jar loading
  without a separate design that can be tested.
- Running Java inside the Horizon process.
- Guessing at Horizon internals or using private IPC.
- Claiming that a fixture result proves third-party mod compatibility.
- Modifying the original mod jar.
