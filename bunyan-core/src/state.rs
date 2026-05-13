use rusqlite::Connection;
use std::sync::{Arc, Mutex, RwLock};

use crate::backends::{self, RuntimeBackend};
use crate::event_bus::EventBus;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub backend: Arc<dyn RuntimeBackend>,
    /// Origin URL the daemon is currently serving (e.g.
    /// "http://127.0.0.1:3333"). Set by the server at startup; observation
    /// URLs returned to delegating agents use this as their base.
    pub server_origin: RwLock<String>,
    /// In-process event bus. Routes publish lifecycle envelopes here so
    /// `/events` (SSE) subscribers see them in real time. The on-disk hook
    /// executor is the other consumer.
    pub event_bus: Arc<EventBus>,
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            backend: backends::default_backend(),
            server_origin: RwLock::new(default_origin()),
            event_bus: EventBus::new(256),
        }
    }

    pub fn with_backend(db: Connection, backend: Arc<dyn RuntimeBackend>) -> Self {
        Self {
            db: Mutex::new(db),
            backend,
            server_origin: RwLock::new(default_origin()),
            event_bus: EventBus::new(256),
        }
    }

    pub fn set_server_origin(&self, origin: impl Into<String>) {
        *self.server_origin.write().unwrap() = origin.into();
    }

    pub fn server_origin(&self) -> String {
        self.server_origin.read().unwrap().clone()
    }
}

fn default_origin() -> String {
    "http://127.0.0.1:3333".to_string()
}
