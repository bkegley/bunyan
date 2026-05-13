//! Filesystem-based hook executor.
//!
//! Bunyan emits lifecycle events; users can subscribe by dropping executable
//! scripts in well-known directories. The model mirrors git hooks: any
//! shebanged executable file works, there is no plugin manifest, and bunyan
//! does not load directories of helpers.
//!
//! Discovery order when an event fires (all matching hooks run sequentially):
//!
//! 1. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>`
//! 2. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>.d/*`
//! 3. `~/.config/bunyan/hooks/<event>`
//! 4. `~/.config/bunyan/hooks/<event>.d/*`
//!
//! A hook may return exit code 78 to short-circuit further hooks for this event.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SHORT_CIRCUIT_EXIT_CODE: i32 = 78;
const DEFAULT_TIMEOUT_SECS: u64 = 10;
const BOOTSTRAP_TIMEOUT_SECS: u64 = 300;

/// Outcome of a single hook invocation.
#[derive(Debug, Clone)]
pub struct HookOutcome {
    pub path: PathBuf,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl HookOutcome {
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0) || self.exit_code == Some(SHORT_CIRCUIT_EXIT_CODE)
    }

    pub fn short_circuited(&self) -> bool {
        self.exit_code == Some(SHORT_CIRCUIT_EXIT_CODE)
    }
}

/// Aggregate result of firing an event.
#[derive(Debug, Default, Clone)]
pub struct HookRunResult {
    pub outcomes: Vec<HookOutcome>,
}

impl HookRunResult {
    /// True if at least one hook executed and succeeded (or short-circuited).
    pub fn any_succeeded(&self) -> bool {
        self.outcomes.iter().any(|o| o.succeeded())
    }

    /// True if at least one hook ran (regardless of exit code).
    pub fn any_ran(&self) -> bool {
        !self.outcomes.is_empty()
    }
}

/// Context passed to hooks via environment variables.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub event: String,
    pub repo_name: Option<String>,
    pub repo_id: Option<String>,
    pub workspace_name: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_path: Option<String>,
    pub branch: Option<String>,
    pub server_port: Option<u16>,
    pub repo_root_path: Option<String>,
    /// Event-specific extras passed through as BUNYAN_<UPPER_KEY> env vars.
    pub extras: HashMap<String, String>,
}

impl HookContext {
    pub fn new(event: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            ..Default::default()
        }
    }

    pub fn with_repo(mut self, name: impl Into<String>, id: impl Into<String>) -> Self {
        self.repo_name = Some(name.into());
        self.repo_id = Some(id.into());
        self
    }

    pub fn with_workspace(
        mut self,
        name: impl Into<String>,
        id: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        self.workspace_name = Some(name.into());
        self.workspace_id = Some(id.into());
        self.workspace_path = Some(path.into());
        self
    }

    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    pub fn with_repo_root(mut self, path: impl Into<String>) -> Self {
        self.repo_root_path = Some(path.into());
        self
    }

    pub fn with_server_port(mut self, port: u16) -> Self {
        self.server_port = Some(port);
        self
    }

    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extras.insert(key.into(), value.into());
        self
    }

    fn timestamp(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    /// Build the environment variable map a hook will receive.
    fn build_env(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("BUNYAN_EVENT".into(), self.event.clone());
        env.insert("BUNYAN_EVENT_TIMESTAMP".into(), self.timestamp());
        if let Some(v) = &self.repo_name {
            env.insert("BUNYAN_REPO".into(), v.clone());
        }
        if let Some(v) = &self.repo_id {
            env.insert("BUNYAN_REPO_ID".into(), v.clone());
        }
        if let Some(v) = &self.workspace_name {
            env.insert("BUNYAN_WORKSPACE".into(), v.clone());
        }
        if let Some(v) = &self.workspace_id {
            env.insert("BUNYAN_WORKSPACE_ID".into(), v.clone());
        }
        if let Some(v) = &self.workspace_path {
            env.insert("BUNYAN_PATH".into(), v.clone());
        }
        if let Some(v) = &self.branch {
            env.insert("BUNYAN_BRANCH".into(), v.clone());
        }
        if let Some(p) = self.server_port {
            env.insert("BUNYAN_SERVER_PORT".into(), p.to_string());
        }
        for (k, v) in &self.extras {
            env.insert(format!("BUNYAN_{}", k.to_uppercase()), v.clone());
        }
        env
    }
}

/// Resolver that returns the candidate hook search roots.
///
/// Abstracted so tests can inject temp directories without touching `$HOME`.
pub trait HookRoots {
    /// `~/.config/bunyan/hooks` (or its test equivalent).
    fn user_root(&self) -> Option<PathBuf>;
    /// `~/bunyan/repos/<repo>/.bunyan/hooks` (or its test equivalent).
    fn repo_root(&self, repo_name: &str) -> Option<PathBuf>;
}

