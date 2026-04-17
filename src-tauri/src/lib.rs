use std::process::Command as StdCommand;
use std::time::Duration;

/// macOS GUI apps get a minimal PATH. Append common tool directories so we can
/// find tmux, git, docker, etc. when launched from Finder.
fn fix_path_env() {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();

    let extra_dirs = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        &format!("{}/.cargo/bin", home),
        &format!("{}/.local/bin", home),
        // mise/asdf shims
        &format!("{}/.local/share/mise/shims", home),
        &format!("{}/.asdf/shims", home),
    ];

    let mut path = current;
    for dir in extra_dirs {
        if !dir.is_empty() && std::path::Path::new(dir).exists() && !path.contains(dir) {
            path = format!("{}:{}", path, dir);
        }
    }

    std::env::set_var("PATH", &path);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    fix_path_env();

    let port: u16 = std::env::var("BUNYAN_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3333);

    ensure_daemon_running(port);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn ensure_daemon_running(port: u16) {
    let url = format!("http://127.0.0.1:{}/health", port);

    // Check if already running
    if check_health(&url) {
        return;
    }

    // Find the `bunyan` CLI binary and use `bunyan serve` to start the daemon.
    // Look next to current exe first, then fall back to PATH.
    let bunyan_bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bunyan")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("bunyan"));

    StdCommand::new(&bunyan_bin)
        .args(["serve", "--port", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to start bunyan daemon. Is the `bunyan` CLI on your PATH?");

    // Wait for it to be ready
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if check_health(&url) {
            return;
        }
    }
    panic!("bunyan daemon did not start within 5 seconds");
}

fn check_health(url: &str) -> bool {
    ureq::get(url)
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}
