pub mod error;
pub mod routes;

use std::sync::Arc;

use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;

use crate::models;
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::health::health,
        routes::repos::list,
        routes::repos::get,
        routes::repos::create,
        routes::repos::update,
        routes::repos::delete,
        routes::workspaces::list,
        routes::workspaces::get,
        routes::workspaces::create,
        routes::workspaces::archive,
        routes::workspaces::get_sessions,
        routes::workspaces::get_panes,
        routes::workspaces::start_claude,
        routes::workspaces::resume_claude,
        routes::workspaces::open_shell,
        routes::workspaces::view,
        routes::workspaces::kill_pane_handler,
        routes::docker::status,
        routes::docker::container_status,
        routes::docker::container_ports,
        routes::editors::detect,
        routes::editors::open,
        routes::sessions::active,
        routes::system::info,
        routes::settings::list,
        routes::settings::get,
        routes::settings::set,
    ),
    components(schemas(
        models::Repo,
        models::CreateRepoInput,
        models::UpdateRepoInput,
        models::WorkspaceState,
        models::ContainerMode,
        models::ContainerConfig,
        models::Workspace,
        models::CreateWorkspaceInput,
        models::Setting,
        models::TmuxPane,
        models::WorkspacePaneInfo,
        models::PortMapping,
        models::ClaudeSessionEntry,
        models::StatusResponse,
        models::DockerStatusResponse,
        models::ContainerStatusResponse,
        models::ClaudeResumeInput,
        models::OpenEditorInput,
        models::SetSettingInput,
        models::SystemInfo,
        models::ErrorResponse,
    )),
    tags(
        (name = "health", description = "Health check"),
        (name = "repos", description = "Repository management"),
        (name = "workspaces", description = "Workspace management"),
        (name = "sessions", description = "Claude session management"),
        (name = "docker", description = "Docker container management"),
        (name = "editors", description = "Editor detection and launch"),
        (name = "settings", description = "App settings"),
        (name = "system", description = "System information"),
    )
)]
struct ApiDoc;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // OpenAPI
        .route(
            "/api-doc/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        // Health
        .route("/health", get(routes::health::health))
        // Repos
        .route("/repos", get(routes::repos::list))
        .route("/repos", post(routes::repos::create))
        .route("/repos/{id}", get(routes::repos::get))
        .route("/repos/{id}", put(routes::repos::update))
        .route("/repos/{id}", delete(routes::repos::delete))
        // Workspaces
        .route("/workspaces", get(routes::workspaces::list))
        .route("/workspaces", post(routes::workspaces::create))
        .route("/workspaces/{id}", get(routes::workspaces::get))
        .route(
            "/workspaces/{id}/archive",
            post(routes::workspaces::archive),
        )
        .route(
            "/workspaces/{id}/sessions",
            get(routes::workspaces::get_sessions),
        )
        .route(
            "/workspaces/{id}/panes",
            get(routes::workspaces::get_panes),
        )
        .route(
            "/workspaces/{id}/claude",
            post(routes::workspaces::start_claude),
        )
        .route(
            "/workspaces/{id}/claude/resume",
            post(routes::workspaces::resume_claude),
        )
        .route(
            "/workspaces/{id}/shell",
            post(routes::workspaces::open_shell),
        )
        .route("/workspaces/{id}/view", post(routes::workspaces::view))
        .route(
            "/workspaces/{id}/panes/{index}",
            delete(routes::workspaces::kill_pane_handler),
        )
        // Docker
        .route("/docker/status", get(routes::docker::status))
        .route(
            "/workspaces/{id}/container/status",
            get(routes::docker::container_status),
        )
        .route(
            "/workspaces/{id}/container/ports",
            get(routes::docker::container_ports),
        )
        // Editors
        .route("/editors", get(routes::editors::detect))
        .route("/workspaces/{id}/editor", post(routes::editors::open))
        // Sessions
        .route("/sessions/active", get(routes::sessions::active))
        // System
        .route("/system/info", get(routes::system::info))
        // Settings
        .route("/settings", get(routes::settings::list))
        .route("/settings/{key}", get(routes::settings::get))
        .route("/settings/{key}", put(routes::settings::set))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn start_server(state: Arc<AppState>, port: u16) {
    let app = build_router(state);

    // Write port file for discovery
    let port_file = port_file_path();
    if let Some(parent) = port_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&port_file, port.to_string());

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to bind server");

    eprintln!("Bunyan server listening on http://127.0.0.1:{}", port);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(port_file.clone()))
        .await
        .expect("Server error");
}

fn port_file_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("Cannot determine home directory")
        .join(".bunyan")
        .join("server.port")
}

async fn shutdown_signal(port_file: std::path::PathBuf) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    // Cleanup port file
    let _ = std::fs::remove_file(&port_file);
    eprintln!("Bunyan server shutting down");
}
