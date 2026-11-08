# Vibe architecture

Vibe is a Rust host that inspects Forge mod jars, decides what it can honestly
support, launches a Java sidecar for the supported runtime subset, and exports
data-only content through Horizon's documented content-mod format.

The design is shaped by two facts:

1. Horizon is a Rust engine with no JVM, so it cannot execute Java or Forge
   mods directly.
2. Vibe must not rewrite, patch, or pretend to run mods whose behavior depends
   on Minecraft classes or bytecode transformation.

Everything below follows from those constraints.

## High-level flow

```text
 Forge jar(s)
      |
      v
 +-------------+     read-only      +----------------------+
 | vibe-loader | ----------------->  | metadata + class     |
 |             |                     | facts + SHA-256      |
 +-------------+                     +----------+-----------+
                                                |
                                                v
                                     +----------------------+
                                     | vibe-core::analysis  |
                                     | verdict + blockers   |
                                     +----------+-----------+
                                                |
                    +---------------------------+---------------------------+
                    |                                                       |
                    v                                                       v
          +-------------------+                                 +----------------------+
          | vibe-registry     |                                 | vibe-horizon         |
          | load order,       |                                 | documented .hmod     |
          | conflicts, cycles |                                 | content export       |
          +---------+---------+                                 +----------+-----------+
                    |                                                      |
                    v                                                      v
          +-------------------+                                   Horizon content mod
          | vibe-cli / host   |                                   (assets + data only)
          +---------+---------+
                    |
                    | JSON lines over stdin/stdout
                    v
          +-------------------+
          | Java sidecar      |
          | vibe-runtime      |
          | + forge API shim  |
          +-------------------+
                    |
                    v
          Mod code in isolated class loaders, real JVM
```

Inspection, planning, and export never execute mod code. Only `vibe run`
launches the JVM.

## Crates

| Crate | Responsibility |
| --- | --- |
| `vibe-core` | Domain types (`ModId`, errors), Forge metadata parsing, Maven version/range handling, class-file parsing, compatibility analysis, the embedded profile matrix, the embedded compatibility database, and reusable configuration types. |
| `vibe-loader` | Recursive jar discovery, read-only zip inspection, manifest parsing, SHA-256, nested-archive scanning, and extraction of class/resource facts. |
| `vibe-registry` | Registration of inspected mods, duplicate ids, namespace ownership, missing dependencies, cycles, and deterministic topological ordering. |
| `vibe-events` | Standalone typed host event bus and lifecycle state machine. It is tested and available to embedding hosts; the CLI's authoritative event records come from the Java runtime. |
| `vibe-bridge` | Protocol types, JSON-lines codec, Java 17+ discovery, runtime process client, handshake, request timeouts, event collection, and stderr capture. |
| `vibe-horizon` | `horizon-ctl` discovery/probe and Horizon `.hmod` content export. |
| `vibe-cli` | The `vibe` binary: `doctor`, `inspect`, `plan`, `run`, `profiles`, `horizon`, `setup`, `mods`, `logs`, `bench`, `compat`, human and `--json` output. |
| `java/vibe-forge-api` | Clean-room Forge-shaped classes and annotations used by mods, including the functional registry and Minecraft data subset. |
| `java/vibe-runtime` | Java bridge process, class loading, nested-JAR extraction, mod construction, event dispatch, registry materialization, and lifecycle execution. |

## Analysis pipeline

### Discovery

`vibe-loader::discover_jars` accepts files and directories. Directories are
walked recursively for `.jar` files. Missing paths are reported rather than
silently ignored, and non-jar files are listed as skipped.

### Inspection

`inspect_jar` opens the archive read-only and records:

- SHA-256 and size;
- `META-INF/mods.toml`, `META-INF/MANIFEST.MF`, or legacy `mcmod.info`;
- loader kind and declared mod ids, versions, sides, dependencies, authors;
- every class entry, parsed for its Java release, superclass/interfaces,
  annotations, and referenced class names;
