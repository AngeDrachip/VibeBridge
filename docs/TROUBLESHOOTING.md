# Troubleshooting

Start with these commands:

```powershell
vibe doctor
vibe setup
vibe mods
vibe logs --tail 80
```

Use `--json` on any command when you want to attach machine-readable output to a
bug report.

## "Java runtime unavailable" / "javac not found"

Vibe needs a **JDK 17 or newer**.

- Check the detected Java:

```powershell
vibe doctor
```

- Pass a specific install:

```powershell
vibe doctor --java "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot"
vibe run --java "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot" mods
```

- A Java 8 `java.exe` or launcher shim earlier on `PATH` does not help. Set
  `JAVA_HOME` to the JDK 17+ folder or pass `--java`.
- On Linux, install a JDK, not only a JRE.

## "no Vibe runtime found"

The Java sidecar is missing.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
```

```bash
bash java/build.sh
```

Then check:

```powershell
vibe doctor --runtime java\build\vibe-runtime.jar
```

You can also set `VIBE_RUNTIME_JAR` or pass `--runtime`.

## "unknown compatibility profile"

List the profiles in this build:

```powershell
vibe profiles
```

Profile ids are stable, but a new build may add or rename targets. Use an id
from the list. Do not assume a Minecraft version is supported because it exists
in Minecraft; only listed profiles are known to this build.

## "profile is metadata-only with no runtime backend"

Vibe can inspect, plan, and export that target but cannot execute its Java code.
This is expected for:

- `forge-1.12.2-14`
- `forge-1.16.5-36`
- `forge-1.18.2-40`
- `forge-1.19.2-43`
- `forge-1.20.4-49`
- `forge-26.3-66`

Runtime backends exist only for `forge-1.20.1-47` and `forge-1.21.1-52`, and
those are experimental subsets, not Minecraft gameplay.

## "the selected mods target more than one compatibility profile"

One JVM run uses one profile. Either:

- split the mods into separate runs with `--profile`; or
- remove the mods that target another Minecraft/Forge version.

Vibe refuses to mix profiles instead of guessing.

## A mod is "blocked"

Run:

```powershell
vibe inspect mods\example.jar
```

Common blockers:

| Blocker | Meaning | What you can do |
| --- | --- | --- |
| Unsupported Minecraft classes | The mod uses `net.minecraft.*` classes outside Vibe's subset. | Nothing yet; this is a real missing API. |
| Unsupported Forge API | The mod uses Forge classes outside the clean-room subset. | Nothing yet; check `docs/COMPATIBILITY.md`. |
| External classes | A bundled or expected library is missing. | Install the dependency or use a jar that bundles it. |
| Mixin / coremod / access transformer | Vibe does not transform bytecode. | Not supported. |
| Missing entrypoint | No `@Mod` class was found. | Check the jar is a Forge mod, not a resource pack. |
| Unsupported loader | Kotlin/other language loaders are not implemented. | Not supported yet. |

## A mod is "content-only"

There is no Java entrypoint Vibe can run. You may still export assets/data:

```powershell
vibe horizon export mods\example.jar --output dist\example.hmod
vibe horizon validate dist\example.hmod
```

Content-only does **not** mean the mod's gameplay works.

## Missing dependency or version mismatch

`vibe mods` and `vibe plan` list missing dependencies and version mismatches.
Vibe does not download anything. Install the required jar, then rerun the
command. Check the side: a `CLIENT`-only dependency is not required for
`--side SERVER`.

## A nested JAR dependency is not found

Vibe extracts `META-INF/jarjar/*.jar` and `META-INF/jars/*.jar` into a temporary
directory and adds them to the mod class loader. If a class is still missing:

- confirm the nested archive is inside one of those two paths;
- run `vibe inspect <jar>` and check the nested-jar warning and external-class
  report;
- nested dependency **version ranges** are not resolved by Vibe.

## Horizon is not found

- Put `horizon-ctl` on `PATH`, or set `VIBE_HORIZON_CTL`:

```powershell
$env:VIBE_HORIZON_CTL = "C:\path\to\horizon-ctl.exe"
vibe horizon probe
```

- Export and install work without a running game; only `probe` needs the game.
- Horizon has no JVM. A Java mod's gameplay code cannot run inside Horizon.

## Horizon rejects an exported bundle

```powershell
vibe horizon validate dist\example.hmod
```

Vibe checks the manifest, id/side/API, claimed namespaces, paths, size limits,
and first-level block/item declarations. Horizon's own
`cargo horizon-mod check` remains authoritative.

If a namespace is missing, export with `--namespace <ns>` only when the original
mod really owns that namespace. Never claim a namespace you do not own.

## The run failed

1. Read the crash report:

```powershell
vibe logs --tail 120
```

2. Check the `failures` list in the JSON report.
3. Re-run with a smaller set of mods.
4. Run `vibe inspect` on each jar.
5. If the failure is in Vibe itself, include the session id, the log file, the
   Vibe version (`vibe --version`), the Java version, and the mod jar name in a
   GitHub issue. Do not upload mod jars you are not allowed to share.

## Performance

Measure before claiming anything:

```powershell
vibe bench --iterations 5 --profile forge-1.21.1-52 mods
```

`bench` reports startup, load, lifecycle, and total time per run, plus
min/median/max. Results depend on the machine, Java version, heap size, and the
selected mods. Vibe does not ship a performance claim without these numbers.

## Still stuck

Open an issue at
<https://github.com/AngeDrachip/VibeBridge/issues> with:

- `vibe --version`
- `vibe doctor --json`
- `vibe mods --json`
- the relevant `logs/vibe-run-*.log` or `logs/vibe-crash-*.log`
- the exact command you ran

Never attach a mod jar unless its license allows redistribution.
