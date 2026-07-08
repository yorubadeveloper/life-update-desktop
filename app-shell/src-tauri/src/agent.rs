//! Spawns/manages the Python daemon (`agent/`) as a child process.
//!
//! Dual-mode: in a real build, `agent_command()` resolves the PyInstaller
//! onedir bundle shipped as a Tauri resource directory and runs it
//! directly. In dev (`cargo tauri dev`), no such resource exists, so it
//! falls back to `uv run life-update-agent ...` in the sibling `agent/`
//! source directory - the dev workflow verified earlier keeps working
//! unchanged.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct AgentProcess(pub Mutex<Option<Child>>);

pub fn agent_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("agent")
}

/// The frozen binary, if this is a real build with the resource bundled -
/// see `agent/build.sh` and `tauri.conf.json`'s `bundle.resources`.
fn resolve_bundled_binary(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let candidate = resource_dir
        .join("life-update-agent")
        .join("life-update-agent");
    candidate.exists().then_some(candidate)
}

fn agent_command(app: &AppHandle, args: &[&str]) -> Command {
    if let Some(binary) = resolve_bundled_binary(app) {
        let mut cmd = Command::new(binary);
        cmd.args(args);
        cmd
    } else {
        let mut cmd = Command::new("uv");
        cmd.args(["run", "life-update-agent"]).args(args).current_dir(agent_dir());
        cmd
    }
}

pub fn start(app: &AppHandle, state: &AgentProcess) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;

    if let Some(child) = guard.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(()); // already running
        }
    }

    let child = agent_command(app, &["run"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start agent: {e}"))?;

    *guard = Some(child);
    Ok(())
}

pub fn stop(state: &AgentProcess) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

pub fn is_running(state: &AgentProcess) -> bool {
    let mut guard = match state.0.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match guard.as_mut() {
        Some(child) => match child.try_wait() {
            Ok(None) => true,
            _ => {
                *guard = None;
                false
            }
        },
        None => false,
    }
}

#[derive(Serialize, Deserialize)]
pub struct AgentStatus {
    pub unprocessed_raw_events: i64,
    pub total_captured_events: i64,
    pub unsent_portfolio_events: i64,
    pub total_synced_portfolio_events: i64,
    pub last_sync_at: Option<String>,
}

pub fn fetch_status(app: &AppHandle) -> Result<AgentStatus, String> {
    let output = agent_command(app, &["status", "--json"])
        .output()
        .map_err(|e| format!("failed to run status check: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
