use std::path::PathBuf;

/// Errors produced by the core layer.
#[derive(Debug, thiserror::Error)]
pub enum VibeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid archive: {0}")]
    Archive(String),

    #[error("invalid metadata: {0}")]
    Metadata(String),

    #[error("invalid class file: {0}")]
    ClassFile(String),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("invalid mod id {0:?}: expected ^[a-z][a-z0-9_]{{1,63}}$")]
    InvalidModId(String),

    #[error("invalid resource location {0:?}: expected namespace:path")]
    InvalidResourceLocation(String),

    #[error("bridge protocol error: {0}")]
    Protocol(String),

    #[error("Java runtime unavailable: {0}")]
    JavaUnavailable(String),

    #[error("runtime failed: {0}")]
    Runtime(String),

    #[error("Horizon integration error: {0}")]
    Horizon(String),

    #[error("test fixture error: {0}")]
    Fixture(String),

    #[error("path {0} does not exist")]
    MissingPath(PathBuf),
}

pub type Result<T> = std::result::Result<T, VibeError>;
