//! Talks to the local Ollama server directly (rather than shelling out to
//! the Python CLI) so pull progress can stream to the frontend as native
//! Tauri events.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct PullProgress {
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
}

pub async fn list_local_models(host: &str) -> Result<HashSet<String>, String> {
    let url = format!("{}/api/tags", host.trim_end_matches('/'));
    let response = reqwest::get(&url).await.map_err(|e| format!("failed to reach ollama at {host}: {e}"))?;
    let body: TagsResponse = response.json().await.map_err(|e| e.to_string())?;
    Ok(body.models.into_iter().map(|m| m.name).collect())
}

pub async fn pull_model(app: &AppHandle, host: &str, model: &str) -> Result<(), String> {
    let url = format!("{}/api/pull", host.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("failed to reach ollama at {host}: {e}"))?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].to_string();
            buffer.drain(..=pos);
            if line.trim().is_empty() {
                continue;
            }

            let event: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if let Some(error) = event.get("error").and_then(|v| v.as_str()) {
                return Err(format!("pulling {model} failed: {error}"));
            }

            let progress = PullProgress {
                status: event.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                completed: event.get("completed").and_then(|v| v.as_u64()),
                total: event.get("total").and_then(|v| v.as_u64()),
            };
            let _ = app.emit("model-pull-progress", &progress);
        }
    }

    Ok(())
}
