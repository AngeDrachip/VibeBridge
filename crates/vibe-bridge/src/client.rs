use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use vibe_core::error::{Result, VibeError};

use crate::codec::{read_message, write_message};
use crate::java::JavaInstallation;
use crate::protocol::{BridgeError, Hello, Message, PROTOCOL_VERSION};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub java: JavaInstallation,
    pub runtime: PathBuf,
    pub max_heap_mb: u32,
    pub side: String,
    pub java_args: Vec<String>,
    pub start_timeout: Duration,
    pub request_timeout: Duration,
    pub extra_env: Vec<(String, String)>,
}

impl RuntimeOptions {
    pub fn new(java: JavaInstallation, runtime: impl Into<PathBuf>) -> Self {
        Self {
            java,
            runtime: runtime.into(),
            max_heap_mb: 2048,
            side: "SERVER".into(),
            java_args: Vec::new(),
            start_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(30),
            extra_env: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeEvent {
    pub name: String,
    pub data: Value,
}

enum Inbound {
    Message(Message),
    Malformed(String),
    Eof,
}

pub struct RuntimeClient {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Inbound>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    events: Vec<RuntimeEvent>,
    hello: Hello,
    next_id: u64,
    request_timeout: Duration,
    session: String,
}

impl RuntimeClient {
    /// Spawn the Java runtime and complete the protocol handshake.
    pub fn spawn(options: RuntimeOptions) -> Result<Self> {
        let session = format!(
            "vibe-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        let mut command = options.java.command();
        command.arg(format!("-Xmx{}m", options.max_heap_mb.max(256)));
        command.arg("-Dfile.encoding=UTF-8");
        command.arg(format!("-Dvibe.session={session}"));
        command.arg(format!("-Dvibe.side={}", options.side));
        for arg in &options.java_args {
            command.arg(arg);
        }

        if options
            .runtime
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("jar"))
            .unwrap_or(false)
        {
            command.arg("-jar").arg(&options.runtime);
        } else {
            command
                .arg("-cp")
                .arg(&options.runtime)
                .arg("dev.vibe.runtime.VibeRuntime");
        }
        command.arg("--stdio");

        for (key, value) in &options.extra_env {
            command.env(key, value);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| {
            VibeError::JavaUnavailable(format!(
                "could not start {}: {e}",
                options.java.executable.display()
            ))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| VibeError::Runtime("could not open Java stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| VibeError::Runtime("could not open Java stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| VibeError::Runtime("could not open Java stderr".into()))?;

        let (tx, rx) = channel();
        spawn_stdout_reader(stdout, tx.clone());
        let stderr_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(200)));
        spawn_stderr_reader(stderr, Arc::clone(&stderr_buffer));

        let mut client = Self {
            child,
            stdin,
            receiver: rx,
            stderr: stderr_buffer,
            events: Vec::new(),
            hello: Hello {
                protocol: PROTOCOL_VERSION,
                runtime: "unknown".into(),
                java_version: options.java.version.clone(),
                java_vendor: options.java.vendor.clone(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                pid: 0,
                supported_forge_api: Vec::new(),
            },
            next_id: 1,
            request_timeout: options.request_timeout,
            session,
        };
        client.handshake(options.start_timeout)?;
        Ok(client)
    }

    fn handshake(&mut self, timeout: Duration) -> Result<()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(VibeError::Runtime(format!(
                    "Java runtime did not send a hello within {}ms; stderr:\n{}",
                    timeout.as_millis(),
                    self.stderr_tail()
                )));
            }
            match self.receiver.recv_timeout(remaining) {
                Ok(Inbound::Message(Message::Hello(hello))) => {
                    if hello.protocol != PROTOCOL_VERSION {
                        return Err(VibeError::Protocol(format!(
                            "Java runtime speaks protocol {}, Rust client speaks {}",
                            hello.protocol, PROTOCOL_VERSION
                        )));
                    }
                    self.hello = hello;
                    let welcome = Message::Welcome {
                        protocol: PROTOCOL_VERSION,
                        session: self.session.clone(),
                    };
                    self.send(&welcome)?;
                    return Ok(());
                }
                Ok(Inbound::Message(Message::Event { event, data })) => {
                    self.events.push(RuntimeEvent { name: event, data });
                }
                Ok(Inbound::Message(Message::Goodbye { reason })) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime exited during handshake: {reason}"
                    )));
                }
                Ok(Inbound::Message(other)) => {
                    return Err(VibeError::Protocol(format!(
                        "unexpected message during handshake: {other:?}"
                    )));
                }
                Ok(Inbound::Malformed(error)) => {
                    return Err(VibeError::Protocol(error));
                }
                Ok(Inbound::Eof) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime closed stdout during handshake; stderr:\n{}",
                        self.stderr_tail()
                    )));
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime did not send a hello within {}ms; stderr:\n{}",
                        timeout.as_millis(),
                        self.stderr_tail()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime reader stopped during handshake; stderr:\n{}",
                        self.stderr_tail()
                    )));
                }
            }
        }
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&Message::request(id, method, params))?;

        let deadline = std::time::Instant::now() + self.request_timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(VibeError::Runtime(format!(
                    "request {method} timed out after {}ms; stderr:\n{}",
                    self.request_timeout.as_millis(),
                    self.stderr_tail()
                )));
            }
            match self.receiver.recv_timeout(remaining) {
                Ok(Inbound::Message(Message::Response {
                    id: response_id,
                    ok,
                    result,
                    error,
                })) => {
                    if response_id != id {
                        // A stale response cannot be matched to a caller. It is
                        // ignored rather than corrupting the current request.
                        continue;
                    }
                    if ok {
                        return Ok(result);
                    }
                    return Err(response_error(method, error));
                }
                Ok(Inbound::Message(Message::Event { event, data })) => {
                    self.events.push(RuntimeEvent { name: event, data });
                }
                Ok(Inbound::Message(Message::Goodbye { reason })) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime exited during {method}: {reason}"
                    )));
                }
                Ok(Inbound::Message(other)) => {
                    return Err(VibeError::Protocol(format!(
                        "unexpected message while waiting for {method}: {other:?}"
                    )));
                }
                Ok(Inbound::Malformed(error)) => return Err(VibeError::Protocol(error)),
                Ok(Inbound::Eof) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime closed stdout during {method}; stderr:\n{}",
                        self.stderr_tail()
                    )));
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(VibeError::Runtime(format!(
                        "request {method} timed out; stderr:\n{}",
                        self.stderr_tail()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(VibeError::Runtime(format!(
                        "Java runtime reader stopped during {method}; stderr:\n{}",
                        self.stderr_tail()
                    )));
                }
            }
        }
    }

    pub fn ping(&mut self) -> Result<Value> {
        self.request(crate::protocol::method::PING, json!({}))
    }

    pub fn runtime_info(&mut self) -> Result<Value> {
        self.request(crate::protocol::method::INFO, json!({}))
    }

    pub fn load_mods(&mut self, paths: &[PathBuf], side: &str) -> Result<Value> {
        let paths: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        self.request(
            crate::protocol::method::LOAD_MODS,
            json!({
                "paths": paths,
                "side": side,
            }),
        )
    }

    pub fn start_lifecycle(&mut self) -> Result<Value> {
        self.request(crate::protocol::method::START_LIFECYCLE, json!({}))
    }

    pub fn mod_status(&mut self) -> Result<Value> {
        self.request(crate::protocol::method::MOD_STATUS, json!({}))
    }

    pub fn registry_snapshot(&mut self) -> Result<Value> {
        self.request(crate::protocol::method::REGISTRY_SNAPSHOT, json!({}))
    }

    pub fn take_events(&mut self) -> Vec<RuntimeEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn shutdown(mut self) -> Result<()> {
        let _ = self.request(crate::protocol::method::SHUTDOWN, json!({}));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(error) => return Err(VibeError::Runtime(error.to_string())),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }

    pub fn stderr_tail(&self) -> String {
        let buffer = self.stderr.lock().expect("stderr lock poisoned");
        buffer.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    fn send(&mut self, message: &Message) -> Result<()> {
        write_message(&mut self.stdin, message)
    }
}