- `net.minecraft.*` and `net.minecraftforge.*` references;
- Mixin configuration files, access-transformer files, core-plugin declarations,
  and nested jar entries (`META-INF/jarjar/` and `META-INF/jars/`);
- classes inside nested archives, recorded as provided so bundled libraries are
  not misreported as external;
- asset/data presence and namespaces;
- entries that failed to parse (reported, not fatal).

Class annotations are read from the constant pool's runtime-visible annotations.
The parser does not load classes into a JVM.

### Verdict

`vibe-core::analysis` turns facts into a `CompatibilityReport`:

- `runnable-subset`: no hard blocker and at least one `@Mod` entrypoint for a
  Java loader.
- `content-only`: a recognised loader but no Java entrypoint to run, including
  low-code FML mods.
- `blocked`: at least one error-severity blocker.
- `unknown`: not recognisably a Forge mod.

Errors block a run. Warnings are visible in reports but do not by themselves
prevent a run. The exact supported Forge API and Minecraft API lists are
constants shared by analysis and the Java runtime (see `SUPPORTED_FORGE_API` and
`SUPPORTED_MINECRAFT_API` in `crates/vibe-core/src/analysis.rs`). Only
`net.minecraft.*` classes outside the implemented subset block a mod.

## Compatibility profiles

Vibe's own release version is independent from the Minecraft, Forge, and Horizon
versions it targets. `crates/vibe-core/data/profiles.toml` is the machine-readable
matrix, embedded into `vibe-core` with `include_str!` so a released binary always
carries the profiles it was tested with.

Each profile fixes:

- an exact Minecraft version and the Maven range it covers;
- a representative Forge loader version and loader family range;
- the minimum Java release;
- a Vibe API backend identifier, when one exists;
- an honest status: `experimental-subset` or `metadata-only`.

`ProfileMatrix::resolve` matches a mod's declared Minecraft range and Forge
loader range against every profile. It returns `matched` only when exactly one
profile is compatible. `ambiguous`, `unsupported`, and `undeclared` are explicit
outcomes. An explicit `--profile` is checked against the declarations rather
than assumed; a metadata-only profile is refused by `vibe run` before a JVM is
started.

`vibe-registry` resolves every inspected mod and carries `target_profile`,
`profile_status`, the declared ranges, and failure reasons into the plan. A plan
has one `profile` only when every planned mod agrees; otherwise
`profile_conflicts` explains which mods disagree. `vibe run` enforces one
runtime-capable profile per JVM and passes the profile id and backend to the
sidecar through `-Dvibe.profile` and `-Dvibe.backend` and through `mods.load`.

This is a Proton-like selection model, not universal compatibility: it lets Vibe
grow version-specific backends without tying Vibe's release version to one
Minecraft version.

## Registry and load plan

`vibe-registry` registers mods by id and produces:

- duplicate-id reports with all jars involved;
- namespace ownership and conflicts;
- missing mandatory/optional dependencies;
- ordering cycles;
- a deterministic topological order using `before`/`after` dependency ordering.

Environment dependencies (`forge`, `minecraft`, `neoforge`, `fml`, `java`) are
not treated as mods to look up in the input set.

The registry does not implement Forge registries, `DeferredRegister`, or
Minecraft content registration. It reports the mod ids and namespaces present in
the input. The **runtime** registry subset (below) is separate: it lives in the
Java sidecar and materializes entries registered by mod code.

## Runtime registry subset

`java/vibe-forge-api` implements a small but functional registry layer:

- `VibeRegistry` — an ordered in-memory map of `ResourceLocation` to value;
- `VibeRegistryManager` — holds the item, block, and creative-tab registries,
  posts `RegisterEvent` helpers, and reports a snapshot;
- `DeferredRegister` and `RegistryObject` — lazy entry holders that resolve when
  the owning registry materializes;
