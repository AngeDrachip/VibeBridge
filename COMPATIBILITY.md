# Compatibility

This document describes exactly what Vibe 0.1.0 implements and what it does not.
It is deliberately narrower than "Forge compatibility". A `runnable-subset`
verdict means Vibe found no hard blocker and can construct the mod's entrypoint
against the API listed here; it does not mean the mod will behave like it does
in Minecraft.

Last updated: 2026-11-07.

## Verdicts

| Verdict | Criteria |
| --- | --- |
| `runnable-subset` | Loader is Java FML, at least one `@Mod` class exists, no error-severity blocker is present. |
| `content-only` | Recognised loader but no Java entrypoint to execute (including low-code FML). |
| `blocked` | At least one error-severity blocker. `vibe run` skips the jar unless `--include-blocked` is used. |
| `unknown` | No recognisable Forge metadata or entrypoint. |

Warnings (nested jars, native libraries, unparseable class files, missing Forge
metadata) are reported but do not by themselves block a run.

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

## Tested behavior

The automated suite compiles and runs two fixtures against the real shim:

| Fixture | Coverage |
| --- | --- |
| `hello-forge` | Modern `(FMLJavaModLoadingContext)` constructor, `ModLoadingContext`, typed `addListener`, reflective `@SubscribeEvent` on the mod bus, custom `PingEvent`, legacy pre-init handler, log4j shim, resources under `hello_forge`. |
| `legacy-hello` | `@Mod` with legacy lifecycle handlers: construction, pre-init, init, post-init, load-complete; `ModList` cross-check. |

Observed integration result with both fixtures and `--side SERVER`:

```text
loaded 2 mod entrypoint(s)
lifecycle: phase=complete, ok=true
event records: 9/9 succeeded
```

The fixture resources also export to a Horizon `.hmod` whose manifest is
validated (`api = "0.2"`, `capabilities = ["registries"]`).

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
2. `vibe inspect --json <jar>` output;
3. `vibe run --json <jar>` output when the verdict is `runnable-subset`;
4. the Java version selected by `vibe doctor`;
5. whether the claim is about running the lifecycle or only about export.

Do not describe a successful fixture run as evidence that general Forge mods run
on Horizon.
