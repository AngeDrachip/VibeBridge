# Installing Vibe

Vibe 1.0.0-beta.1 is the **first beta**, not a finished runtime. It can inspect
original Forge jars, run a small clean-room Forge API subset in a real JVM, and
export assets/data as a Horizon content mod. It does not make Horizon execute
Java gameplay code, and most real Forge mods are still blocked or untested.

If you only want the binary, download
`vibe-1.0.0-beta.1-windows-x86_64.zip` from
[the v1.0.0-beta.1 release](https://github.com/AngeDrachip/VibeBridge/releases/tag/v1.0.0-beta.1),
unpack it, and run `vibe doctor`. The steps below build from source.

If you want the long version, start with
[docs/INSTALLATION.md](docs/INSTALLATION.md). This file is the short path.

## What you need

- Windows 10/11 or a modern Linux distribution.
- A **JDK 17 or newer**. Vibe compiles and runs its Java sidecar with
  `--release 17`. Java 8 is not enough.
- Rust 1.85+ only if you build Vibe from source.
- Optional: a Horizon installation with `horizon-ctl` if you want to probe the
  game and install content bundles.

## Windows quick start (from source)

```powershell
git clone https://github.com/AngeDrachip/VibeBridge.git
cd VibeBridge

# Point JAVA_HOME at a JDK 17+.
$env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.20.101-hotspot"

# Build the Java sidecar and the test fixtures.
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File java\test-mods\build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File java\test-mods\blocked\build.ps1

# Build the CLI.
cargo build --release

# Create mods/, mods-disabled/, and logs/, and check your setup.
target\release\vibe.exe setup
```

## Linux quick start (from source)

```bash
git clone https://github.com/AngeDrachip/VibeBridge.git
cd VibeBridge

export JAVA_HOME=/path/to/jdk-17
bash java/build.sh
bash java/test-mods/build.sh
bash java/test-mods/blocked/build.sh
cargo build --release

./target/release/vibe setup
```

## Install your first mod

1. Put original Forge `.jar` files into `mods/` (or pass a folder to the
   commands).
2. Run `vibe mods` to see the detected Minecraft/Forge versions, profile,
   verdict, and missing dependencies.
3. Run `vibe inspect <jar>` for the exact blockers.
4. Run `vibe run` only when the mod is `runnable-subset` and resolves to a
   runtime-capable profile (`forge-1.20.1-47` or `forge-1.21.1-52`).
5. For content-only mods, export assets/data with
   `vibe horizon export <jar> --output <file>.hmod` and install the bundle with
   `vibe horizon install`.

## Check a live Horizon instance

```powershell
vibe horizon probe      # find horizon-ctl
vibe horizon connect    # verify the running game, exit 0 when ready
```

`connect` checks the control socket, the player in the world, pending chunks,
the Minecraft version, and the Horizon content API. Turn on
Options > Horizon Settings > AI Agent in the game, or start it with
`horizon-ctl launch --world NAME --wait`.

## Enable and disable mods

```powershell
vibe mods --disable my-mod.jar
vibe mods --enable my-mod.jar
```

Disabled jars are moved to `mods-disabled/`; they are not deleted.

## Read the logs

Every `vibe run` writes `logs/vibe-run-<session>.log` and
`logs/vibe-run-<session>.json`. A failed run also writes
`logs/vibe-crash-<session>.log`.

```powershell
vibe logs --tail 80
```

## Updating Vibe

Vibe's release version is independent from Minecraft, Forge, and Horizon
versions. To update a source checkout:

```powershell
git pull
powershell -NoProfile -ExecutionPolicy Bypass -File java\build.ps1
cargo build --release
vibe doctor
```

Compatibility profiles are embedded in the binary. Rebuilding Vibe is how you
update the profile matrix and the Java API subset; there is no separate
"compatibility backend" download yet.

## Important limits

- Vibe does **not** run Minecraft. It runs a clean-room subset of the Forge API
  in a separate JVM.
- Horizon has no JVM. Vibe cannot make Horizon execute Java gameplay code.
- Only a small `net.minecraft.*` subset is implemented. Unsupported references
  still block a mod.
- Mixin, coremods, access transformers, capabilities, networking, entities,
  block entities, menus, recipes, and world access are not implemented.
- Real third-party mods have **not** been validated in this build environment.
  The compatibility database marks them `UNKNOWN`.
- Vibe 1.0 Beta has **not** been released because its release requirements have
  not been met yet.

See [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) and
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).
