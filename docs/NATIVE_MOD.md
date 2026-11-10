# Vibe as a Horizon native mod (`.hnmod`)

`vibe-1.0.0-beta.1.hnmod` is Vibe packaged in Horizon's **native mod** format.
It is not a `.hmod`: a `.hmod` carries data and assets, while a `.hnmod` carries
Rust source that the Horizon launcher compiles into the game. That is the only
way to add behaviour to Horizon itself, and it is what this file does: it makes
the game start Vibe's JVM sidecar for you and adds `/vibe` commands to chat.

The hard limit does not move. Horizon still has no JVM, and Forge gameplay still
runs in Vibe's separate Java process, not in Horizon's world. The mod removes the
manual `vibe run` step and puts the controls in chat; it does not make Horizon
execute Java.

## What the mod does

| Hook | Behaviour |
| --- | --- |
| `server_about_to_start` | Starts `vibe run <instance>/vibe/mods --side SERVER` in the background when a world opens. |
| `server_stopping` | Stops the sidecar so no JVM outlives the world. |
| `command` | Answers `/vibe ...` in chat before the server or Horizon sees it. |
| `commands` | Adds `vibe` to chat completion. |
| `mods` | Adds a **Vibe 1.0.0-beta.1** row under Options → Mods. |

Chat commands:

```text
/vibe status    sidecar state, jar count, exe and log paths, log summary
/vibe mods      list the jars in <instance>/vibe/mods/
/vibe start     start the sidecar now (no world needed)
/vibe stop      stop the sidecar
/vibe reload    stop, then read the jars again
/vibe export    export every jar's assets/data as a .hmod in <instance>/horizon-mods/
/vibe help      the commands and the hard limit
```

The mod never calls into Horizon's world. It only spawns a process and prints
its result in chat.

## What it needs on disk

The `.hnmod` is the wiring; the Vibe release is the runtime. Unpack the release
zip **inside the instance folder** (the folder holding `options.txt`, `saves/`
and `horizon-mods/`, usually `%APPDATA%\horizon`), so that:

```text
<instance>/
  vibe/
    vibe.exe                      from vibe-1.0.0-beta.1-windows-x86_64.zip
    java/build/vibe-runtime.jar   from the same zip
    java/runtime/                 the optimized JRE from the same zip
    mods/                         put your Forge jars here
    logs/                         created on first start
  horizon-mods/                   .hmod content exported by /vibe export
```

`VIBE_EXE` overrides the executable path if you keep Vibe elsewhere. The mod
also looks beside `horizon-client.exe` and on `PATH`.

When `<instance>/vibe/java/runtime/` exists, the mod sets `VIBE_JAVA` to it, so
the sidecar uses the bundled JRE and no system Java is needed. Remove that folder
and the sidecar falls back to `VIBE_JAVA`, then `JAVA_HOME`, then `PATH`.

A `.hnmod` is source-only by design (Horizon compiles it into the game), so it
cannot embed a JRE. The release zip carries `java/runtime/` beside the mod's
files; that is why the Vibe folder has to be unpacked in the instance.

## Install

1. Install a `native-*` Horizon build (the launcher's Mods page does this when
   you install a native mod) and turn native mods on.
2. On the Mods page choose **Add from GitHub**, paste
   `https://github.com/AngeDrachip/VibeBridge`, accept the native-code warning,
   and let the launcher build. Or copy `vibe-1.0.0-beta.1.hnmod` into the
   instance's `horizon-native-mods/` folder (the Mods page's **Open folder**
   shows the exact path) and press **Build**.
3. Unpack the Vibe release into the instance folder as shown above.
4. Start a world. The sidecar starts automatically; type `/vibe status`.

The first build downloads Rust and compiles Horizon, so it takes a while. Later
builds reuse the cache.

## How it was verified

- `horizon-mixins apply` on Horizon `2ebf00a0281c592f79191e76fce0e7d522c6f425`
  with horizon-mixins `d9c83d7096b7ea5b83678da2efdfd6161f74997e` (0.6.0):
  5 injections, 1 module, status `ok`.
- `cargo build --locked -p horizon-client --bin horizon-client --profile fast`
  succeeds, and the built game was driven with `horizon-ctl` on prepared 26.3
  resources.
- Opening `VibeProof` started the sidecar from the release layout **with no
  system Java used**: the mod found `vibe/java/runtime/`, set `VIBE_JAVA`, and the
  run log reports `java.home` = `<instance>/vibe/java/runtime`; the sidecar
  reports `Vibe runtime: vibe-runtime/1.0.0-beta.1 (Java 27)`, profile
  `forge-1.21.1-52`, one loaded entrypoint (`hello_forge`) and
  `lifecycle: phase=complete, ok=true`, 4/4 event records.
- `/vibe status` reported the finished sidecar (exit 0), the jar count, the exe,
  `Java: <instance>\vibe\java\runtime (bundled, optimized)`, and the log lines.
- `/vibe mods`, `/vibe help`, `/vibe reload` and `/vibe export` worked;
  `/vibe export` wrote `horizon-mods/hello-forge.hmod` (730 bytes).
- The packaged `.hnmod` is accepted by Horizon's own reader,
  `horizon_mods::index::native::Bundle::read`: id `vibe`, version
  `1.0.0-beta.1`, hooks 44, nine entries.

The launcher-side install itself was not exercised on this machine (the
launcher is not installed here), so the steps above mirror Horizon's documented
`native-build` flow rather than a click-through test.

## Uninstall

Remove the mod on the Mods page and rebuild. The `<instance>/vibe/` folder is
yours: delete it to remove the sidecar, its logs and the jars.