impl Drop for RuntimeClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn response_error(method: &str, error: Option<BridgeError>) -> VibeError {
    match error {
        Some(error) => {
            let details = error
                .details
                .map(|details| format!("\n{details}"))
                .unwrap_or_default();
            VibeError::Runtime(format!(
                "{method} failed [{}]: {}{details}",
                error.code, error.message
            ))
        }
        None => VibeError::Runtime(format!("{method} failed without an error payload")),
    }
}

fn spawn_stdout_reader(stdout: std::process::ChildStdout, tx: Sender<Inbound>) {
    std::thread::Builder::new()
        .name("vibe-java-stdout".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(message)) => {
                        if tx.send(Inbound::Message(message)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(Inbound::Eof);
                        break;
                    }
                    Err(error) => {
                        if tx.send(Inbound::Malformed(error.to_string())).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .expect("could not spawn Java stdout reader");
}

fn spawn_stderr_reader(stderr: std::process::ChildStderr, buffer: Arc<Mutex<VecDeque<String>>>) {
    std::thread::Builder::new()
        .name("vibe-java-stderr".into())
        .spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(std::result::Result::ok) {
                let mut buffer = buffer.lock().expect("stderr lock poisoned");
                if buffer.len() == 200 {
                    buffer.pop_front();
                }
                buffer.push_back(line);
            }
        })
        .expect("could not spawn Java stderr reader");
}

/// Helper for tests: look for a runtime jar/classes directory relative to a
/// repository root.
pub fn default_runtime_path(repo_root: &Path) -> Option<PathBuf> {
    let jar = repo_root
        .join("java")
        .join("build")
        .join("vibe-runtime.jar");
    if jar.is_file() {
        return Some(jar);
    }
    let classes = repo_root.join("java").join("build").join("classes");
    if classes.is_dir() {
        return Some(classes);
    }
    None
}

/// A command line for the Java runtime, used by tests and diagnostics.
pub fn java_command_line(java: &JavaInstallation, runtime: &Path) -> Command {
    let mut command = java.command();
    if runtime
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
    {
        command.arg("-jar").arg(runtime);
    } else {
        command
            .arg("-cp")
            .arg(runtime)
            .arg("dev.vibe.runtime.VibeRuntime");
    }
    command.arg("--stdio");
    command
}