/// Default resolver using `dirs` + the per-repo path the user passed in.
pub struct DefaultHookRoots {
    pub repo_root_path: Option<PathBuf>,
}

impl DefaultHookRoots {
    pub fn new(repo_root_path: Option<PathBuf>) -> Self {
        Self { repo_root_path }
    }
}

impl HookRoots for DefaultHookRoots {
    fn user_root(&self) -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("bunyan").join("hooks"))
    }

    fn repo_root(&self, _repo_name: &str) -> Option<PathBuf> {
        self.repo_root_path
            .as_ref()
            .map(|p| p.join(".bunyan").join("hooks"))
    }
}

/// Discover the ordered list of hook executables for an event.
pub fn discover_hooks(roots: &dyn HookRoots, event: &str, repo_name: Option<&str>) -> Vec<PathBuf> {
    let mut hooks: Vec<PathBuf> = Vec::new();

    if let Some(repo) = repo_name {
        if let Some(root) = roots.repo_root(repo) {
            collect_for_event(&root, event, &mut hooks);
        }
    }
    if let Some(root) = roots.user_root() {
        collect_for_event(&root, event, &mut hooks);
    }
    hooks
}

fn collect_for_event(root: &Path, event: &str, out: &mut Vec<PathBuf>) {
    let single = root.join(event);
    if is_executable_file(&single) {
        out.push(single);
    }
    let dir = root.join(format!("{}.d", event));
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_executable_file(p))
                .collect();
            files.sort();
            out.extend(files);
        }
    }
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && (m.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// Pick the timeout for an event. Tunable later via config.
fn default_timeout_for(event: &str) -> Duration {
    if event == "workspace.created" {
        Duration::from_secs(BOOTSTRAP_TIMEOUT_SECS)
    } else {
        Duration::from_secs(DEFAULT_TIMEOUT_SECS)
    }
}

/// Run all discovered hooks for an event, in order.
///
/// - Each hook gets the context as environment variables.
/// - Each hook runs with a per-event timeout; on timeout, the child is killed.
/// - A hook returning exit code 78 stops further hooks for this event.
/// - Hook failures are logged to stderr and do not crash the caller.
pub fn fire(roots: &dyn HookRoots, ctx: &HookContext) -> HookRunResult {
    let hooks = discover_hooks(roots, &ctx.event, ctx.repo_name.as_deref());
    let timeout = default_timeout_for(&ctx.event);
    let env = ctx.build_env();

    let mut result = HookRunResult::default();
    for hook in hooks {
        let outcome = run_one(&hook, &env, timeout);
        let short_circuit = outcome.short_circuited();
        log_outcome(&ctx.event, &outcome);
        result.outcomes.push(outcome);
        if short_circuit {
            break;
        }
    }
    result
}

fn run_one(path: &Path, env: &HashMap<String, String>, timeout: Duration) -> HookOutcome {
    let start = Instant::now();
    let mut cmd = Command::new(path);
    cmd.envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return HookOutcome {
                path: path.to_path_buf(),
                exit_code: None,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: format!("failed to spawn hook: {}", e),
                timed_out: false,
            };
        }
    };

    // Take the pipes immediately and drain them on background threads. This
    // prevents two bugs:
    //   1. If the hook writes more than the pipe buffer, it blocks on write.
    //   2. After we kill a hook that spawned grandchildren, the parent
    //      process is gone but the orphaned grandchildren may still hold the
    //      pipe open. Reading on a thread lets us bound the total wait time.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = stdout.map(|mut s| {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        rx
    });
    let stderr_handle = stderr.map(|mut s| {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        rx
    });

    // Poll-with-timeout. Simple and avoids pulling in another dep.
    let poll_interval = Duration::from_millis(25);
    let deadline = start + timeout;
    let (exit_status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let killed = child.wait().ok();
                    break (killed, true);
                }
                thread::sleep(poll_interval);
            }
            Err(_) => break (None, false),
        }
    };

    // Drain the pipe readers with a small grace window. If the hook left
    // orphaned children holding the pipes open, we don't want to block on them.
    let drain = Duration::from_millis(500);
    let stdout = stdout_handle
        .and_then(|rx| rx.recv_timeout(drain).ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|rx| rx.recv_timeout(drain).ok())
        .unwrap_or_default();

    let exit_code = exit_status.and_then(|s| s.code());
    HookOutcome {
        path: path.to_path_buf(),
        exit_code,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
    }
}

