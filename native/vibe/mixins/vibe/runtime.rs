// Vibe's sidecar: starts the Vibe CLI in a real JVM beside Horizon, from the
// jars a player drops in `<game>/vibe/mods/`, and answers `/vibe`.
//
// This file is both the mod's library (its tests) and the module the bundle
// adds to the game as `crate::game::vibe`. It never calls into Horizon: the
// Java code runs in its own process, exactly as `vibe run` does from a
// terminal. Horizon's world does not receive Forge gameplay from it.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The mod's id (its settings folder), name and version.
pub const ID: &str = "vibe";
pub const NAME: &str = "Vibe";
pub const VERSION: &str = "1.0.0-beta.1";
pub const DESCRIPTION: &str =
    "Runs Java Forge mods in Vibe's JVM sidecar. Drop jars in <game>/vibe/mods; /vibe status, /vibe mods, /vibe reload. Java gameplay stays outside Horizon's world.";

/// One `vibe run` process and where it wrote.
struct Run {
    child: Child,
    pid: u32,
    log: PathBuf,
    mods: PathBuf,
    exe: PathBuf,
    started: Instant,
    exit: Option<ExitStatus>,
}

static RUN: Mutex<Option<Run>> = Mutex::new(None);

/// The folder the release zip is unpacked into: `<game>/vibe/`.
pub fn home(game_dir: &Path) -> PathBuf {
    game_dir.join("vibe")
}

/// The folder players drop Forge jars into: `<game>/vibe/mods/`.
pub fn mods_dir(game_dir: &Path) -> PathBuf {
    home(game_dir).join("mods")
}

/// Where the sidecar and export logs go: `<game>/vibe/logs/`.
pub fn logs_dir(game_dir: &Path) -> PathBuf {
    home(game_dir).join("logs")
}

/// The optimized JRE unpacked with the release: `<game>/vibe/java/runtime/`.
/// When it is there, the sidecar uses it instead of whatever Java the machine
/// has, so a run does not depend on the player's setup.
pub fn bundled_java(game_dir: &Path) -> Option<PathBuf> {
    let runtime = home(game_dir).join("java").join("runtime");
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    runtime.join("bin").join(name).is_file().then_some(runtime)
}

