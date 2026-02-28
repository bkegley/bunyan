pub mod models;
pub mod error;
pub mod state;
pub mod db;
pub mod git;
pub mod tmux;
pub mod terminal;
pub mod editor;
pub mod docker;
pub mod workspace;
pub mod sessions;

#[cfg(feature = "server")]
pub mod server;

use rusqlite::Connection;
use std::sync::Arc;

pub fn get_db_path() -> std::path::PathBuf {
    let app_dir = dirs::data_local_dir()
        .expect("Could not determine app data directory")
        .join("com.bunyan.app");

    std::fs::create_dir_all(&app_dir).expect("Could not create app data directory");

    app_dir.join("bunyan.db")
}

pub fn init_state() -> Arc<state::AppState> {
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");
    db::initialize_database(&conn).expect("Failed to initialize database schema");
    Arc::new(state::AppState::new(conn))
}
