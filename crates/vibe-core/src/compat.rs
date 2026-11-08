//! The compatibility database.
//!
//! The database records what Vibe has actually tested, with an explicit status
//! for every entry. It is embedded so a released binary reports the same
//! evidence it was built with. Entries are only promoted above `UNKNOWN` when
//! there is a reproducible test or a manual verification note.

use serde::{Deserialize, Serialize};

use crate::error::{Result, VibeError};

/// The compatibility database compiled into this build.
pub const EMBEDDED_COMPAT: &str = include_str!("../data/compat.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatStatus {
    /// Every tested feature works.
    Full,
    /// Most tested features work.
    High,
    /// Some tested features work.
    Partial,
    /// Only compatible data/assets are available.
    ContentOnly,
    /// Cannot currently run.
    Blocked,
    /// Not sufficiently tested.
    Unknown,
}

impl CompatStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::High => "HIGH",
            Self::Partial => "PARTIAL",
            Self::ContentOnly => "CONTENT_ONLY",
            Self::Blocked => "BLOCKED",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.trim().to_ascii_uppercase().as_str() {
            "FULL" => Self::Full,
            "HIGH" => Self::High,
            "PARTIAL" => Self::Partial,
            "CONTENT_ONLY" => Self::ContentOnly,
            "BLOCKED" => Self::Blocked,
            "UNKNOWN" => Self::Unknown,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatEntry {
    /// A stable identifier, normally the mod id.
    pub id: String,
    pub name: String,
    /// A category from the test matrix (utility, technology, ...).
    pub category: String,
    #[serde(default)]
    pub minecraft: Option<String>,
    #[serde(default)]
    pub forge: Option<String>,
    pub status: CompatStatus,
    /// What was actually run or observed. This is the evidence behind the
    /// status, not a marketing claim.
    pub evidence: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatDatabase {
    pub schema: u32,
    pub entries: Vec<CompatEntry>,
}

impl CompatDatabase {
    pub fn embedded() -> Result<Self> {
        Self::parse(EMBEDDED_COMPAT)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let database: CompatDatabase = toml::from_str(text).map_err(|e| {
            VibeError::Config(format!("could not parse compatibility database: {e}"))
        })?;
        if database.schema != 1 {
            return Err(VibeError::Config(format!(
                "unsupported compatibility database schema {}; this Vibe build understands schema 1",
                database.schema
            )));
        }
        Ok(database)
    }

    pub fn filter_status(&self, status: CompatStatus) -> Vec<&CompatEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.status == status)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_database_parses_and_has_evidence_for_every_entry() {
        let database = CompatDatabase::embedded().unwrap();
        assert!(!database.entries.is_empty());
        for entry in &database.entries {
            assert!(!entry.id.is_empty());
            assert!(
                !entry.evidence.trim().is_empty(),
                "{} has no evidence",
                entry.id
            );
        }
    }

    #[test]
    fn fixture_entries_are_promoted_but_real_mods_are_not() {
        let database = CompatDatabase::embedded().unwrap();
        let registry = database
            .entries
            .iter()
            .find(|entry| entry.id == "registry_demo")
            .expect("registry fixture entry");
        assert_eq!(registry.status, CompatStatus::Full);

        for entry in database.entries.iter().filter(|entry| {
            !entry.id.ends_with("_demo")
                && !entry.id.ends_with("_forge")
                && !entry.id.ends_with("_hello")
        }) {
            assert_eq!(
                entry.status,
                CompatStatus::Unknown,
                "{} must stay UNKNOWN until it is tested",
                entry.id
            );
        }
    }

    #[test]
    fn statuses_parse_case_insensitively() {
        assert_eq!(CompatStatus::parse("full"), Some(CompatStatus::Full));
        assert_eq!(
            CompatStatus::parse("content_only"),
            Some(CompatStatus::ContentOnly)
        );
        assert_eq!(CompatStatus::parse("nope"), None);
    }
}
