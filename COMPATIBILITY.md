# Compatibility

This document describes exactly what Vibe 0.3.0 implements and what it does not.
It is deliberately narrower than "Forge compatibility". A `runnable-subset`
verdict means Vibe found no hard blocker and can construct the mod's entrypoint
against the API listed here; it does not mean the mod will behave like it does
in Minecraft.

Last updated: 2026-11-08.

## Verdicts

| Verdict | Criteria |
| --- | --- |
| `runnable-subset` | Loader is Java FML, at least one `@Mod` class exists, no error-severity blocker is present. |
| `content-only` | Recognised loader but no Java entrypoint to execute (including low-code FML). |
| `blocked` | At least one error-severity blocker. `vibe run` skips the jar unless `--include-blocked` is used. |
| `unknown` | No recognisable Forge metadata or entrypoint. |

Warnings (nested jars, native libraries, unparseable class files, missing Forge
metadata) are reported but do not by themselves block a run.

## Compatibility profiles

Vibe's release version is independent from Minecraft and Forge. A profile fixes
one Minecraft version, one Forge loader family, a Java level, and a runtime
backend. The embedded matrix is in `crates/vibe-core/data/profiles.toml`.

| Profile | Minecraft | Forge | Java | Status | Runtime backend |
| --- | --- | --- | --- | --- | --- |
| `forge-1.20.1-47` | 1.20.1 | 47.x | 17 | `experimental-subset` | `forge-api-subset-v1` |
| `forge-1.21.1-52` | 1.21.1 | 52.x | 21 | `experimental-subset` | `forge-api-subset-v1` |
| `forge-26.3-66` | 26.3 | 66.x | 21 | `metadata-only` | none |

- `experimental-subset` means the clean-room Forge API subset can run the tested
  fixture shape. It is not Minecraft gameplay parity and is not a claim about
  third-party mods.
- `metadata-only` means detection, planning, inspection, and content export are
  available, but `vibe run` refuses to execute Java code for that target.
- A mod's target is read from a `minecraft` dependency and `loaderVersion` in
  `mods.toml`, or from `mcmod.info` (`acceptedMinecraftVersions` preferred over
  `mcversion`).
- Vibe resolves a profile only when exactly one profile matches both the
  Minecraft and Forge declarations. Missing declarations are `undeclared`;
  several matches are `ambiguous`. Neither is guessed.
- `--profile <id>` chooses explicitly but is still checked against the
  declarations. One JVM run uses one profile.

The Java runtime reports `supported_profiles` in its handshake and rejects a
profile or backend it does not provide. `forge-26.3-66` is intentionally absent
from that list.

## Implemented Forge API

Only these `net.minecraftforge.*` classes are accepted by analysis. A reference
to any other class in that package blocks the jar with
`UnsupportedForgeApi`.

```text
net.minecraftforge.common.MinecraftForge
net.minecraftforge.eventbus.api.Event
net.minecraftforge.eventbus.api.EventPriority
net.minecraftforge.eventbus.api.IEventBus
net.minecraftforge.eventbus.api.SubscribeEvent
net.minecraftforge.eventbus.api.bus.BusGroup
net.minecraftforge.eventbus.api.bus.EventBus
net.minecraftforge.eventbus.api.bus.IModBusEventMarker
net.minecraftforge.eventbus.api.listener.SubscribeEvent
net.minecraftforge.fml.ModContainer
net.minecraftforge.fml.ModList
net.minecraftforge.fml.ModLoadingContext
net.minecraftforge.fml.common.FMLCommonHandler
net.minecraftforge.fml.common.Mod
net.minecraftforge.fml.common.Mod$Bus
net.minecraftforge.fml.common.Mod$Dist
net.minecraftforge.fml.common.Mod$EventBusSubscriber
net.minecraftforge.fml.common.Mod$EventHandler
net.minecraftforge.fml.common.event.FMLConstructionEvent
net.minecraftforge.fml.common.event.FMLInitializationEvent
net.minecraftforge.fml.common.event.FMLLoadCompleteEvent
net.minecraftforge.fml.common.event.FMLPostInitializationEvent
net.minecraftforge.fml.common.event.FMLPreInitializationEvent
net.minecraftforge.fml.common.event.FMLServerStartingEvent
net.minecraftforge.fml.event.IModBusEvent
net.minecraftforge.fml.event.lifecycle.FMLClientSetupEvent
net.minecraftforge.fml.event.lifecycle.FMLCommonSetupEvent
net.minecraftforge.fml.event.lifecycle.FMLConstructModEvent
net.minecraftforge.fml.event.lifecycle.FMLDedicatedServerSetupEvent
net.minecraftforge.fml.event.lifecycle.FMLLoadCompleteEvent
net.minecraftforge.fml.event.lifecycle.InterModEnqueueEvent
net.minecraftforge.fml.event.lifecycle.InterModProcessEvent
net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext
```

