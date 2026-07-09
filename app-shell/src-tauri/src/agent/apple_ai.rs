//! Bridge to Apple Intelligence (the FoundationModels framework) via a
//! tiny bundled Swift helper. FoundationModels is Swift-only, so the
//! helper (swift/LifeUpdateAI.swift, built by scripts/build-ai-helper.sh)
//! is the one non-Rust piece: it reads an activity log on stdin and prints
//! a guided-generation JSON summary on stdout. The model itself is managed
//! entirely by macOS - nothing downloaded, nothing resident in our process.

use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const APPLE_ENGINE: &str = "apple-intelligence";

#[derive(Deserialize)]
pub struct AiSummary {
    pub project: String,
    pub category: String,
    pub summary: String,
}

pub fn helper_path(resource_dir: &Path) -> PathBuf {
    resource_dir.join("ai-helper").join("life-update-ai")
}

/// Ok(()) if Apple Intelligence is available on this machine; Err with the
/// OS-reported reason otherwise (not enabled, unsupported hardware, or the
/// helper missing - e.g. running on a macOS too old to have the framework).
pub fn availability(helper: &Path) -> Result<(), String> {
    let out = Command::new(helper)
        .arg("check")
        .output()
        .map_err(|e| format!("Apple Intelligence helper failed to launch: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let reason = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if reason.is_empty() {
            "Apple Intelligence is not available on this Mac".to_string()
        } else {
            reason
        })
    }
}

/// Summarize a redacted activity log into {project, category, summary}.
pub fn summarize(helper: &Path, activity: &str) -> Result<AiSummary, String> {
    let mut child = Command::new(helper)
        .arg("summarize")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch AI helper: {e}"))?;

    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(activity.as_bytes())
        .map_err(|e| e.to_string())?;

    // Local inference should finish in seconds; guard against a hung helper
    // so the worker loop can't stall forever.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                return Err("AI helper timed out".to_string());
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bad helper output: {e}"))
}
