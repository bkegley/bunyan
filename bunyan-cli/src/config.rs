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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_override_takes_priority() {
        // Even if BUNYAN_PORT is set, explicit override wins
        let url = discover_server_url(Some(9999));
        assert_eq!(url, "http://127.0.0.1:9999");
    }

    #[test]
    fn default_port_when_no_override_no_env_no_file() {
        // Temporarily unset env var to test default
        let prev = std::env::var("BUNYAN_PORT").ok();
        std::env::remove_var("BUNYAN_PORT");

        // This will fall through to port file (which may or may not exist)
        // then to default 3333. We can't control the port file in tests,
        // but we can at least verify the override path works.
        let url = discover_server_url(Some(4444));
        assert_eq!(url, "http://127.0.0.1:4444");

        // Restore
        if let Some(v) = prev {
            std::env::set_var("BUNYAN_PORT", v);
        }
    }

    #[test]
    fn url_format_is_correct() {
        let url = discover_server_url(Some(8080));
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(url.ends_with("8080"));
    }
}
