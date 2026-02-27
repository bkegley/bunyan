mod commands;

use rusqlite::Connection;

/// macOS GUI apps get a minimal PATH. Resolve the user's shell PATH so we can
/// find tmux, git, docker, etc.
fn fix_path_env() {
    if let Ok(output) = std::process::Command::new("/bin/zsh")
        .args(["-l", "-c", "echo $PATH"])
        .output()
    {
        if let Ok(path) = String::from_utf8(output.stdout) {
            std::env::set_var("PATH", path.trim());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    fix_path_env();

    let db_path = bunyan_core::get_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");

    bunyan_core::db::initialize_database(&conn).expect("Failed to initialize database schema");

    let app_state = bunyan_core::state::AppState::new(conn);

    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
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
            commands::claude::get_workspace_sessions,
            commands::claude::resume_claude_session,
            commands::claude::list_workspace_panes,
            commands::claude::open_shell_pane,
            commands::claude::view_workspace,
            commands::claude::kill_pane,
            commands::docker::check_docker_available,
            commands::docker::get_container_status,
            commands::docker::get_container_ports,
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Throw);

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    // Spawn HTTP server on a background thread with its own AppState
    let server_port: u16 = std::env::var("BUNYAN_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3333);
    let server_state = bunyan_core::init_state();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(bunyan_core::server::start_server(server_state, server_port));
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