fn log_outcome(event: &str, o: &HookOutcome) {
    let path = o.path.display();
    if o.timed_out {
        eprintln!(
            "[bunyan/hooks] {} {} timed out after {:?}",
            event, path, o.duration
        );
    } else {
        match o.exit_code {
            Some(0) => eprintln!(
                "[bunyan/hooks] {} {} ok in {:?}",
                event, path, o.duration
            ),
            Some(SHORT_CIRCUIT_EXIT_CODE) => eprintln!(
                "[bunyan/hooks] {} {} short-circuited further hooks (exit 78) in {:?}",
                event, path, o.duration
            ),
            Some(code) => eprintln!(
                "[bunyan/hooks] {} {} exited {} in {:?}",
                event, path, code, o.duration
            ),
            None => eprintln!(
                "[bunyan/hooks] {} {} terminated without exit code in {:?}",
                event, path, o.duration
            ),
        }
    }
    if !o.stdout.is_empty() {
        eprintln!("[bunyan/hooks] {} {} stdout: {}", event, path, o.stdout.trim_end());
    }
    if !o.stderr.is_empty() {
        eprintln!("[bunyan/hooks] {} {} stderr: {}", event, path, o.stderr.trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct StaticRoots {
        user: Option<PathBuf>,
        repo: Option<PathBuf>,
    }
    impl HookRoots for StaticRoots {
        fn user_root(&self) -> Option<PathBuf> {
            self.user.clone()
        }
        fn repo_root(&self, _repo: &str) -> Option<PathBuf> {
            self.repo.clone()
        }
    }

    fn write_hook(path: &Path, script: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, script).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    fn unique_tempdir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bunyan-hooks-test-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discover_finds_global_event_file() {
        let tmp = unique_tempdir("discover_global");
        write_hook(&tmp.join("workspace.ready_to_view"), "#!/bin/sh\nexit 0\n");
        let roots = StaticRoots {
            user: Some(tmp.clone()),
            repo: None,
        };
        let hooks = discover_hooks(&roots, "workspace.ready_to_view", Some("repo"));
        assert_eq!(hooks.len(), 1);
        assert!(hooks[0].ends_with("workspace.ready_to_view"));
    }

    #[test]
    fn discover_skips_non_executable_files() {
        let tmp = unique_tempdir("discover_non_exec");
        let p = tmp.join("workspace.ready_to_view");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
        // Do NOT chmod +x
        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let hooks = discover_hooks(&roots, "workspace.ready_to_view", None);
        assert!(hooks.is_empty());
    }

    #[test]
    fn discover_orders_repo_before_user_and_sorts_dotd_lexically() {
        let user = unique_tempdir("discover_user");
        let repo = unique_tempdir("discover_repo");
        write_hook(&repo.join("evt"), "#!/bin/sh\nexit 0\n");
        write_hook(&repo.join("evt.d/30-late.sh"), "#!/bin/sh\nexit 0\n");
        write_hook(&repo.join("evt.d/10-early.sh"), "#!/bin/sh\nexit 0\n");
        write_hook(&user.join("evt"), "#!/bin/sh\nexit 0\n");
        write_hook(&user.join("evt.d/20-mid.sh"), "#!/bin/sh\nexit 0\n");

        let roots = StaticRoots {
            user: Some(user.clone()),
            repo: Some(repo.clone()),
        };
        let hooks = discover_hooks(&roots, "evt", Some("repo"));

        let names: Vec<String> = hooks
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["evt", "10-early.sh", "30-late.sh", "evt", "20-mid.sh"]);
        // First two must be from repo, last two from user
        assert!(hooks[0].starts_with(&repo));
        assert!(hooks[1].starts_with(&repo));
        assert!(hooks[3].starts_with(&user));
    }

    #[test]
    fn fire_passes_env_vars_to_hook() {
        let tmp = unique_tempdir("fire_env");
        let marker = tmp.join("marker.txt");
        let script = format!(
            "#!/bin/sh\necho \"$BUNYAN_EVENT $BUNYAN_REPO $BUNYAN_WORKSPACE $BUNYAN_PATH\" > {}\n",
            marker.display()
        );
        write_hook(&tmp.join("workspace.ready_to_view"), &script);

        let roots = StaticRoots {
            user: Some(tmp.clone()),
            repo: None,
        };
        let ctx = HookContext::new("workspace.ready_to_view")
            .with_repo("frontend", "repo-id")
            .with_workspace("ws-name", "ws-id", "/tmp/fake-path")
            .with_branch("fix-bug")
            .with_server_port(3333);

        let result = fire(&roots, &ctx);
        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(result.outcomes[0].exit_code, Some(0));
        let content = fs::read_to_string(&marker).unwrap();
        assert_eq!(
            content.trim(),
            "workspace.ready_to_view frontend ws-name /tmp/fake-path"
        );
    }

    #[test]
    fn fire_captures_stdout_and_stderr_and_continues_on_failure() {
        let tmp = unique_tempdir("fire_capture");
        write_hook(
            &tmp.join("evt.d/10-fail.sh"),
            "#!/bin/sh\necho hello-out\necho hello-err 1>&2\nexit 3\n",
        );
        write_hook(&tmp.join("evt.d/20-ok.sh"), "#!/bin/sh\necho ok\nexit 0\n");

        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let ctx = HookContext::new("evt");
        let result = fire(&roots, &ctx);

        assert_eq!(result.outcomes.len(), 2);
        assert_eq!(result.outcomes[0].exit_code, Some(3));
        assert!(result.outcomes[0].stdout.contains("hello-out"));
        assert!(result.outcomes[0].stderr.contains("hello-err"));
        assert!(!result.outcomes[0].succeeded());
        assert_eq!(result.outcomes[1].exit_code, Some(0));
        assert!(result.outcomes[1].succeeded());
    }

    #[test]
    fn fire_short_circuits_on_exit_78() {
        let tmp = unique_tempdir("fire_short_circuit");
        write_hook(&tmp.join("evt.d/10-short.sh"), "#!/bin/sh\nexit 78\n");
        write_hook(&tmp.join("evt.d/20-never.sh"), "#!/bin/sh\nexit 0\n");

        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let ctx = HookContext::new("evt");
        let result = fire(&roots, &ctx);

        assert_eq!(result.outcomes.len(), 1);
        assert_eq!(result.outcomes[0].exit_code, Some(SHORT_CIRCUIT_EXIT_CODE));
        assert!(result.outcomes[0].short_circuited());
        assert!(result.outcomes[0].succeeded());
    }

    #[test]
    fn fire_returns_empty_when_no_hooks_present() {
        let tmp = unique_tempdir("fire_empty");
        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let ctx = HookContext::new("workspace.ready_to_view");
        let result = fire(&roots, &ctx);
        assert!(!result.any_ran());
        assert!(!result.any_succeeded());
    }

    #[test]
    fn fire_kills_hook_that_exceeds_timeout() {
        let tmp = unique_tempdir("fire_timeout");
        // 'evt' has a 10s default timeout. Sleep 30s; we expect the hook to be
        // killed well before that. The test should complete in ~10s.
        write_hook(&tmp.join("evt"), "#!/bin/sh\nsleep 30\n");

        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let ctx = HookContext::new("evt");
        let started = Instant::now();
        let result = fire(&roots, &ctx);
        let elapsed = started.elapsed();

        assert_eq!(result.outcomes.len(), 1);
        assert!(result.outcomes[0].timed_out);
        assert!(
            elapsed < Duration::from_secs(15),
            "hook should have been killed within ~10s, took {:?}",
            elapsed
        );
    }

    #[test]
    fn fire_runs_repo_hook_before_user_hook() {
        let user = unique_tempdir("ordering_user");
        let repo = unique_tempdir("ordering_repo");
        let marker = unique_tempdir("ordering_marker").join("out.txt");

        write_hook(
            &repo.join("evt"),
            &format!("#!/bin/sh\necho repo >> {}\n", marker.display()),
        );
        write_hook(
            &user.join("evt"),
            &format!("#!/bin/sh\necho user >> {}\n", marker.display()),
        );

        let roots = StaticRoots {
            user: Some(user),
            repo: Some(repo),
        };
        let ctx = HookContext::new("evt").with_repo("r", "id");
        let result = fire(&roots, &ctx);
        assert_eq!(result.outcomes.len(), 2);

        let content = fs::read_to_string(&marker).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines, vec!["repo", "user"]);
    }

    #[test]
    fn extras_are_exposed_as_uppercase_env_vars() {
        let tmp = unique_tempdir("extras");
        let marker = tmp.join("m.txt");
        write_hook(
            &tmp.join("claude.started"),
            &format!(
                "#!/bin/sh\necho \"$BUNYAN_SESSION_ID\" > {}\n",
                marker.display()
            ),
        );
        let roots = StaticRoots {
            user: Some(tmp),
            repo: None,
        };
        let ctx = HookContext::new("claude.started").with_extra("session_id", "abc-123");
        let result = fire(&roots, &ctx);
        assert_eq!(result.outcomes.len(), 1);
        let content = fs::read_to_string(&marker).unwrap();
        assert_eq!(content.trim(), "abc-123");
    }
}
