//! The capture agent, in-process. What used to be a separate PyInstaller-
//! frozen Python daemon (spawned as a child process, ~900MB resident from
//! the spacy/presidio stack) is now a handful of Rust threads inside this
//! binary: window tracker, file watcher, optional screen watcher, and the
//! idle-gated inference worker. One process, named Life-Update, a few MB.

pub mod apple_ai;
mod cluster;
pub mod db;
mod file_watcher;
mod frame_queue;
mod idle;
mod redaction;
mod screen_watcher;
mod summarize;
mod sync;
mod vision_ocr;
mod window_tracker;
mod worker;

use crate::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub struct Running {
    stop: Arc<AtomicBool>,
    cfg: Arc<AgentConfig>,
    frames: Arc<frame_queue::FrameQueue>,
}

/// Some(Running) while the capture threads are up.
pub struct AgentProcess(pub Mutex<Option<Running>>);

pub struct AgentConfig {
    pub db_path: PathBuf,
    pub token: String,
    pub api_url: String,
    pub ollama_host: String,
    pub model: String,
    pub vision_engine: String,
    pub screen_watch_enabled: bool,
    pub screen_interval_seconds: f64,
    pub exclude_apps: Vec<String>,
    pub exclude_patterns: Vec<regex::Regex>,
    pub watch_dirs: Vec<PathBuf>,
    pub idle_threshold_minutes: f64,
    pub cpu_load_ceiling_percent: f64,
    pub helper_path: PathBuf,
}

