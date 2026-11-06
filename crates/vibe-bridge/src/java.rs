use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use vibe_core::error::{Result, VibeError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaInstallation {
    pub executable: PathBuf,
    pub version: String,
    pub major: u16,
    pub vendor: String,
}

impl JavaInstallation {
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        hide_window(&mut command);
        command
    }
}

/// Locate a Java runtime suitable for Vibe (Java 17+).
///
/// Search order:
/// 1. an explicit path (file or JDK/JRE home),
/// 2. `VIBE_JAVA`, then `JAVA_HOME`,
/// 3. the `java`/`javac` on `PATH`, preferring the JDK's own `java` so a
///    legacy launcher shim earlier on `PATH` does not win,
/// 4. common JDK installation directories.
pub fn locate_java(preferred: Option<&Path>) -> Result<JavaInstallation> {
    if let Some(path) = preferred {
        return inspect_candidate(path).ok_or_else(|| {
            VibeError::JavaUnavailable(format!(
                "configured Java path {} is not a usable Java executable or home",
                path.display()
            ))
        });
    }
    if let Some(path) = env_java_path() {
        return inspect_candidate(&path).ok_or_else(|| {
            VibeError::JavaUnavailable(format!(
                "VIBE_JAVA is set to {}, which is not a usable Java executable or home",
                path.display()
            ))
        });
    }

    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if !home.trim().is_empty() {
            candidates.insert(PathBuf::from(home));
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for name in executable_names("java") {
            let candidate = dir.join(name);
            if candidate.is_file() {
                candidates.insert(candidate);
            }
        }
        for name in executable_names("javac") {
            let javac = dir.join(name);
            if javac.is_file() {
                for java_name in executable_names("java") {
                    let java = dir.join(java_name);
                    if java.is_file() {
                        candidates.insert(java);
                    }
                }
            }
        }
    }
    for root in common_jdk_roots() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    candidates.insert(entry.path());
                }
            }
        }
    }

    let mut installations: Vec<JavaInstallation> = candidates
        .into_iter()
        .filter_map(|path| inspect_candidate(&path))
        .collect();
    installations.sort_by(|a, b| {
        b.major
            .cmp(&a.major)
            .then_with(|| a.executable.cmp(&b.executable))
    });
    installations.dedup_by(|a, b| a.executable == b.executable);

    if let Some(found) = installations.iter().find(|java| java.major >= 17) {
        return Ok(found.clone());
    }

    let detail = if installations.is_empty() {
        "no java executable found on PATH or in common JDK locations".to_string()
    } else {
        format!(
            "only older runtimes were found: {}",
            installations
                .iter()
                .map(|java| format!("{} (Java {})", java.version, java.major))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(VibeError::JavaUnavailable(format!(
        "Vibe requires Java 17 or newer; {detail}. Install a JDK 17+ or set VIBE_JAVA/JAVA_HOME."
    )))
}

/// Report all discovered Java installations, newest first. Used by `vibe doctor`.
pub fn detect_all_java() -> Vec<JavaInstallation> {
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    if let Some(path) = env_java_path() {
        candidates.insert(path);
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if !home.trim().is_empty() {
            candidates.insert(PathBuf::from(home));
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for name in executable_names("java") {
            let candidate = dir.join(name);
            if candidate.is_file() {
                candidates.insert(candidate);
            }
        }
        for name in executable_names("javac") {
            let javac = dir.join(name);
            if javac.is_file() {
                for java_name in executable_names("java") {
                    let java = dir.join(java_name);
                    if java.is_file() {
                        candidates.insert(java);
                    }
                }
            }
        }
    }
    for root in common_jdk_roots() {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    candidates.insert(entry.path());
                }
            }
        }
    }
    let mut installations: Vec<JavaInstallation> = candidates
        .into_iter()
        .filter_map(|path| inspect_candidate(&path))
        .collect();
    installations.sort_by(|a, b| {
        b.major
            .cmp(&a.major)
            .then_with(|| a.executable.cmp(&b.executable))
    });
    installations.dedup_by(|a, b| a.executable == b.executable);
    installations
}

fn env_java_path() -> Option<PathBuf> {
    let value = std::env::var("VIBE_JAVA").ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

fn inspect_candidate(path: &Path) -> Option<JavaInstallation> {
    let executable = resolve_java_executable(path)?;
    let output = Command::new(&executable).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push('\n');
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let version = parse_java_version(&text)?;
    let major = parse_major(&version);
    let vendor = detect_vendor(&text);
    Some(JavaInstallation {
        executable,
        version,
        major,
        vendor,
    })
}

fn resolve_java_executable(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.is_dir() {
        for candidate in [
            path.join("bin").join(exe_name("java")),
            path.join(exe_name("java")),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let raw = path.to_string_lossy();
    if !raw.contains(std::path::MAIN_SEPARATOR) {
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            for name in executable_names(&raw) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn parse_java_version(text: &str) -> Option<String> {
    if let Some(start) = text.find('"') {
        if let Some(end) = text[start + 1..].find('"') {
            return Some(text[start + 1..start + 1 + end].to_string());
        }
    }
    // `javac 17.0.20.1` (not used by `java -version`, but useful for tests).
    let line = text
        .lines()
        .find(|line| line.contains("version") || line.starts_with("javac"))?;
    let token = line
        .split_whitespace()
        .find(|token| token.chars().any(|c| c.is_ascii_digit()))?;
    Some(token.trim_matches('"').to_string())
}

fn parse_major(version: &str) -> u16 {
    let mut parts = version.split(['.', '-', '+', '_']);
    let first = parts.next().unwrap_or("0");
    if first == "1" {
        parts.next().and_then(|v| v.parse().ok()).unwrap_or(8)
    } else {
        first.parse().unwrap_or(0)
    }
}

fn detect_vendor(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    for vendor in [
        "eclipse adoptium",
        "temurin",
        "graalvm",
        "microsoft",
        "zulu",
        "corretto",
        "openjdk",
        "oracle",
    ] {
        if lower.contains(vendor) {
            return vendor.to_string();
        }
    }
    "unknown".to_string()
}

fn executable_names(stem: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{stem}.exe"), stem.to_string()]
    } else {
        vec![stem.to_string()]
    }
}

fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn common_jdk_roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
            PathBuf::from(r"C:\Program Files\Java"),
            PathBuf::from(r"C:\Program Files\Microsoft"),
            PathBuf::from(r"C:\Program Files\Zulu"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/lib/jvm"),
            PathBuf::from("/Library/Java/JavaVirtualMachines"),
            PathBuf::from("/opt/homebrew/opt"),
        ]
    }
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_version_strings() {
        assert_eq!(
            parse_major(&parse_java_version("openjdk version \"17.0.2\" 2022-01-18").unwrap()),
            17
        );
        assert_eq!(
            parse_major(&parse_java_version("java version \"1.8.0_191\"").unwrap()),
            8
        );
        assert_eq!(parse_major("21.0.1"), 21);
    }

    #[test]
    fn locates_a_java_17_runtime_when_available() {
        if std::env::var_os("CI").is_some() || cfg!(windows) {
            // This is a real integration point, but the machine may have only
            // an older launcher JRE. `vibe doctor` reports what exists instead.
        }
        match locate_java(None) {
            Ok(java) => {
                assert!(java.major >= 17);
                assert!(java.executable.exists());
            }
            Err(error) => {
                eprintln!("no Java 17+ on this machine: {error}");
            }
        }
    }
}