- `ForgeRegistries` — accessors for the supported registries;
- `RegisterEvent` — the Forge-shaped event posted once per registry.

During the `register` phase the runtime posts `RegisterEvent` for
`minecraft:item`, `minecraft:block`, and `minecraft:creative_mode_tab`, then
calls `VibeRegistryManager.materializeAll()`. The result is returned by
`registry.snapshot` under `registries`. This is real data that mods can read
back; it is not connected to Minecraft or Horizon and cannot produce in-game
items or blocks.

## Nested JARs

The Java runtime extracts archives under `META-INF/jarjar/` and `META-INF/jars/`
to a per-session temporary directory, adds them to the owning mod's
`URLClassLoader`, and deletes the directory in `reset()`. Inspection reads the
same entries read-only and adds their class names to the provided-class set, so
a bundled library does not trigger `ExternalClasses`. Nested dependency version
ranges are not resolved.

## Java bridge

### Process model

`RuntimeClient::spawn` starts:

```text
java -Xmx<heap>m -Dfile.encoding=UTF-8 -Dvibe.session=<id> -Dvibe.side=<side> \
     -Dvibe.profile=<profile> -Dvibe.backend=<backend> \
     -jar <vibe-runtime.jar> --stdio
```

or, for a classes directory, `-cp <classes> dev.vibe.runtime.VibeRuntime
--stdio`. The Java process is a child of the Rust host. `Drop` kills and reaps
it if shutdown did not complete.

### Protocol

Protocol version 1 is UTF-8 JSON, one object per line.

| `type` | Direction | Purpose |
| --- | --- | --- |
| `hello` | Java -> Rust | Protocol version, runtime name, Java version/vendor, OS, arch, pid, `supported_forge_api`, `supported_profiles`, `active_profile`, `backend`. |
| `welcome` | Rust -> Java | Confirms protocol version and session. |
| `request` | Rust -> Java | `id`, `method`, `params`. |
| `response` | Java -> Rust | Matching `id`, `ok`, `result` or `error`. |
| `event` | Java -> Rust | Named event with data (for example `runtime.ready`). |
| `goodbye` | Both | Clean shutdown or fatal handshake mismatch. |

Methods:

| Method | Effect |
| --- | --- |
| `runtime.ping` | Liveness check. |
| `runtime.info` | Runtime version, side, active profile/backend, Java/OS details, supported API and profiles, limitations. |
| `mods.load` | Load jars under one profile/backend, inspect entries, construct `@Mod` classes, register subscribers. The runtime rejects a profile or backend it does not provide. |
| `lifecycle.start` | Run the Forge-shaped lifecycle. |
| `mods.status` | Current side, phase, mod statuses, failures. |
| `registry.snapshot` | Mod ids, namespaces, event records, and materialized registry entries. |
| `runtime.shutdown` | Reply, then stop the process. |

The real stdout is reserved for protocol frames. On startup the Java runtime
redirects `System.out` to stderr so ordinary mod logging cannot corrupt the
stream. The Rust side reads stdout on a dedicated thread, buffers stderr (last
200 lines), and uses bounded startup/request timeouts. Malformed frames and
stale responses are rejected or ignored with diagnostics.

### Java runtime behavior

