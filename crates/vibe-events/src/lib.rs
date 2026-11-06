//! Vibe's internal event bus and Forge-like lifecycle state machine.
//!
//! This bus is for Vibe's Rust components. Events from the Java runtime arrive
//! through `vibe-bridge` and are re-emitted here when the CLI is driving a
//! session.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use vibe_core::error::{Result, VibeError};

type Callback = Arc<dyn Fn(&dyn Any) -> Result<()> + Send + Sync>;

struct Listener {
    name: String,
    callback: Callback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event: String,
    pub listener: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct EventBus {
    listeners: HashMap<TypeId, Vec<Listener>>,
    records: Vec<EventRecord>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe<E, F>(&mut self, name: impl Into<String>, callback: F)
    where
        E: Any,
        F: Fn(&E) -> Result<()> + Send + Sync + 'static,
    {
        let callback: Callback = Arc::new(move |any: &dyn Any| {
            let event = any
                .downcast_ref::<E>()
                .ok_or_else(|| VibeError::Protocol("event type mismatch".into()))?;
            callback(event)
        });
        self.listeners
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Listener {
                name: name.into(),
                callback,
            });
    }

    pub fn emit<E: Any>(&mut self, event: &E) -> usize {
        let event_name = std::any::type_name::<E>().to_string();
        let Some(listeners) = self.listeners.get(&TypeId::of::<E>()) else {
            return 0;
        };
        let mut called = 0;
        for listener in listeners {
            called += 1;
            match (listener.callback)(event) {
                Ok(()) => self.records.push(EventRecord {
                    event: event_name.clone(),
                    listener: listener.name.clone(),
                    ok: true,
                    error: None,
                }),
                Err(error) => self.records.push(EventRecord {
                    event: event_name.clone(),
                    listener: listener.name.clone(),
                    ok: false,
                    error: Some(error.to_string()),
                }),
            }
        }
        called
    }

    pub fn records(&self) -> &[EventRecord] {
        &self.records
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

/// Forge-inspired lifecycle phases as Vibe models them. These names describe
/// Vibe's side of the bridge; they are not a claim that Forge's internal state
/// machine has been reproduced exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Discovered,
    Inspected,
    Planned,
    RuntimeReady,
    Constructed,
    CommonSetup,
    SidedSetup,
    Complete,
    Failed,
}

impl Phase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Phase::Complete | Phase::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: Phase,
    pub to: Phase,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifecycle {
    phase: Phase,
    history: Vec<Transition>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            phase: Phase::Discovered,
            history: Vec::new(),
        }
    }
}

impl Lifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn history(&self) -> &[Transition] {
        &self.history
    }

    pub fn transition(&mut self, to: Phase, note: Option<String>) -> Result<()> {
        if !allowed(self.phase, to) {
            return Err(VibeError::Protocol(format!(
                "invalid lifecycle transition {:?} -> {:?}",
                self.phase, to
            )));
        }
        self.history.push(Transition {
            from: self.phase,
            to,
            note,
        });
        self.phase = to;
        Ok(())
    }

    pub fn fail(&mut self, note: impl Into<String>) {
        if self.phase != Phase::Failed {
            self.history.push(Transition {
                from: self.phase,
                to: Phase::Failed,
                note: Some(note.into()),
            });
            self.phase = Phase::Failed;
        }
    }
}

fn allowed(from: Phase, to: Phase) -> bool {
    if to == Phase::Failed {
        return from != Phase::Failed;
    }
    matches!(
        (from, to),
        (Phase::Discovered, Phase::Inspected)
            | (Phase::Inspected, Phase::Planned)
            | (Phase::Planned, Phase::RuntimeReady)
            | (Phase::RuntimeReady, Phase::Constructed)
            | (Phase::Constructed, Phase::CommonSetup)
            | (Phase::CommonSetup, Phase::SidedSetup)
            | (Phase::SidedSetup, Phase::Complete)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Ping(u32);

    #[test]
    fn dispatches_typed_events_and_records_failures() {
        let mut bus = EventBus::new();
        bus.subscribe::<Ping, _>("first", |event| {
            assert_eq!(event.0, 7);
            Ok(())
        });
        bus.subscribe::<Ping, _>("second", |_| Err(VibeError::Protocol("boom".into())));
        assert_eq!(bus.emit(&Ping(7)), 2);
        assert_eq!(bus.records().len(), 2);
        assert!(bus.records()[0].ok);
        assert!(!bus.records()[1].ok);
    }

    #[test]
    fn enforces_lifecycle_order() {
        let mut lifecycle = Lifecycle::new();
        assert!(lifecycle.transition(Phase::Inspected, None).is_ok());
        assert!(lifecycle.transition(Phase::Complete, None).is_err());
        lifecycle.transition(Phase::Planned, None).unwrap();
        lifecycle.transition(Phase::RuntimeReady, None).unwrap();
        lifecycle.transition(Phase::Constructed, None).unwrap();
        lifecycle.transition(Phase::CommonSetup, None).unwrap();
        lifecycle.transition(Phase::SidedSetup, None).unwrap();
        lifecycle.transition(Phase::Complete, None).unwrap();
        assert_eq!(lifecycle.history().len(), 7);
    }
}
