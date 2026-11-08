use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bump when a change breaks the Java runtime or Rust client.
pub const PROTOCOL_VERSION: u32 = 1;

pub mod method {
    pub const PING: &str = "runtime.ping";
    pub const INFO: &str = "runtime.info";
    pub const LOAD_MODS: &str = "mods.load";
    pub const START_LIFECYCLE: &str = "lifecycle.start";
    pub const MOD_STATUS: &str = "mods.status";
    pub const REGISTRY_SNAPSHOT: &str = "registry.snapshot";
    pub const SHUTDOWN: &str = "runtime.shutdown";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    Welcome {
        protocol: u32,
        session: String,
    },
    Request {
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Response {
        id: u64,
        ok: bool,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        error: Option<BridgeError>,
    },
    Event {
        event: String,
        #[serde(default)]
        data: Value,
    },
    Goodbye {
        #[serde(default)]
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub protocol: u32,
    pub runtime: String,
    pub java_version: String,
    #[serde(default)]
    pub java_vendor: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub pid: u64,
    #[serde(default)]
    pub supported_forge_api: Vec<String>,
    /// Compatibility profile ids this runtime can execute.
    #[serde(default)]
    pub supported_profiles: Vec<String>,
    /// The profile this process was started with, if any.
    #[serde(default)]
    pub active_profile: Option<String>,
    /// The API backend serving `active_profile`, if any.
    #[serde(default)]
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<String>,
}

impl Message {
    pub fn request(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self::Request {
            id,
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_through_json() {
        let message = Message::request(
            7,
            method::LOAD_MODS,
            serde_json::json!({"paths": ["a.jar"]}),
        );
        let text = serde_json::to_string(&message).unwrap();
        assert!(text.contains("\"type\":\"request\""));
        let parsed: Message = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, message);
    }

    #[test]
    fn response_round_trips_through_json() {
        let message = Message::Response {
            id: 1,
            ok: true,
            result: serde_json::json!({"constructed": 1}),
            error: None,
        };
        let text = serde_json::to_string(&message).unwrap();
        let parsed: Message = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, message);
    }
}
