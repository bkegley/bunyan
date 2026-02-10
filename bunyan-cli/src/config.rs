use std::path::PathBuf;

/// Discover the server URL.
/// Priority: --port flag > BUNYAN_PORT env > ~/.bunyan/server.port file > default 3333
pub fn discover_server_url(port_override: Option<u16>) -> String {
    let port = port_override
        .or_else(|| {
            std::env::var("BUNYAN_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
        })
        .or_else(|| read_port_file())
        .unwrap_or(3333);

    format!("http://127.0.0.1:{}", port)
}

fn read_port_file() -> Option<u16> {
    let path = port_file_path()?;
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn port_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".bunyan").join("server.port"))
}
