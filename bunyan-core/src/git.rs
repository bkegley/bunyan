use std::process::Command;

use crate::error::{BunyanError, Result};

pub trait GitOps: Send + Sync {
    fn clone_repo(&self, url: &str, path: &str) -> Result<()>;
    fn worktree_add(&self, repo_path: &str, worktree_path: &str, branch: &str) -> Result<()>;
    fn worktree_remove(&self, repo_path: &str, worktree_path: &str) -> Result<()>;
    #[allow(dead_code)]
    fn worktree_list(&self, repo_path: &str) -> Result<Vec<String>>;
}

pub struct RealGit;

impl GitOps for RealGit {
    fn clone_repo(&self, url: &str, path: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["clone", url, path])
            .output()
            .map_err(|e| BunyanError::Git(format!("Failed to run git clone: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Git(format!("git clone failed: {}", stderr)));
        }

        Ok(())
    }

    fn worktree_add(&self, repo_path: &str, worktree_path: &str, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["worktree", "add", worktree_path, "-b", branch])
            .current_dir(repo_path)
            .output()
            .map_err(|e| BunyanError::Git(format!("Failed to run git worktree add: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Git(format!(
                "git worktree add failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn worktree_remove(&self, repo_path: &str, worktree_path: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["worktree", "remove", worktree_path])
            .current_dir(repo_path)
            .output()
            .map_err(|e| BunyanError::Git(format!("Failed to run git worktree remove: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Git(format!(
                "git worktree remove failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn worktree_list(&self, repo_path: &str) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo_path)
            .output()
            .map_err(|e| BunyanError::Git(format!("Failed to run git worktree list: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Git(format!(
                "git worktree list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let worktrees = stdout
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .map(|line| line.trim_start_matches("worktree ").to_string())
            .collect();

        Ok(worktrees)
    }
}
