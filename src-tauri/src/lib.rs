mod commands;
mod db;
mod error;
mod git;
mod models;
mod process;
mod state;
mod terminal;

use rusqlite::Connection;

fn get_db_path() -> std::path::PathBuf {
    let app_dir = dirs::data_local_dir()
        .expect("Could not determine app data directory")
        .join("com.bunyan.app");

    std::fs::create_dir_all(&app_dir).expect("Could not create app data directory");

    app_dir.join("bunyan.db")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");

    db::initialize_database(&conn).expect("Failed to initialize database schema");

    let app_state = state::AppState::new(conn);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::repos::list_repos,
            commands::repos::get_repo,
            commands::repos::create_repo,
            commands::repos::update_repo,
            commands::repos::delete_repo,
            commands::workspaces::list_workspaces,
            commands::workspaces::get_workspace,
            commands::workspaces::create_workspace,
            commands::workspaces::archive_workspace,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            commands::claude::get_active_claude_sessions,
            commands::claude::open_claude_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
