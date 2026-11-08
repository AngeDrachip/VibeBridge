# Compatibility

Vibe 0.4.0 implements a **small, tested, clean-room subset** of the Forge and
Minecraft Java APIs. It is not a general Forge runtime. This page states exactly
what exists, what is tested, and what remains blocked.

The machine-readable source of truth is:

- `crates/vibe-core/data/profiles.toml` — Minecraft/Forge targets;
- `crates/vibe-core/data/compat.toml` — compatibility database;
- `crates/vibe-core/src/analysis.rs` — the exact supported class lists.

Run `vibe compat` and `vibe profiles` to see what your binary contains.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| `FULL` | Every tested feature works. |
| `HIGH` | Most tested features work. |
| `PARTIAL` | Some tested features work. |
| `CONTENT_ONLY` | Only compatible data/assets are available. |
| `BLOCKED` | Cannot currently run. |
| `UNKNOWN` | Not sufficiently tested. |

A status is only promoted above `UNKNOWN` when there is a reproducible test or a
recorded manual verification. Loading a JAR successfully is not enough.

## Minecraft/Forge targets

| Profile | Minecraft | Forge | Java | Runtime backend |
| --- | --- | --- | --- | --- |
| `forge-1.12.2-14` | 1.12.2 | 14.x | 8 | none (metadata-only) |
| `forge-1.16.5-36` | 1.16.5 | 36.x | 8 | none (metadata-only) |
| `forge-1.18.2-40` | 1.18.2 | 40.x | 17 | none (metadata-only) |
| `forge-1.19.2-43` | 1.19.2 | 43.x | 17 | none (metadata-only) |
| `forge-1.20.1-47` | 1.20.1 | 47.x | 17 | `forge-api-subset-v1` |
| `forge-1.20.4-49` | 1.20.4 | 49.x | 17 | none (metadata-only) |
| `forge-1.21.1-52` | 1.21.1 | 52.x | 21 | `forge-api-subset-v1` |
| `forge-26.3-66` | 26.3 | 66.x | 21 | none (metadata-only) |

`metadata-only` means Vibe can detect, plan, inspect, and export assets/data but
cannot execute the mod's Java code. `experimental-subset` means a tested
clean-room API subset exists; it does **not** mean Minecraft gameplay works.

One JVM run uses one profile. Mixed targets are reported, not silently mixed.

## Forge API coverage

The following `net.minecraftforge.*` classes are implemented by the clean-room
shim and are accepted by analysis:

### Event bus and lifecycle

- `net.minecraftforge.eventbus.api.Event`
- `net.minecraftforge.eventbus.api.EventPriority`
- `net.minecraftforge.eventbus.api.IEventBus`
- `net.minecraftforge.eventbus.api.SubscribeEvent`
- `net.minecraftforge.eventbus.api.bus.BusGroup`
- `net.minecraftforge.eventbus.api.bus.EventBus`
- `net.minecraftforge.eventbus.api.bus.IModBusEventMarker`
- `net.minecraftforge.eventbus.api.listener.SubscribeEvent`
- `net.minecraftforge.fml.common.event.FMLConstructionEvent`
- `net.minecraftforge.fml.common.event.FMLInitializationEvent`
- `net.minecraftforge.fml.common.event.FMLPostInitializationEvent`
- `net.minecraftforge.fml.common.event.FMLPreInitializationEvent`
- `net.minecraftforge.fml.common.event.FMLLoadCompleteEvent`
- `net.minecraftforge.fml.common.event.FMLServerStartingEvent`
- `net.minecraftforge.fml.event.IModBusEvent`
- `net.minecraftforge.fml.event.lifecycle.FMLClientSetupEvent`
- `net.minecraftforge.fml.event.lifecycle.FMLCommonSetupEvent`
- `net.minecraftforge.fml.event.lifecycle.FMLConstructModEvent`
- `net.minecraftforge.fml.event.lifecycle.FMLDedicatedServerSetupEvent`
- `net.minecraftforge.fml.event.lifecycle.FMLLoadCompleteEvent`
- `net.minecraftforge.fml.event.lifecycle.InterModEnqueueEvent`
- `net.minecraftforge.fml.event.lifecycle.InterModProcessEvent`

### Mod identity and context

- `net.minecraftforge.common.MinecraftForge`
- `net.minecraftforge.fml.ModContainer`
- `net.minecraftforge.fml.ModList`
- `net.minecraftforge.fml.ModLoadingContext`
- `net.minecraftforge.fml.common.FMLCommonHandler`
- `net.minecraftforge.fml.common.Mod` and its `Bus`, `Dist`,
  `EventBusSubscriber`, and `EventHandler` members
- `net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext`

### Registries

These are **functional**, not placeholders. Entries are stored in ordered
in-memory maps and reported by `vibe run` / `vibe inspect --json`.