/// The first safety layer - checked before anything is captured.
pub fn is_excluded(cfg: &AgentConfig, app_name: Option<&str>, window_title: Option<&str>) -> bool {
    // Never track ourselves - time spent in the Life-Update settings window
    // is meta-noise, not the user's work.
    if app_name == Some("Life-Update") || app_name == Some("app-shell") {
        return true;
    }
    if let Some(app) = app_name {
        let lowered = app.to_lowercase();
        if cfg.exclude_apps.iter().any(|e| lowered.contains(&e.to_lowercase())) {
            return true;
        }
    }
    if let Some(title) = window_title {
        if cfg.exclude_patterns.iter().any(|p| p.is_match(title)) {
            return true;
        }
    }
    false
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn load_config(app: &AppHandle) -> AgentConfig {
    let state = settings::read_state();
    let state_dir = settings::state_dir();
    let env = settings::read_env_values(
        &state_dir,
        &[
            "LIFE_UPDATE_TOKEN",
            "LIFE_UPDATE_API_URL",
            "OLLAMA_HOST",
            "WATCH_DIRS",
            "IDLE_THRESHOLD_MINUTES",
            "CPU_LOAD_CEILING_PERCENT",
        ],
    );

    // Configs written before the Rust rewrite may still say "tesseract";
    // Apple Vision OCR replaced it and is a strict upgrade.
    let vision_engine = if state.vision_engine == "tesseract" {
        crate::vision_models::NATIVE_ENGINE.to_string()
    } else {
        state.vision_engine
    };

    let helper_path = app
        .path()
        .resource_dir()
        .map(|d| apple_ai::helper_path(&d))
        .unwrap_or_else(|_| PathBuf::from("life-update-ai"));

    AgentConfig {
        db_path: state_dir.join("agent.db"),
        token: env.get("LIFE_UPDATE_TOKEN").cloned().unwrap_or_default(),
        // Canonical host: the apex 308-redirects to www, and HTTP clients
        // strip the Authorization header on cross-host redirects - which
        // made every sync silently fail. POST directly to www.
        api_url: env
            .get("LIFE_UPDATE_API_URL")
            .cloned()
            .unwrap_or_else(|| "https://www.life-update.com".to_string()),
        ollama_host: env
            .get("OLLAMA_HOST")
            .cloned()
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        model: state.ollama_model,
        vision_engine,
        screen_watch_enabled: state.screen_watch_enabled,
        screen_interval_seconds: state.screen_capture_interval_seconds,
        exclude_apps: state.apps,
        exclude_patterns: state
            .title_patterns
            .iter()
            .filter_map(|p| regex::Regex::new(p).ok())
            .collect(),
        watch_dirs: env
            .get("WATCH_DIRS")
            .map(|s| s.split(',').map(str::trim).filter(|d| !d.is_empty()).map(expand_home).collect())
            .unwrap_or_default(),
        idle_threshold_minutes: env
            .get("IDLE_THRESHOLD_MINUTES")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3.0),
        // 50, not the old 30: on machines that hover above 30% CPU even
        // when the user is away, a 30% ceiling meant summaries never ran
        // at all - sessions piled up as "queued" forever.
        cpu_load_ceiling_percent: env
            .get("CPU_LOAD_CEILING_PERCENT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(50.0),
        helper_path,
    }
}

pub fn start(app: &AppHandle, state: &AgentProcess) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(running) = guard.as_ref() {
        if !running.stop.load(Ordering::Relaxed) {
            return Ok(()); // already running
        }
    }

    let cfg = Arc::new(load_config(app));
    let _ = db::open(&cfg.db_path).map_err(|e| format!("failed to open local store: {e}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let frames = Arc::new(frame_queue::FrameQueue::default());

    {
        let (cfg, stop) = (cfg.clone(), stop.clone());
        std::thread::Builder::new()
            .name("window-tracker".into())
            .spawn(move || window_tracker::run(cfg, stop))
            .map_err(|e| e.to_string())?;
    }
    {
        let (cfg, stop) = (cfg.clone(), stop.clone());
        std::thread::Builder::new()
            .name("file-watcher".into())
            .spawn(move || file_watcher::run(cfg, stop))
            .map_err(|e| e.to_string())?;
    }
    if cfg.screen_watch_enabled {
        let (cfg, frames, stop) = (cfg.clone(), frames.clone(), stop.clone());
        std::thread::Builder::new()
            .name("screen-watcher".into())
            .spawn(move || screen_watcher::run(cfg, frames, stop))
            .map_err(|e| e.to_string())?;
    }
    {
        let (cfg, frames, stop) = (cfg.clone(), frames.clone(), stop.clone());
        std::thread::Builder::new()
            .name("inference-worker".into())
            .spawn(move || worker::run(cfg, frames, stop))
            .map_err(|e| e.to_string())?;
    }

    *guard = Some(Running { stop, cfg, frames });
    Ok(())
}

/// The config/frames of the running agent, for on-demand operations.
pub fn running_parts(state: &AgentProcess) -> Option<(Arc<AgentConfig>, Arc<frame_queue::FrameQueue>)> {
    let guard = state.0.lock().ok()?;
    let running = guard.as_ref()?;
    if running.stop.load(Ordering::Relaxed) {
        return None;
    }
    Some((running.cfg.clone(), running.frames.clone()))
}

/// "Summarize now": one immediate inference + sync pass, skipping the idle
/// gate entirely - the user asked for it, so it runs. Returns sessions
/// processed.
pub fn summarize_now_blocking(cfg: &AgentConfig, frames: &frame_queue::FrameQueue) -> usize {
    if frames.len() > 0 && crate::vision_models::is_ollama_backed(&cfg.vision_engine) {
        worker::process_pending_frames(cfg, frames);
    }
    let n = worker::run_once(cfg);
    sync::sync_pending(&cfg.db_path, &cfg.api_url, &cfg.token);
    n
}

pub fn stop(state: &AgentProcess) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(running) = guard.take() {
        running.stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub fn is_running(state: &AgentProcess) -> bool {
    state
        .0
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|r| !r.stop.load(Ordering::Relaxed)))
        .unwrap_or(false)
}

/// A captured (already-redacted) raw event, for the History view.
#[derive(Serialize)]
pub struct RawEventView {
    pub id: i64,
    pub ts: String,
    pub kind: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_path: Option<String>,
    pub extra_json: Option<String>,
    pub processed: bool,
}

pub fn recent_events(limit: u32) -> Result<Vec<RawEventView>, String> {
    let db_path = settings::state_dir().join("agent.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, ts, kind, app_name, window_title, file_path, extra_json, processed_at FROM raw_events ORDER BY id DESC LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], |r| {
            Ok(RawEventView {
                id: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                app_name: r.get(3)?,
                window_title: r.get(4)?,
                file_path: r.get(5)?,
                extra_json: r.get(6)?,
                processed: r.get::<_, Option<String>>(7)?.is_some(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// The raw events inside a session's time window - "what the summary saw".
pub fn session_events(started_at: &str, ended_at: &str) -> Result<Vec<RawEventView>, String> {
    let db_path = settings::state_dir().join("agent.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, ts, kind, app_name, window_title, file_path, extra_json, processed_at FROM raw_events WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts ASC LIMIT 500")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([started_at, ended_at], |r| {
            Ok(RawEventView {
                id: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                app_name: r.get(3)?,
                window_title: r.get(4)?,
                file_path: r.get(5)?,
                extra_json: r.get(6)?,
                processed: r.get::<_, Option<String>>(7)?.is_some(),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// A summarized session from the sync queue, for the Home view.
#[derive(Serialize)]
pub struct SessionView {
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
    pub project: String,
    pub category: String,
    pub focus_score: f64,
    pub apps_used: Vec<String>,
    pub summary: String,
    pub sent_at: Option<String>,
    pub held: bool,
}

pub fn recent_sessions(limit: u32) -> Result<Vec<SessionView>, String> {
    let db_path = settings::state_dir().join("agent.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, started_at, ended_at, project, category, focus_score, apps_used_json, summary, sent_at, COALESCE(held,0) FROM portfolio_event_queue ORDER BY created_at DESC LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit], |r| {
            let apps_json: String = r.get(6)?;
            Ok(SessionView {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                project: r.get(3)?,
                category: r.get(4)?,
                focus_score: r.get(5)?,
                apps_used: serde_json::from_str(&apps_json).unwrap_or_default(),
                summary: r.get(7)?,
                sent_at: r.get(8)?,
                held: r.get::<_, i64>(9)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Release a held session for syncing (next 60s tick sends it).
pub fn release_session(id: &str) -> Result<(), String> {
    let conn = db::open(&settings::state_dir().join("agent.db")).map_err(|e| e.to_string())?;
    conn.execute("UPDATE portfolio_event_queue SET held = 0 WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Discard a held session entirely.
pub fn discard_session(id: &str) -> Result<(), String> {
    let conn = db::open(&settings::state_dir().join("agent.db")).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM portfolio_event_queue WHERE id = ?1 AND sent_at IS NULL", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub use idle::{has_screen_recording_permission, request_screen_recording_permission};

#[derive(Serialize, Deserialize, Default)]
pub struct AgentStatus {
    pub unprocessed_raw_events: i64,
    pub total_captured_events: i64,
    pub unsent_portfolio_events: i64,
    pub total_synced_portfolio_events: i64,
    pub last_sync_at: Option<String>,
}

/// Reads status counts straight out of SQLite - polled by the Settings UI.
pub fn fetch_status() -> Result<AgentStatus, String> {
    let db_path = settings::state_dir().join("agent.db");
    if !db_path.exists() {
        return Ok(AgentStatus::default());
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;

    let count = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, [], |r| r.get(0)).map_err(|e| e.to_string())
    };

    Ok(AgentStatus {
        unprocessed_raw_events: count("SELECT COUNT(*) FROM raw_events WHERE processed_at IS NULL")?,
        total_captured_events: count("SELECT COUNT(*) FROM raw_events")?,
        unsent_portfolio_events: count("SELECT COUNT(*) FROM portfolio_event_queue WHERE sent_at IS NULL")?,
        total_synced_portfolio_events: count("SELECT COUNT(*) FROM portfolio_event_queue WHERE sent_at IS NOT NULL")?,
        last_sync_at: conn
            .query_row("SELECT MAX(sent_at) FROM portfolio_event_queue", [], |r| r.get(0))
            .map_err(|e| e.to_string())?,
    })
}
