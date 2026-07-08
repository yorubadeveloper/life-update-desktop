//! Manages the bundled Ollama runtime's lifecycle.
//!
//! If something is already serving at `OLLAMA_HOST` (a system Ollama.app
//! install, or a previous run), it's reused rather than double-spawned -
//! and in that case we never kill it, since it isn't ours to kill. Only a
//! server *we* spawned gets stopped, and only on Quit (not on Pause -
//! pausing just stops the Python agent, leaving the model server warm).

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct OllamaProcess(pub Mutex<OllamaState>);

#[derive(Default)]
pub struct OllamaState {
    child: Option<Child>,
    we_own_it: bool,
}

/// The bundled runtime dir, if this is a real build - see
/// `scripts/fetch-ollama.sh` and `tauri.conf.json`'s `bundle.resources`.
fn resolve_bundled_binary(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let candidate = resource_dir.join("ollama-runtime").join("ollama");
    candidate.exists().then_some(candidate)
}

async fn is_healthy(host: &str) -> bool {
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(format!("{}/api/tags", host.trim_end_matches('/')))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub async fn ensure_server_running(app: &AppHandle, host: &str, state: &OllamaProcess) -> Result<(), String> {
    if is_healthy(host).await {
        return Ok(()); // reuse whatever's already serving - not ours to manage
    }

    let binary = resolve_bundled_binary(app).unwrap_or_else(|| PathBuf::from("ollama"));

    let child = Command::new(&binary)
        .arg("serve")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start ollama ({}): {e}", binary.display()))?;

    {
        let mut guard = state.0.lock().map_err(|e| e.to_string())?;
        guard.child = Some(child);
        guard.we_own_it = true;
    }

    for _ in 0..30 {
        if is_healthy(host).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err("ollama did not become healthy within 15s of starting".to_string())
}

pub fn stop(state: &OllamaProcess) {
    let mut guard = match state.0.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.we_own_it {
        if let Some(mut child) = guard.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    guard.we_own_it = false;
}