The same list is published by the Java runtime in its handshake and
`runtime.info` response (`supported_forge_api`). It must be kept in sync in:

- `crates/vibe-core/src/analysis.rs`
- `java/vibe-runtime/src/main/java/dev/vibe/runtime/VibeRuntime.java`

A small logging shim (`org.apache.logging.log4j.Logger` and `LogManager`) is
also supplied so common javafml constructors link. It is not a Forge API and is
not part of the compatibility decision.

## Entrypoints and annotations

### `@Mod`

- Read at runtime by the Java sidecar and at analysis time from class files.
- `value()` is preferred; `modid()` is used when `value()` is blank.
- A blank id falls back to a jar-derived id with a warning.
- Duplicate ids: the first class wins; later classes are rejected with a
  warning.
- Constructor selection: `(FMLJavaModLoadingContext)`, then `(IEventBus)`, then
  no-argument.
- `version()` is used when non-blank, otherwise `Implementation-Version`, then
  `0.0.0`.
- Other annotation elements are accepted for linkage but not interpreted.

### `@Mod.EventHandler`

Legacy instance methods with exactly one event parameter are invoked when the
event type is assignable to that parameter. `FMLConstructionEvent`,
`FMLPreInitializationEvent`, `FMLInitializationEvent`,
`FMLPostInitializationEvent`, and legacy `FMLLoadCompleteEvent` are dispatched.
Modern `FMLConstructModEvent`, `FMLCommonSetupEvent`, sided setup, and inter-mod
events are dispatched through the event bus instead.

### `@Mod.EventBusSubscriber`

- Static subscriber classes are registered on the Forge bus (`Bus.FORGE`, the
  default) or the owning mod's bus (`Bus.MOD`).
- An explicit `modid()` selects the mod bus owner; otherwise the first mod in
  the same jar is used. Classes without a matching mod are skipped.
- `Dist[] value()` filters registration: empty means both sides; `CLIENT` and
  `DEDICATED_SERVER` restrict it.
- Classes also carrying `@Mod` are not registered as separate subscribers.

### `@SubscribeEvent`

Both `net.minecraftforge.eventbus.api.SubscribeEvent` and
`net.minecraftforge.eventbus.api.listener.SubscribeEvent` are recognised.
Listeners may be instance or static methods with one event parameter.
`priority()` is read. Dispatch order is deterministic; failures are recorded
per listener and included in the lifecycle report.

### Event buses

- A shared Forge bus (`MinecraftForge.EVENT_BUS`) and one bus per mod.
- `IEventBus.register`, `unregister`, `post`, and typed `addListener` are
  implemented for the supported event types.
- Records include event name, listener label, success, and failure text.

## Lifecycle

`vibe run` executes this order:

```text
construct
  constructor
construct_events
  FMLConstructModEvent (mod bus)
  FMLConstructionEvent (legacy handlers)
common_setup
  FMLCommonSetupEvent (mod bus)
  FMLPreInitializationEvent (legacy handlers)
init
  FMLInitializationEvent (legacy handlers)
sided_setup
  FMLClientSetupEvent       when --side CLIENT
  FMLDedicatedServerSetupEvent otherwise
post_init
  FMLPostInitializationEvent (legacy handlers)
inter_mod
  InterModEnqueueEvent
  InterModProcessEvent
complete
  net.minecraftforge.fml.event.lifecycle.FMLLoadCompleteEvent
  net.minecraftforge.fml.common.event.FMLLoadCompleteEvent (legacy handlers)
```

