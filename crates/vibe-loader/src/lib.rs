//! Discovery and inspection of Forge mod archives.
//!
//! Vibe never modifies an input jar. Inspection opens archives read-only,
//! parses metadata and class files, and records blockers without executing any
//! mod code.

mod discover;
mod inspect;
mod manifest;

pub use discover::{discover_jars, DiscoveryReport};
pub use inspect::{class_facts, inspect_jar, JarInspection, JarSummary};
pub use manifest::{parse_manifest, Manifest};
