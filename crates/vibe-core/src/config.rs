use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, VibeError};
use crate::metadata::ModSide;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Client,
    #[default]
    Server,
    Both,
}

impl Side {
    pub fn as_forge_side(self) -> &'static str {
        match self {
            Side::Client => "CLIENT",
            Side::Server => "SERVER",
            Side::Both => "BOTH",
        }
    }

    pub fn from_mod_side(side: ModSide) -> Self {
        match side {
            ModSide::Client => Side::Client,
            ModSide::Server => Side::Server,
            ModSide::Both | ModSide::Unknown => Side::Both,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VibeConfig {
    pub java: Option<PathBuf>,
    pub mods: ModsConfig,
    pub runtime: RuntimeConfig,
    pub bridge: BridgeConfig,
    pub horizon: HorizonConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModsConfig {
    pub directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub jar: Option<PathBuf>,
    pub side: Side,
    pub max_heap_mb: u32,
    pub java_args: Vec<String>,
    pub start_timeout_ms: u64,
    pub request_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            jar: None,
            side: Side::Server,
            max_heap_mb: 2048,
            java_args: Vec::new(),
            start_timeout_ms: 15_000,
            request_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeConfig {
    pub log_events: bool,
    pub protocol: u32,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            log_events: false,
            protocol: 1,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HorizonConfig {
    pub ctl: Option<PathBuf>,
    pub game_dir: Option<PathBuf>,
    pub launcher: Option<PathBuf>,
}

impl VibeConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut config: VibeConfig = toml::from_str(text)
            .map_err(|e| VibeError::Config(format!("could not parse config: {e}")))?;
        config.apply_env();
        Ok(config)
    }

    pub fn load_or_default(path: Option<&Path>) -> Result<Self> {
        let mut config = match path {
            Some(path) if path.exists() => Self::load(path)?,
            _ => Self::default(),
        };
        config.apply_env();
        Ok(config)
    }

    pub fn apply_env(&mut self) {
        if let Ok(java) = std::env::var("VIBE_JAVA") {
            self.java = Some(PathBuf::from(java));
        }
        if let Ok(runtime) = std::env::var("VIBE_RUNTIME_JAR") {
            self.runtime.jar = Some(PathBuf::from(runtime));
        }
        if let Ok(ctl) = std::env::var("VIBE_HORIZON_CTL").or_else(|_| std::env::var("HORIZON_CTL"))
        {
            self.horizon.ctl = Some(PathBuf::from(ctl));
        }
        if let Ok(game_dir) =
            std::env::var("VIBE_HORIZON_HOME").or_else(|_| std::env::var("HORIZON_HOME"))
        {
            self.horizon.game_dir = Some(PathBuf::from(game_dir));
        }
        if let Ok(launcher) = std::env::var("VIBE_HORIZON_LAUNCHER") {
            self.horizon.launcher = Some(PathBuf::from(launcher));
        }
    }

    pub fn mod_directories(&self) -> &[PathBuf] {
        &self.mods.directories
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_config() {
        let config = VibeConfig::parse(
            r#"
            [mods]
            directories = ["mods", "extra-mods"]

            [runtime]
            side = "client"
            max_heap_mb = 4096

            [horizon]
            game_dir = "C:/Games/Horizon"
            "#,
        )
        .unwrap();
        assert_eq!(config.mods.directories.len(), 2);
        assert_eq!(config.runtime.side, Side::Client);
        assert_eq!(config.runtime.max_heap_mb, 4096);
        assert_eq!(config.runtime.start_timeout_ms, 15_000);
    }
}