This is not Forge's full mod-loading pipeline. There is no Minecraft game,
registry freeze, resource reload, server start, network channel setup, or
data-generation phase. `FMLServerStartingEvent` exists for linkage but is not
posted.

## Load planning

`vibe plan` reads metadata only. It reports duplicate mod ids, dependency
ordering, missing dependencies, declared version ranges, version mismatches,
namespace conflicts, cycles, each mod's compatibility profile, and plan-level
profile conflicts. It never downloads, resolves, or executes a mod. A plan has
one profile only when every planned mod agrees; otherwise the conflicts are
listed. An explicit `--profile` that is not satisfied makes `vibe plan` exit
`5`.

`--side CLIENT|SERVER` makes the plan side-aware:

- A mod declared `clientSideOnly` or with `side = "CLIENT"` is excluded from a
  server plan, and the reverse for a server-only mod. Excluded mods are listed
  with a reason rather than silently dropped.
- A dependency declared `side = "CLIENT"` is not reported missing on a server
  plan. A missing dependency keeps its declared side and version range in the
  report.
- A mod or dependency with no declared side is treated as applying to both
  sides. Vibe does not guess.

A dependency that is present but outside its declared range is reported as a
version mismatch, with the raw range and the found version. The range parser is
a pragmatic Maven subset, not a full Maven resolver; an unparseable range is
not reported as a mismatch.

## Blockers

| Blocker | Severity | Meaning |
| --- | --- | --- |
| `MinecraftClasses` | error | References `net.minecraft.*` classes Horizon does not provide. |
| `UnsupportedForgeApi` | error | References `net.minecraftforge.*` outside the list above. |
| `MixinTransformer` | error | Declares Mixin configurations. |
| `AccessTransformer` | error | Declares access transformers. |
| `Coremod` | error | Declares an FML core plugin. |
| `ExternalClasses` | error | References classes from a library the jar does not bundle, and Vibe does not provide. |
| `MissingEntrypoint` | error | Java loader but no `@Mod` class. |
| `UnsupportedLoader` | error | Unknown language loader. |
| `JavaVersionTooNew` | error | Class file release is newer than the selected runtime. |
| `JavaVersionTooOld` | error | Selected runtime is older than Java 17. |
| `NestedJars` | warning | Jar-in-jar entries are reported but not added to the class loader. |
| `NativeLibraries` | warning | Native entries are reported but not extracted or loaded. |
| `ClassLoadError` | warning | Some class files could not be parsed during analysis. |
| `NoForgeMetadata` | warning | No `mods.toml` / `mcmod.info` found. |

For large jars, blockers keep at most 32 evidence entries as a sample, plus
`evidence_total`, and group every entry by Java package (or path prefix) in
`evidence_groups`. Human output prints the largest groups; `--json` has the
full grouped counts. A jar that references hundreds of classes therefore shows
categories such as `net.minecraft.world.level (214)` instead of a wall of class
names. Group examples and the sample are explicitly labelled when truncated.

## Content export compatibility

Export carries only files under claimed namespaces:

- `assets/<namespace>/...`
- `data/<namespace>/...`
- optionally `data/minecraft/tags/...`
- optionally `data/c/tags/...`
- optionally `assets/minecraft/atlases/...`

It does not carry:

- Java classes or `META-INF`;
- Mixin configs, access transformers, or coremods;
- nested jars or native libraries;
- content that exists only as code-side registration;
- pack-format upgrades or data fixes.

Unclaimed mod namespaces are excluded and reported. Horizon's documented limits
are enforced: 65,536 files, 16 MiB per file, 512 MiB total.

The `--json` report explains each included file (path, kind, byte size) and up
to 512 skipped files with a reason (`unclaimed_namespace`, `unsafe_path`, or
`not_content`). Counts stay complete when the per-file skip list is truncated.
When the jar has a root `pack.mcmeta`, its declared `pack_format` and
`supported_formats` are reported; they are metadata, not carried into the
bundle, and Vibe does not upgrade pack formats. The same is true of a declared
Minecraft dependency range: it is reported, not turned into a Horizon version
check.

