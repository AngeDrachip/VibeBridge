use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use vibe_core::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub jars: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub missing_paths: Vec<PathBuf>,
}

/// Find `.jar` files under the supplied files/directories.
///
/// Directories are searched recursively. Hidden directories (`.git`, `.vibe`)
/// and common build output are skipped to avoid accidentally loading unrelated
/// jars. The returned list is deterministic.
pub fn discover_jars(paths: &[PathBuf]) -> Result<DiscoveryReport> {
    let mut jars = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut missing_paths = BTreeSet::new();

    for path in paths {
        if !path.exists() {
            missing_paths.insert(path.clone());
            continue;
        }
        if path.is_file() {
            if is_jar(path) {
                jars.insert(path.clone());
            } else {
                skipped.insert(path.clone());
            }
            continue;
        }
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                if entry.depth() == 0 {
                    return true;
                }
                !(entry.file_type().is_dir()
                    && matches!(
                        name.as_ref(),
                        ".git" | ".vibe" | "target" | "build" | ".gradle" | ".idea"
                    ))
            })
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_file() && is_jar(entry.path()) {
                jars.insert(entry.into_path());
            }
        }
    }

    Ok(DiscoveryReport {
        jars: jars.into_iter().collect(),
        skipped: skipped.into_iter().collect(),
        missing_paths: missing_paths.into_iter().collect(),
    })
}

fn is_jar(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_recursively_and_reports_missing() {
        let temp = std::env::temp_dir().join(format!("vibe-discovery-{}", std::process::id()));
        let nested = temp.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(temp.join("a.jar"), b"not really").unwrap();
        std::fs::write(temp.join("notes.txt"), b"no").unwrap();
        std::fs::write(nested.join("b.JAR"), b"not really").unwrap();
        let report =
            discover_jars(&[temp.clone(), temp.join("missing"), temp.join("notes.txt")]).unwrap();
        assert_eq!(report.jars.len(), 2);
        assert_eq!(report.missing_paths.len(), 1);
        assert_eq!(report.skipped.len(), 1);
        let _ = std::fs::remove_dir_all(temp);
    }
}