- `net.minecraftforge.registries.DeferredRegister`
- `net.minecraftforge.registries.ForgeRegistries`
- `net.minecraftforge.registries.IForgeRegistry`
- `net.minecraftforge.registries.RegisterEvent`
- `net.minecraftforge.registries.RegisterEvent$RegisterHelper`
- `net.minecraftforge.registries.RegistryObject`
- `net.minecraftforge.registries.VibeRegistry`
- `net.minecraftforge.registries.VibeRegistryManager`

Implemented registries: `minecraft:item`, `minecraft:block`,
`minecraft:creative_mode_tab`.

## Minecraft API coverage

The following `net.minecraft.*` classes are implemented and accepted by
analysis. Anything else still blocks a mod.

- `net.minecraft.core.Registry`
- `net.minecraft.core.registries.Registries`
- `net.minecraft.network.chat.Component`
- `net.minecraft.resources.ResourceKey`
- `net.minecraft.resources.ResourceLocation`
- `net.minecraft.world.item.CreativeModeTab`
- `net.minecraft.world.item.CreativeModeTab$Builder`
- `net.minecraft.world.item.Item`
- `net.minecraft.world.item.Item$Properties`
- `net.minecraft.world.item.ItemStack`
- `net.minecraft.world.level.block.Block`
- `net.minecraft.world.level.block.Block$Properties`
- `net.minecraft.world.level.block.state.BlockBehaviour`
- `net.minecraft.world.level.block.state.BlockBehaviour$Properties`

These types hold real data: resource locations validate and compare, item and
block properties are preserved, stacks hold counts, creative tabs keep their
title/icon, and registries can be read back. Vibe does not render or tick them
because there is no Minecraft world.

## What is not implemented

- Mixin bytecode transformation.
- Coremods and access transformers.
- Native library extraction/loading.
- Capabilities.
- Networking.
- Entities and block entities.
- Menus and inventories.
- Recipes, tags, and data generation beyond content export.
- World access, player interaction, and rendering.
- Resource reloading.
- Full Forge registry set (biomes, dimensions, sounds, particles, and so on).
- Nested dependency version-range resolution (nested JARs are loaded, but their
  Maven ranges are not solved).
- Minecraft pack-format conversion.

A mod that references any of these is reported as blocked with evidence.

## Nested JARs

Vibe extracts `META-INF/jarjar/*.jar` and `META-INF/jars/*.jar` into a temporary
directory and adds the archives to the owning mod's class loader. Classes from
nested archives count as provided during analysis, so bundled libraries are not
misreported as external. The temporary directory is removed after the runtime
session.

This is verified by the `nested_demo` fixture, which references a class that
exists only inside `META-INF/jarjar/nested-lib.jar`.

## Tested fixtures

These are clean-room fixtures built by this repository. They are not
third-party mods.

| Fixture | Status | Evidence |
| --- | --- | --- |
| `hello_forge` | `FULL` | Modern constructor, typed and reflective listeners, lifecycle, event records. |
| `legacy_hello` | `FULL` | Legacy `@Mod.EventHandler` lifecycle and construction event. |
| `registry_demo` | `FULL` | `DeferredRegister` items, blocks, and creative tabs materialize in real registries. |
| `nested_demo` | `FULL` | A class from `META-INF/jarjar/nested-lib.jar` loads and runs. |

Blocked-case fixtures cover unsupported Minecraft classes, unsupported Forge
API, external libraries, Mixin, access transformers, coremods, missing
entrypoints, and unsupported loaders.

## Third-party mods

No third-party Forge jars were available in the build environment. The
compatibility database lists representative mods across the required categories
as `UNKNOWN`:

- `jei` (inventory)
- `embeddium` (performance)
- `journeymap` (maps)
- `create` (technology)
- `botania` (magic)
- `twilightforest` (world generation)
- `appleskin` (utility)
- `chipped` (decoration)
- `geckolib` (library)
- `spark` (server)

`UNKNOWN` means **not tested**, not "works". Do not treat the presence of a name
as a compatibility claim.

## In-game functionality

No in-game functionality is demonstrated in this environment. Vibe runs mod code
in a JVM and materializes registry data, but Horizon has no JVM and there is no
running Horizon installation to observe blocks, items, entities, or player
interactions in a world.

This is a hard limitation of the current milestone, not a documentation gap.

## Why Vibe 1.0 Beta is not released

The 1.0 Beta release requirements include real third-party mods passing
functional tests, at least one mod demonstrating in-game functionality, and an
installation guide tested from a clean install with a running Horizon. Those
were not possible here. Vibe 0.4.0 is the development milestone that adds the
registry subset, nested-JAR loading, multi-version profiles, setup/logging/bench
tooling, and tests. It does not claim 1.0 Beta status.
