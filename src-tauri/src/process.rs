use std::process::Command;

use crate::error::{BunyanError, Result};

pub trait ProcessDetector: Send + Sync {
    fn find_claude_pids(&self) -> Result<Vec<u32>>;
    fn get_pid_cwd(&self, pid: u32) -> Result<String>;
    fn get_pid_tty(&self, pid: u32) -> Result<Option<String>>;
}

pub struct RealProcessDetector;

impl ProcessDetector for RealProcessDetector {
    fn find_claude_pids(&self) -> Result<Vec<u32>> {
        let output = Command::new("pgrep")
            .args(["-x", "claude"])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to run pgrep: {}", e)))?;

        // pgrep exits with 1 when no processes found
        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pids = stdout
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect();

        Ok(pids)
    }

    fn get_pid_cwd(&self, pid: u32) -> Result<String> {
        let output = Command::new("lsof")
            .args(["-p", &pid.to_string(), "-Fn"])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to run lsof: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Process(format!("lsof failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("ncwd") {
                return Ok(path.to_string());
            }
        }

        Err(BunyanError::Process(format!(
            "Could not determine CWD for PID {}",
            pid
        )))
    }

    fn get_pid_tty(&self, pid: u32) -> Result<Option<String>> {
        let output = Command::new("ps")
            .args(["-o", "tty=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to run ps: {}", e)))?;

        if !output.status.success() {
            return Ok(None);
        }

        let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if tty.is_empty() || tty == "??" {
            Ok(None)
        } else {
            Ok(Some(tty))
        }
    }
}
