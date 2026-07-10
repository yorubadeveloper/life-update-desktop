//! Layer 4 orchestrator - runs only when the machine is idle and under low
//! load. Sessionizes unprocessed raw events, summarizes each session via
//! the selected engine, queues the results, and syncs anything unsent.

use super::frame_queue::FrameQueue;
use super::summarize::{summarize_session, RelatedSession, SummaryEngine};
use super::{apple_ai, cluster, db, idle, redaction::scan, sync, AgentConfig};
use base64::Engine as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CHECK_INTERVAL_SECONDS: u64 = 60;

const VISION_DESCRIBE_PROMPT: &str = "Describe what is being worked on in this screenshot in one or two plain sentences. Focus on the task or problem, not the UI chrome.";

fn describe_image_ollama(host: &str, model: &str, png: &[u8]) -> Result<String, String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/api/generate", host.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": VISION_DESCRIBE_PROMPT,
            "images": [b64],
            "stream": false,
            "keep_alive": 0,
        }))
        .send()
        .map_err(|e| format!("ollama unreachable: {e}"))?;
    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    Ok(body.get("response").and_then(|v| v.as_str()).unwrap_or("").trim().to_string())
}

pub fn process_pending_frames(cfg: &AgentConfig, frames: &FrameQueue) {
    for frame in frames.drain() {
        match describe_image_ollama(&cfg.ollama_host, &cfg.vision_engine, &frame.png_bytes) {
            Ok(description) if !description.is_empty() => {
                db::insert_raw(
                    &cfg.db_path,
                    &frame.ts, // original capture time, not processing time
                    "screen_text",
                    scan(frame.app_name.as_deref()).as_deref(),
                    scan(frame.title.as_deref()).as_deref(),
                    None,
                    scan(Some(&description)).as_deref(),
                );
            }
            Ok(_) => {}
            Err(e) => log::warn!("vision description failed for a queued frame, discarding: {e}"),
        }
    }
}

/// The gate that keeps guesswork off the timeline: a project named after
/// one of the apps used means the model had nothing real to go on.
pub fn project_is_tool_named(project: &str, apps: &[String]) -> bool {
    let p = project.trim().to_lowercase();
    p == "untitled session" || apps.iter().any(|a| a.trim().to_lowercase() == p)
}

/// One inference pass; returns sessions processed. The machine has been
/// idle past the threshold, so everything fetched belongs to a *completed*
/// period of activity - sessions dropped as too short are genuinely noise
/// and are marked processed; only an engine failure leaves events pending.
pub fn run_once(cfg: &AgentConfig) -> usize {
    let events = db::fetch_unprocessed(&cfg.db_path);
    if events.is_empty() {
        return 0;
    }

    let sessions = cluster::sessionize(events.clone(), cluster::DEFAULT_GAP_MINUTES, cluster::DEFAULT_MIN_SESSION_SECONDS);
    let attempted: std::collections::HashSet<i64> =
        sessions.iter().flatten().map(|e| e.id).collect();
    let noise: Vec<i64> = events.iter().map(|e| e.id).filter(|id| !attempted.contains(id)).collect();
    db::mark_processed(&cfg.db_path, &noise);

    let engine = if cfg.model == apple_ai::APPLE_ENGINE {
        SummaryEngine::Apple { helper: &cfg.helper_path }
    } else {
        SummaryEngine::Ollama { host: &cfg.ollama_host, model: &cfg.model }
    };

    // Memory: this user's most recent summarized sessions from the LOCAL
    // store - which the two-way sync keeps consistent with the web, so
    // web-side edits and deletions are reflected here too.
    let related: Vec<RelatedSession> = super::recent_sessions(10)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| !s.held) // never feed held guesswork back as memory
        .take(5)
        .map(|s| RelatedSession { project: s.project, summary: s.summary, ended_at: s.ended_at })
        .collect();

    let mut processed = 0;
    for session in &sessions {
        match summarize_session(session, &engine, &related, None) {
            Some(mut draft) => {
                // Quality gate: tool-named project = the model was guessing.
                // One corrective retry; if still guessing, HOLD it locally
                // for review instead of syncing guesswork to the timeline.
                let mut held = false;
                if project_is_tool_named(&draft.project, &draft.apps_used) {
                    let correction = format!(
                        "The previous attempt named the project \"{}\" - that is a tool or placeholder, never a project. Look again at titles, files and commits for the real name; if none is visible, use a short plain description of the activity instead.",
                        draft.project
                    );
                    if let Some(second) = summarize_session(session, &engine, &related, Some(&correction)) {
                        draft = second;
                    }
                    held = project_is_tool_named(&draft.project, &draft.apps_used);
                }
                sync::enqueue(&cfg.db_path, &draft, held);
                let ids: Vec<i64> = session.iter().map(|e| e.id).collect();
                db::mark_processed(&cfg.db_path, &ids);
                processed += 1;
            }
            None => {
                log::warn!("skipping session (engine unavailable or bad output), will retry next cycle");
            }
        }
    }
    processed
}

pub fn run(cfg: Arc<AgentConfig>, frames: Arc<FrameQueue>, stop: Arc<AtomicBool>) {
    let mut ticks = CHECK_INTERVAL_SECONDS; // first check soon after start
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_secs(1));
        ticks += 1;
        if ticks < CHECK_INTERVAL_SECONDS {
            continue;
        }
        ticks = 0;

        // Sync runs on the steady timer, NOT behind the idle gate - a
        // handful of small POSTs is cheap, and gating it meant nothing
        // reached life-update.com until the user stepped away for 3+
        // minutes. Only AI inference needs the idle/low-load gate.
        sync::sync_pending(&cfg.db_path, &cfg.api_url, &cfg.token);
        // Two-way: pull web-side edits/deletions back into the local store
        // on the same cadence, so the app (and the model's memory, which
        // reads locally) always reflects what the user curated on the web.
        sync::pull_remote_sessions(&cfg.db_path, &cfg.api_url, &cfg.token);

        if !idle::is_safe_to_run_inference(cfg.idle_threshold_minutes, cfg.cpu_load_ceiling_percent) {
            continue;
        }

        if frames.len() > 0 && crate::vision_models::is_ollama_backed(&cfg.vision_engine) {
            process_pending_frames(&cfg, &frames);
        }

        let count = run_once(&cfg);
        if count > 0 {
            log::info!("inference pass processed {count} session(s)");
            sync::sync_pending(&cfg.db_path, &cfg.api_url, &cfg.token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::project_is_tool_named;

    #[test]
    fn tool_named_projects_are_caught() {
        let apps = vec!["Warp".to_string(), "Google Chrome".to_string()];
        assert!(project_is_tool_named("Warp", &apps));
        assert!(project_is_tool_named("google chrome", &apps));
        assert!(project_is_tool_named("Untitled session", &apps));
        assert!(!project_is_tool_named("AcornTracker", &apps));
    }
}
