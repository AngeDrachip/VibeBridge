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

## 0.2.0 - Better diagnosis before execution (done)

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
- [x] Improve `inspect` evidence grouping for large jars.
- [x] Add fixtures for blocked cases: Minecraft reference, Mixin, access
      transformer, coremod, unsupported Forge API, nested jar.

Exit criteria: each blocker kind has a fixture and an asserted report; no new
runtime API is added.

## 0.3.0 - Independent compatibility profiles (done)

Goal: separate Vibe's release version from Minecraft, Forge, and Horizon
versions, and make one profile per run explicit.

- [x] Embedded profile matrix with real Minecraft/Forge ranges, Java levels,
      backends, and statuses.
- [x] Modern `mods.toml` and legacy `mcmod.info` target detection.
- [x] Matched / ambiguous / unsupported / undeclared resolution without
      guessing.
- [x] `vibe profiles`, `--profile` on `inspect`/`plan`/`run`, and per-mod
      profile data in JSON.
- [x] One runtime-capable profile per JVM, with exit `5` on conflicts and on
      metadata-only targets.
- [x] Profile/backend validation in the bridge handshake and `mods.load`.

Exit criteria: a mod's declared target is reported, mixed or missing targets are
refused rather than guessed, and the runtime never claims a backend it does not
provide.

## 0.4.0 - Functional subset, multi-version profiles, tooling (done)

Goal: move beyond event-only plumbing by implementing a real registry subset,
loading nested JARs, broadening the profile matrix, and making the tool usable
end to end.

- [x] Functional Forge registries: `DeferredRegister`, `RegistryObject`,
      `RegisterEvent`, `ForgeRegistries`, ordered in-memory registries.
- [x] Real Minecraft data subset: `ResourceLocation`, `ResourceKey`, `Registry`,
      `Item`, `ItemStack`, `Block`, `CreativeModeTab`, `Component`.
- [x] A `register` lifecycle phase that materializes deferred entries.
- [x] Nested-JAR extraction from `META-INF/jarjar/` and `META-INF/jars/`.
- [x] Nested classes counted as provided during analysis.
- [x] Eight embedded profiles spanning MC 1.12.2–26.3.
- [x] Embedded compatibility database with evidence-based statuses.
- [x] `vibe setup`, `vibe mods`, `vibe logs`, `vibe bench`, `vibe compat`.
- [x] Run/crash logs from `vibe run`.
- [x] Required installation, troubleshooting, compatibility, and first-launch
      documentation.

Exit criteria: registries materialize in tests, nested JARs load in tests, every
profile is unit-tested, and no compatibility claim lacks evidence. **Met.**

## 0.5 - Real-mod validation and 1.0 Beta

Goal: earn the right to call Vibe 1.0 Beta by testing real mods and a clean
install.

- [ ] Obtain legally redistributable third-party Forge jars across the required
      categories and record exact results in `compat.toml`.
- [ ] Demonstrate at least one mod with actual in-game functionality in a
      running Horizon installation.
- [ ] Test the installation guide from a clean Windows machine.
- [ ] Verify Java, Horizon, original Forge JAR, MC/Forge version, and dependency
      detection on that clean install.
- [ ] Expand the Minecraft API subset only where real-mod tests justify it.
- [ ] Publish `1.0.0-beta.1`, then `1.0.0` when all criteria are met.

Exit criteria: every 1.0 Beta requirement in `docs/COMPATIBILITY.md` is backed
by reproducible evidence. Until then, no 1.0 Beta release is published.

## 0.6 - Horizon workflow integration

Goal: reduce manual steps while staying on documented surfaces.

- [ ] Adopt additional documented `horizon-ctl` commands as Horizon publishes
      them (deploy/install/query), with tests that skip cleanly when no game is
      running.
- [ ] Detect and report a Horizon-managed game directory in `setup`/`doctor`.
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

- Implementing Minecraft or a general Forge runtime. Vibe implements a small,
  documented data/API subset, not the game.
- Mixin transformation, coremods, or access transformers.
- Running Java inside the Horizon process.
- Guessing at Horizon internals or using private IPC.
- Claiming that a fixture result proves third-party mod compatibility.
- Modifying the original mod jar.
