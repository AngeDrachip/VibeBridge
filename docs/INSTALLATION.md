# Installation guide

This guide covers Windows and Linux, from a clean machine to a first Vibe run.
Vibe 0.4.0 is a development milestone. Read
[COMPATIBILITY.md](COMPATIBILITY.md) before expecting a specific third-party mod
to work.

## 1. Install Java 17 or newer

Vibe's Java sidecar is compiled with `--release 17`. Java 8 cannot run it.

### Windows

1. Download a JDK 17 or newer (Eclipse Adoptium Temurin, Microsoft Build of
   OpenJDK, or another distribution).
2. Install it, then set `JAVA_HOME` for your shell:

```powershell
$env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot"
& "$env:JAVA_HOME\bin\java.exe" -version
```

To make it permanent, add `JAVA_HOME` in **System Properties → Environment
Variables** and put `%JAVA_HOME%\bin` on `PATH`.

> Vibe rejects a `javac` older than 17 even if a newer `java` is also installed.
> If `vibe doctor` reports the wrong Java, pass `--java` explicitly.

### Linux

```bash
sudo apt install openjdk-17-jdk      # Debian/Ubuntu
# or: sudo dnf install java-17-openjdk-devel
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
"$JAVA_HOME/bin/java" -version
```

Vibe searches, in order: `--java`, `VIBE_JAVA`, `JAVA_HOME`, `PATH`, and common
JDK install folders. It prefers the JDK's own `java` over launcher shims.

## 2. Install Vibe

### From source

```powershell
# Windows
git clone https://github.com/AngeDrachip/VibeBridge.git
cd VibeBridge
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
cargo build --release
target\release\vibe.exe doctor
```

```bash
# Linux
git clone https://github.com/AngeDrachip/VibeBridge.git
cd VibeBridge
bash java/build.sh
cargo build --release
./target/release/vibe doctor
```

The Java build produces `java/build/vibe-runtime.jar`. Vibe finds it
automatically in a source checkout, or you can set `VIBE_RUNTIME_JAR` or pass
`--runtime`.

### First setup

```powershell
vibe setup
```

This creates:

```text
mods/            enabled mod jars
mods-disabled/   jars you disabled
logs/            run and crash logs
```

`vibe setup --game-dir <horizon-game-folder>` also checks Horizon and creates
`<game-folder>/horizon-mods/` if needed.

## 3. Install Horizon (optional)

Vibe can inspect, plan, run, and export without Horizon. Horizon integration is
only needed to probe a running game or install content bundles.

1. Install Horizon using its own instructions.
2. Make sure `horizon-ctl` is on `PATH`, or set `VIBE_HORIZON_CTL` to its full
   path.
3. Run:

```powershell
vibe horizon probe
vibe setup --game-dir "C:\path\to\horizon\game"
```

The game folder is the one that holds `options.txt`. Horizon reads content mods
from `<game-folder>/horizon-mods/`. Vibe installs files there but does not edit
`horizon-mods.txt`; enabling a content mod is a Horizon setting.

> Horizon has no JVM and cannot execute Java or Forge gameplay code. Vibe's
> `horizon export` carries assets and data only.

## 4. Add mods

Put original Forge `.jar` files into `mods/`. Do not rename them unless you also
update your own records; Vibe uses the file name only for diagnostics.

Scan them:

```powershell
vibe mods
vibe mods --side SERVER
vibe mods --profile forge-1.21.1-52
```

The table shows:

- whether the jar is enabled or disabled;
- its mod id and version;
- the detected compatibility profile;
- the analysis verdict;
- missing dependencies and version mismatches.

Inspect one jar in detail:

```powershell
vibe inspect mods\example.jar
vibe --json inspect mods\example.jar
```

Enable and disable:

```powershell
vibe mods --disable example.jar
vibe mods --enable example.jar
```

## 5. Understand dependency reports

Vibe reads `mods.toml`, `neoforge.mods.toml`, and legacy `mcmod.info`
dependencies. It reports:

- **missing** mandatory and optional dependencies;
- **version mismatches** when a dependency is present but outside the declared
  range;
- **side-specific** dependencies (`BOTH`, `CLIENT`, `SERVER`);
- duplicate mod ids, namespace conflicts, and dependency cycles.

Vibe never downloads dependencies. Install the missing jar yourself, then rerun
`vibe mods` or `vibe plan`.

## 6. First launch

```powershell
vibe run --profile forge-1.21.1-52 mods
```

Vibe starts the Java runtime, loads only `runnable-subset` jars, constructs
`@Mod` entrypoints, posts the lifecycle events, materializes registries, and
writes a log.

A successful run ends with:

```text
lifecycle: phase=complete, ok=true
```

A failed run exits non-zero and writes `logs/vibe-crash-<session>.log`.

See [FIRST_LAUNCH.md](FIRST_LAUNCH.md) for a walkthrough.

## 7. Export and install content

```powershell
vibe horizon export mods\example.jar --output dist\example.hmod
vibe horizon validate dist\example.hmod
vibe horizon install dist\example.hmod --game-dir "C:\path\to\horizon\game" --dry-run
vibe horizon install dist\example.hmod --game-dir "C:\path\to\horizon\game"
```

Export copies assets/data unchanged, skips Java classes and `META-INF`, and
refuses to overwrite its source jar.

## 8. Read logs

```powershell
vibe logs
vibe logs --tail 80
vibe logs --show vibe-run-<session>.log
```

Each run writes:

- `logs/vibe-run-<session>.log` — human-readable summary plus the full report;
- `logs/vibe-run-<session>.json` — machine-readable report;
- `logs/vibe-crash-<session>.log` — only when the run failed.

## 9. Update Vibe

```powershell
git pull
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
cargo build --release
vibe doctor
```

Compatibility profiles and the Java API subset are part of the build. There is
no separate backend download yet.

## 10. Uninstall

Vibe does not install a service or write outside its own folders. Delete the
checkout and the `mods/`, `mods-disabled/`, and `logs/` folders you created. If
you installed content bundles, remove them from `<game-folder>/horizon-mods/`
and from Horizon's enabled-mod list.

## Next steps

- [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- [COMPATIBILITY.md](COMPATIBILITY.md)
- [FIRST_LAUNCH.md](FIRST_LAUNCH.md)