/// `vibe.exe` (or `vibe`) from `VIBE_EXE`, the release folder, the game's own
/// folder, the folder holding `horizon-client.exe`, or `PATH`.
pub fn find_exe(game_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "vibe.exe" } else { "vibe" };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(value) = std::env::var_os("VIBE_EXE").filter(|value| !value.is_empty()) {
        candidates.push(PathBuf::from(value));
    }
    let home = home(game_dir);
    candidates.push(home.join(name));
    candidates.push(home.join("bin").join(name));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            candidates.push(dir.join("..").join(name));
            candidates.push(dir.join("..").join("..").join("vibe").join(name));
            candidates.push(dir.join("..").join("release").join(name));
        }
    }
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The jar files in `mods`, sorted by name.
pub fn jars(mods: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = fs::read_dir(mods) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn ensure_folders(game_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mods = mods_dir(game_dir);
    let logs = logs_dir(game_dir);
    fs::create_dir_all(&mods)
        .and_then(|()| fs::create_dir_all(&logs))
        .map_err(|error| format!("cannot create {} ({error})", mods.display()))?;
    let placeholder = mods.join("PUT-FORGE-JARS-HERE.txt");
    if !placeholder.exists() {
        let text = "\
Vibe runs Java Forge mods from this folder.

1. Put Forge mod jars (*.jar) in this folder.
2. Launch Horizon and open a world. Vibe starts its JVM sidecar automatically.
3. In chat: /vibe status, /vibe mods, /vibe reload, /vibe stop, /vibe help.

Vibe itself must be unpacked one level up, so this folder holds:
  ..\\vibe.exe
  ..\\java\\build\\vibe-runtime.jar

Download: https://github.com/AngeDrachip/VibeBridge/releases/latest
";
        let _ = fs::write(placeholder, text);
    }
    Ok((mods, logs))
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn elapsed(started: Instant) -> String {
    let secs = started.elapsed().as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

fn truncate(line: &str, max: usize) -> String {
    if line.chars().count() <= max {
        return line.to_owned();
    }
    let mut short: String = line.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}

/// The useful lines of a sidecar log: the runtime, profile, loaded mods and
/// lifecycle, without the registry JSON or the paths.
fn summary(path: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let wanted = [
        "Vibe runtime:",
        "  profile:",
        "  loaded ",
        "  lifecycle:",
        "  event records:",
        "  bridge events:",
    ];
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| wanted.iter().any(|prefix| line.starts_with(prefix)))
        .map(|line| truncate(line, 120))
        .collect();
    if lines.is_empty() {
        let all: Vec<&str> = text.lines().collect();
        let from = all.len().saturating_sub(3);
        lines.extend(all[from..].iter().map(|line| truncate(line, 120)));
    }
    lines
}

fn spawn(
    exe: &Path,
    home: &Path,
    args: &[&std::ffi::OsStr],
    log: &Path,
    java: Option<&Path>,
) -> std::io::Result<Child> {
    let out = File::create(log)?;
    let err = out.try_clone()?;
    let mut command = Command::new(exe);
    command
        .args(args)
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    if let Some(java) = java {
        command.env("VIBE_JAVA", java);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the game is a GUI process; the console tool must
        // not flash a terminal window.
        command.creation_flags(0x0800_0000);
    }
    command.spawn()
}

/// Start `vibe run <mods>` unless a sidecar is already running.
pub fn start(game_dir: &Path) -> String {
    let (mods, logs) = match ensure_folders(game_dir) {
        Ok(folders) => folders,
        Err(error) => return format!("Vibe: {error}"),
    };
    let mut guard = RUN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(run) = guard.as_mut() {
        if run.exit.is_none() {
            if let Ok(Some(status)) = run.child.try_wait() {
                run.exit = Some(status);
            } else {
                return format!(
                    "Vibe is already running (pid {}, {}). Log: {}",
                    run.pid,
                    elapsed(run.started),
                    run.log.display()
                );
            }
        }
    }
    let Some(exe) = find_exe(game_dir) else {
        return format!(
            "Vibe: vibe.exe not found. Unpack the Vibe release in {} (vibe.exe and java\\build\\vibe-runtime.jar).",
            home(game_dir).display()
        );
    };
    let log = logs.join(format!("sidecar-{}.log", timestamp()));
    let java = bundled_java(game_dir);
    let args = [
        std::ffi::OsStr::new("run"),
        mods.as_os_str(),
        std::ffi::OsStr::new("--side"),
        std::ffi::OsStr::new("SERVER"),
        std::ffi::OsStr::new("--timeout-ms"),
        std::ffi::OsStr::new("120000"),
    ];
    match spawn(&exe, &home(game_dir), &args, &log, java.as_deref()) {
        Ok(child) => {
            let pid = child.id();
            let count = jars(&mods).len();
            *guard = Some(Run {
                child,
                pid,
                log: log.clone(),
                mods,
                exe,
                started: Instant::now(),
                exit: None,
            });
            format!(
                "Vibe started (pid {pid}) on {count} jar(s). Log: {}",
                log.display()
            )
        }
        Err(error) => format!("Vibe: cannot start {} ({error})", exe.display()),
    }
}

/// Stop the sidecar if it is running.
pub fn stop() -> String {
    let mut guard = RUN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(run) = guard.as_mut() else {
        return "Vibe is not running.".to_owned();
    };
    if let Some(status) = run.exit {
        return format!("Vibe is not running (last {status}).");
    }
    match run.child.kill() {
        Ok(()) => {
            run.exit = run.child.wait().ok();
            format!("Vibe stopped (pid {}).", run.pid)
        }
        Err(error) => format!("Vibe: cannot stop pid {} ({error}).", run.pid),
    }
}

/// Stop the sidecar, then start it again.
pub fn reload(game_dir: &Path) -> String {
    let stopped = stop();
    let started = start(game_dir);
    format!("{stopped} {started}")
}

/// `/vibe status`: what runs, where the jars are, and the end of the log.
pub fn status(game_dir: &Path) -> Vec<String> {
    let mods = mods_dir(game_dir);
    let mut lines = vec![format!(
        "{NAME} {VERSION}: Java Forge mods run in a sidecar JVM, not inside Horizon's world."
    )];
    let mut guard = RUN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(run) = guard.as_mut() {
        if run.exit.is_none() {
            if let Ok(Some(status)) = run.child.try_wait() {
                run.exit = Some(status);
            }
        }
        match run.exit {
            None => lines.push(format!(
                "Sidecar: running (pid {}, {})",
                run.pid,
                elapsed(run.started)
            )),
            Some(status) => lines.push(format!(
                "Sidecar: finished ({status}) after {}",
                elapsed(run.started)
            )),
        }
        lines.push(format!(
            "Mods: {} ({} jar(s))",
            run.mods.display(),
            jars(&run.mods).len()
        ));
        lines.push(format!("Exe: {}", run.exe.display()));
        lines.push(java_line(game_dir));
        lines.push(format!("Log: {}", run.log.display()));
        let summary = summary(&run.log);
        if summary.is_empty() {
            lines.push("  (the log is empty for now)".to_owned());
        } else {
            for line in summary {
                lines.push(format!("  {line}"));
            }
        }
    } else {
        lines.push("Sidecar: not started.".to_owned());
        lines.push(format!(
            "Mods: {} ({} jar(s))",
            mods.display(),
            jars(&mods).len()
        ));
        match find_exe(game_dir) {
            Some(exe) => lines.push(format!("Exe: {}", exe.display())),
            None => lines.push(format!(
                "Exe: not found; unpack the Vibe release in {}",
                home(game_dir).display()
            )),
        }
        lines.push(java_line(game_dir));
        lines.push("Use /vibe start, or open a world to start it automatically.".to_owned());
    }
    lines
}

/// Which JVM the sidecar will use: the optimized JRE shipped with the release,
/// or whatever Java the machine provides.
fn java_line(game_dir: &Path) -> String {
    match bundled_java(game_dir) {
        Some(root) => format!("Java: {} (bundled, optimized)", root.display()),
        None => "Java: system Java (no bundled runtime in vibe/java/runtime)".to_owned(),
    }
}

/// `/vibe mods`: the jars Vibe will read.
pub fn mod_lines(game_dir: &Path) -> Vec<String> {
    let dir = mods_dir(game_dir);
    let found = jars(&dir);
    if found.is_empty() {
        return vec![format!(
            "No Forge jars in {}. Drop *.jar files there, then /vibe reload.",
            dir.display()
        )];
    }
    let mut lines = vec![format!("{} jar(s) in {}:", found.len(), dir.display())];
    for jar in found {
        lines.push(format!(
            "  {}",
            jar.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        ));
    }
    lines
}

/// `/vibe help`: the commands and the one hard limit.
pub fn help() -> Vec<String> {
    vec![
        "/vibe status - sidecar state, jars, exe and the end of the log".to_owned(),
        "/vibe mods - list the jars in the Vibe mods folder".to_owned(),
        "/vibe start - start the JVM sidecar now".to_owned(),
        "/vibe stop - stop the JVM sidecar".to_owned(),
        "/vibe reload - stop it, then read the jars again".to_owned(),
        "/vibe export - export each jar's assets/data as a Horizon .hmod (background)".to_owned(),
        "Java code runs in Vibe's JVM beside Horizon; Horizon's world does not run Forge gameplay."
            .to_owned(),
    ]
}

/// `/vibe export`: run `vibe horizon export` for every jar in the background.
pub fn export(game_dir: &Path) -> String {
    let (mods, logs) = match ensure_folders(game_dir) {
        Ok(folders) => folders,
        Err(error) => return format!("Vibe: {error}"),
    };
    let Some(exe) = find_exe(game_dir) else {
        return "Vibe: vibe.exe not found; unpack the Vibe release in <game>/vibe/.".to_owned();
    };
    let found = jars(&mods);
    if found.is_empty() {
        return format!("No Forge jars in {}.", mods.display());
    }
    let count = found.len();
    let log = logs.join(format!("export-{}.log", timestamp()));
    let out_dir = game_dir.join("horizon-mods");
    let _ = fs::create_dir_all(&out_dir);
    let home = home(game_dir);
    let message = format!(
        "Vibe export started for {count} jar(s) into {}. Log: {}",
        out_dir.display(),
        log.display()
    );
    let log_for_thread = log.clone();
    let java = bundled_java(game_dir);
    std::thread::spawn(move || {
        let mut text = String::new();
        for jar in found {
            let stem = jar
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("vibe-export");
            let out = out_dir.join(format!("{stem}.hmod"));
            let args = [
                std::ffi::OsStr::new("horizon"),
                std::ffi::OsStr::new("export"),
                jar.as_os_str(),
                std::ffi::OsStr::new("--output"),
                out.as_os_str(),
                std::ffi::OsStr::new("--force"),
            ];
            let mut command = Command::new(&exe);
            command
                .args(args)
                .current_dir(&home)
                .stdin(Stdio::null());
            if let Some(java) = &java {
                command.env("VIBE_JAVA", java);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            match command.output() {
                Ok(output) => {
                    text.push_str(&format!(
                        "== {} -> {} ({})\n{}\n{}\n",
                        jar.display(),
                        out.display(),
                        output.status,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                Err(error) => {
                    text.push_str(&format!("== {} failed: {error}\n", jar.display()));
                }
            }
        }
        let _ = fs::write(&log_for_thread, text);
    });
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_jars_are_listed() {
        let root = std::env::temp_dir().join(format!("vibe-mod-test-{}", timestamp()));
        let _ = fs::create_dir_all(&root);
        fs::write(root.join("a.jar"), b"a").unwrap();
        fs::write(root.join("b.JAR"), b"b").unwrap();
        fs::write(root.join("notes.txt"), b"c").unwrap();
        let found = jars(&root);
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("a.jar"));
        assert!(found[1].ends_with("b.JAR"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn help_names_the_hard_limit() {
        let text = help().join("\n");
        assert!(text.contains("Horizon's world does not run Forge gameplay"));
    }

    #[test]
    fn paths_live_under_the_game_folder() {
        let game = Path::new("C:/horizon");
        assert_eq!(mods_dir(game), Path::new("C:/horizon/vibe/mods"));
        assert_eq!(logs_dir(game), Path::new("C:/horizon/vibe/logs"));
    }

    #[test]
    fn a_bundled_runtime_is_used_only_when_it_has_java() {
        let game = std::env::temp_dir().join(format!("vibe-java-test-{}", timestamp()));
        let runtime = home(&game).join("java").join("runtime");
        let java = runtime
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        let _ = fs::remove_dir_all(&game);
        assert_eq!(bundled_java(&game), None);
        fs::create_dir_all(java.parent().unwrap()).unwrap();
        assert_eq!(bundled_java(&game), None);
        fs::write(&java, b"").unwrap();
        assert_eq!(bundled_java(&game), Some(runtime));
        assert!(java_line(&game).contains("bundled, optimized"));
        let _ = fs::remove_dir_all(&game);
    }
}
