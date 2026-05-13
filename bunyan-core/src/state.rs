use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::backends::{self, RuntimeBackend};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub backend: Arc<dyn RuntimeBackend>,
}

impl AppState {
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            backend: backends::default_backend(),
        }
    }

    pub fn with_backend(db: Connection, backend: Arc<dyn RuntimeBackend>) -> Self {
        Self {
            db: Mutex::new(db),
            backend,
        }
    }
}