Each jar gets its own `URLClassLoader` whose parent is the runtime class loader
(which already contains Vibe's Forge shim). Class initialisation is deferred
during the scan. A class carrying `@Mod` becomes a mod entrypoint; duplicate ids
are rejected with a warning.

Construction tries, in order:

1. `(FMLJavaModLoadingContext)`;
2. `(IEventBus)`;
3. `()`.

`ModLoadingContext` is set for the duration of construction and lifecycle
callbacks. A per-mod `VibeEventBus` plus a shared Forge bus provide `post`,
`register`, typed `addListener`, and reflective `@SubscribeEvent` dispatch.
`@Mod.EventBusSubscriber` classes are registered to the mod bus or Forge bus and
are filtered by `side` (empty `value()` means both sides).

Lifecycle order as implemented:

| Phase | Dispatch |
| --- | --- |
| construct | Constructor |
| construct_events | `FMLConstructModEvent`, then legacy `FMLConstructionEvent` |
| register | `RegisterEvent` per supported registry, then `VibeRegistryManager.materializeAll()` |
| common_setup | `FMLCommonSetupEvent`, then legacy `FMLPreInitializationEvent` |
| init | legacy `FMLInitializationEvent` |
| sided_setup | `FMLClientSetupEvent` for `CLIENT`, otherwise `FMLDedicatedServerSetupEvent` |
| post_init | legacy `FMLPostInitializationEvent` |
| inter_mod | `InterModEnqueueEvent`, then `InterModProcessEvent` |
| complete | lifecycle `FMLLoadCompleteEvent`, then legacy `FMLLoadCompleteEvent` |

This is a constrained subset, not Forge's full loading pipeline. In particular
`FMLServerStartingEvent` exists in the API shim but is not posted by the current
runner because Vibe has no Minecraft server to start.

Event dispatch failures are recorded per listener with the event name and
failure text, then reported. They do not abort the rest of the lifecycle.

## Horizon integration

Vibe uses only documented Horizon surfaces:

- `horizon-ctl --help` to confirm the control tool runs;
- `horizon-ctl state` to ask whether a game is reachable;
- the documented `.hmod` content-mod archive layout for export.

No undocumented IPC, injection, or game patching is used.

### Content export

`export_content`:

1. Requires a regular source jar and refuses to write over it.
2. Determines the bundle id (the declared mod id, or `--id` when several mods
   are declared).
3. Accepts extra namespaces only if they are valid and not reserved
   (`minecraft`, `c`, `horizon`).
4. Classifies zip entries:
   - `assets/<claimed>/...` and `data/<claimed>/...` are content;
   - `data/minecraft/tags/...` may be included;
   - `data/c/tags/...` convention tags may be included;
   - `assets/minecraft/atlases/...` may be included;
   - everything else is ignored, and unclaimed mod namespaces are reported.
5. Enforces Horizon limits: 65,536 files, 16 MiB per file, 512 MiB total.
6. Writes a sorted, deterministic zip to a temporary file with
   `horizon-mod.toml` first.
7. Validates the manifest (`id`, `api = "0.2"`, `capabilities` includes
   `registries`).
8. Replaces the destination atomically by rename.

The manifest contains id, name, version, optional description, authors,
`api = "0.2"`, side, `capabilities = ["registries"]`, and extra namespaces.
Resource bytes are copied unchanged. Java classes, `META-INF`, Mixins,
coremods, access transformers, and code-registered content are never included.

## Trust boundary

Vibe is not a sandbox. Mod bytecode executes inside the sidecar JVM with the
same user permissions as the `vibe` process. Analysis reduces accidental
breakage and surfaces unsupported dependencies; it is not a security boundary.
Run only jars you trust.

The Rust host treats Java output as untrusted data: protocol frames are parsed
strictly, ordinary `System.out` writes are redirected to stderr, the retained
stderr tail is bounded, and requests have deadlines. A malicious mod can still
deliberately write to the process's file descriptors or escape through JNI;
that is why this is not a security boundary.

## Failure handling

- Discovery/inspection errors are reported per path; one bad jar does not hide
  the others.
- `vibe run` refuses to start when no selected jar is `runnable-subset`
  (exit `3`) unless `--include-blocked` is used for diagnostics.
- `vibe run` refuses to start when the selected mods disagree on a runtime
  profile, when an explicit profile is not satisfied, or when the profile is
  metadata-only (exit `5`).
- Runtime failures produce exit `4` with stderr tail or bridge error details.
- Export validates the finished bundle before replacing an existing file.
- Shutdown asks the runtime to stop, waits up to five seconds, then kills it.
