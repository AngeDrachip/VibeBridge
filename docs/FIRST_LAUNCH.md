# First launch

This walkthrough takes you from a built Vibe checkout to a first Java-runtime
run and a first content export. It uses the repository's clean-room fixtures so
you can verify the toolchain before trying a real mod.

## 1. Check the environment

```powershell
vibe doctor
```

You want:

- a Java 17+ executable;
- the Vibe runtime jar;
- optionally, `horizon-ctl`.

If Java or the runtime is missing, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## 2. Create the working folders

```powershell
vibe setup
```

Expected layout:

```text
mods/
mods-disabled/
logs/
```

## 3. Build and scan the fixtures

If you built the repository, the fixture jars are in
`java/test-mods/build/`. Copy one or more into `mods/`:

```powershell
Copy-Item java\test-mods\build\hello-forge.jar mods\
Copy-Item java\test-mods\build\registry-demo.jar mods\
Copy-Item java\test-mods\build\nested-demo.jar mods\
```

Scan:

```powershell
vibe mods
```

You should see `runnable-subset` and the profile `forge-1.21.1-52`.

## 4. Inspect one fixture

```powershell
vibe inspect mods\registry-demo.jar
```

`vibe inspect` does not execute code. It reports the detected metadata,
entrypoints, referenced classes, profile resolution, and blockers.

## 5. Run the Java runtime

```powershell
vibe run --profile forge-1.21.1-52 mods
```

Expected output includes:

```text
Vibe runtime: vibe-runtime/0.4.0 (...)
  profile: forge-1.21.1-52 (Minecraft 1.21.1, Forge 52.1.20, backend forge-api-subset-v1)
  loaded 3 mod entrypoint(s):
  lifecycle: phase=complete, ok=true
  log:   logs\vibe-run-<session>.log
```

What just happened:

1. Vibe inspected each jar and selected the `forge-1.21.1-52` profile.
2. It started the Java sidecar in a separate JVM.
3. The sidecar created an isolated class loader per jar.
4. It constructed each `@Mod` entrypoint.
5. It posted construction, register, common setup, sided setup, inter-mod, and
   load-complete events.
6. `registry_demo` materialized an item, a block, and a creative tab in Vibe's
   in-memory registries.
7. `nested_demo` loaded a class from its bundled `META-INF/jarjar/nested-lib.jar`.
8. Vibe wrote a log and shut the JVM down.

This proves the Java/Rust integration and the API subset. It does **not** prove
Minecraft gameplay.

## 6. Read the log

```powershell
vibe logs --tail 80
```

The JSON file next to the text log contains the full report, including event
records, registry contents, failures, and timings.

## 7. Measure a run

```powershell
vibe bench --iterations 3 --profile forge-1.21.1-52 mods
```

Example shape (your numbers will differ):

```text
run 1: startup 103.6 ms, load 33.1 ms, lifecycle 11.5 ms, total 148.3 ms, ok=true
startup    min 103.4 ms, median 103.5 ms, max 103.6 ms
```

`bench` is a measurement tool, not a performance claim.

## 8. Export content

The `hello-forge` fixture contains assets and data:

```powershell
vibe horizon export mods\hello-forge.jar --output dist\hello-forge.hmod
vibe horizon validate dist\hello-forge.hmod
```

The bundle contains `horizon-mod.toml`, `assets/hello_forge/...`, and
`data/hello_forge/...`. Java code is not carried.

## 9. Try a real mod

1. Put an original Forge jar into `mods/`.
2. Run `vibe mods`.
3. If it is `blocked`, run `vibe inspect` and read the blocker.
4. If it is `content-only`, export its assets/data.
5. If it is `runnable-subset` and resolves to a runtime profile, run it with
   `vibe run --profile <id> mods`.

Real third-party mods have not been validated in this build environment. Expect
`UNKNOWN` in the compatibility database until evidence exists.

## 10. What to report

If something fails, collect:

- `vibe --version`
- `vibe doctor --json`
- `vibe mods --json`
- `logs/vibe-run-*.log` or `logs/vibe-crash-*.log`
- the exact command you ran

Open an issue at <https://github.com/AngeDrachip/VibeBridge/issues>.

## What "success" means here

A successful first launch means:

- Vibe found Java and its runtime;
- it detected the mod's Minecraft/Forge target;
- the mod passed analysis;
- the JVM constructed the entrypoint and completed the lifecycle;
- logs were written.

It does not mean the mod is playable in Horizon. Horizon has no JVM, and Vibe
does not implement Minecraft gameplay. See
[COMPATIBILITY.md](COMPATIBILITY.md).
