//! The Rust side of Vibe's process bridge.
//!
//! The bridge is a line-delimited JSON protocol over the Java runtime's stdin
//! and stdout. It is deliberately simple: no ports, no authentication tokens,
//! no generated bindings, and no hidden shared memory. Every message is a JSON
//! object with a `type` field.

pub mod client;
pub mod codec;
pub mod java;
pub mod protocol;

pub use client::{
    default_runtime_path, java_command_line, RuntimeClient, RuntimeEvent, RuntimeOptions,
};
pub use codec::{read_message, write_message};
pub use java::{bundled_java_root, detect_all_java, locate_java, JavaInstallation};
pub use protocol::{BridgeError, Hello, Message, PROTOCOL_VERSION};
