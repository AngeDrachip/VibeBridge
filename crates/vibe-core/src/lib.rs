//! Vibe core: Forge mod metadata, class-file facts, compatibility analysis,
//! and Vibe's own configuration types.
//!
//! This crate deliberately has no process, archive, or network code. It is the
//! pure domain layer shared by the loader, the bridge, the CLI, and tests.

pub mod analysis;
pub mod classfile;
pub mod config;
pub mod error;
pub mod ids;
pub mod metadata;
pub mod version;

pub use analysis::{
    analyze, AnalysisFacts, Blocker, BlockerKind, CompatibilityReport, EvidenceGroup, Severity,
    Verdict,
};
pub use error::{Result, VibeError};
pub use ids::{ModId, ResourceLocation};
pub use metadata::{Dependency, LoaderKind, MetadataSource, ModFileMetadata, ModMetadata, ModSide};