## Tested behavior

The automated suite compiles and runs two fixtures against the real shim:

| Fixture | Coverage |
| --- | --- |
| `hello-forge` | Modern `(FMLJavaModLoadingContext)` constructor, `ModLoadingContext`, typed `addListener`, reflective `@SubscribeEvent` on the mod bus, custom `PingEvent`, legacy pre-init handler, log4j shim, resources under `hello_forge`. |
| `legacy-hello` | `@Mod` with legacy lifecycle handlers: construction, pre-init, init, post-init, load-complete; `ModList` cross-check. |

Observed integration result with both fixtures and `--side SERVER`:

```text
profile: forge-1.21.1-52 (Minecraft 1.21.1, Forge 52.1.20)
loaded 2 mod entrypoint(s)
lifecycle: phase=complete, ok=true
event records: 9/9 succeeded
```

Both fixtures declare Minecraft `[1.21.1,1.22)` and Forge `[52,53)`, so they
resolve to `forge-1.21.1-52`. The fixture resources also export to a Horizon
`.hmod` whose manifest is validated (`api = "0.2"`,
`capabilities = ["registries"]`).

### Blocked-case fixtures

`crates/vibe-cli/tests/blocked_fixtures.rs` inspects nine compiled jars and
asserts the exact blocker each one produces:

| Fixture | Asserted result |
| --- | --- |
| `blocked-minecraft-ref` | `blocked`, exactly one error: `MinecraftClasses` |
| `blocked-unsupported-forge` | `blocked`, `UnsupportedForgeApi` |
| `blocked-external-lib` | `blocked`, `ExternalClasses` (`com.google.gson.Gson`) |
| `blocked-mixin` | `blocked`, `MixinTransformer` |
| `blocked-access-transformer` | `blocked`, `AccessTransformer` |
| `blocked-coremod` | `blocked`, `Coremod` |
| `blocked-no-entrypoint` | `blocked`, `MissingEntrypoint` |
| `blocked-unsupported-loader` | `blocked`, `UnsupportedLoader` |
| `blocked-nested-jar` | `runnable-subset` with one `NestedJars` warning |

The fixtures are compiled by `java/test-mods/blocked/build.ps1` (or `build.sh`).
They test Vibe's own inspection, not third-party mods.

These fixtures test Vibe's own subset. They do **not** demonstrate
compatibility with any third-party Forge mod. No specific third-party mod is
claimed as supported until a test for it is added and recorded here.

## Known gaps and honest limits

- Most real Forge mods reference Minecraft classes and are therefore blocked.
  That is the expected result, not a regression.
- Mixin, coremods, access transformers, and nested jars are not executed.
- The runtime has no Minecraft registries, `DeferredRegister`, capabilities,
  networking, commands, world access, or client rendering.
- Third-party library references are analysed from the jar's constant pools and
  compared with the classes the jar ships, the JDK/platform modules, Vibe's
  Forge and log4j shims, and compile-time-only annotation packages. A reference
  that remains is reported as `ExternalClasses`, with the classes and their
  library groups. This is static evidence, not a runtime proof: a reference in
  dead or reflective code can be reported even when it is never reached.
- No sandbox: mod code runs with the host's OS permissions.
- Pack formats are not upgraded during export.
- The Java runtime is compiled with `--release 17`; newer class files are
  blocked when the selected runtime is older than their release.
- Only the lifecycle events listed above are posted.

## Reporting compatibility

When reporting a result, include:

1. the exact jar (name, version, SHA-256 if possible);
2. `vibe profiles` and the resolved profile id;
3. `vibe inspect --json <jar>` output;
4. `vibe run --json <jar>` output when the verdict is `runnable-subset`;
5. the Java version selected by `vibe doctor`;
6. whether the claim is about running the lifecycle or only about export.

Do not describe a successful fixture run as evidence that general Forge mods run
on Horizon.
