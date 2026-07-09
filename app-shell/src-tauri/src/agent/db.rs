//! Layer 3 - local SQLite store. Same schema and semantics as the Python
//! agent's db.py, so existing databases carry forward unchanged.

use rusqlite::Connection;
use std::path::Path;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS raw_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    kind TEXT NOT NULL,              -- 'window' | 'file' | 'git_commit' | 'screen_text'
    app_name TEXT,
    window_title TEXT,
    file_path TEXT,
    extra_json TEXT,
    processed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events (ts);
CREATE INDEX IF NOT EXISTS idx_raw_events_processed ON raw_events (processed_at);

CREATE TABLE IF NOT EXISTS portfolio_event_queue (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    project TEXT NOT NULL,
    category TEXT NOT NULL,
    focus_score REAL NOT NULL,
    apps_used_json TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL,
    sent_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_queue_unsent ON portfolio_event_queue (sent_at);
";

pub fn open(db_path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

/// One captured activity row, matching the raw_events schema.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub id: i64,
    pub ts: String,
    pub kind: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub file_path: Option<String>,
    pub extra_json: Option<String>,
}

pub fn insert_raw(
    db_path: &Path,
    ts: &str,
    kind: &str,
    app_name: Option<&str>,
    window_title: Option<&str>,
    file_path: Option<&str>,
    extra_json: Option<&str>,
) {
    if let Ok(conn) = open(db_path) {
        let _ = conn.execute(
            "INSERT INTO raw_events (ts, kind, app_name, window_title, file_path, extra_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![ts, kind, app_name, window_title, file_path, extra_json],
        );
    }
}

pub fn fetch_unprocessed(db_path: &Path) -> Vec<RawEvent> {
    let Ok(conn) = open(db_path) else { return Vec::new() };
    let Ok(mut stmt) =
        conn.prepare("SELECT id, ts, kind, app_name, window_title, file_path, extra_json FROM raw_events WHERE processed_at IS NULL ORDER BY ts ASC")
    else {
        return Vec::new();
    };
    stmt.query_map([], |row| {
        Ok(RawEvent {
            id: row.get(0)?,
            ts: row.get(1)?,
            kind: row.get(2)?,
            app_name: row.get(3)?,
            window_title: row.get(4)?,
            file_path: row.get(5)?,
            extra_json: row.get(6)?,
        })
    })
    .map(|rows| rows.filter_map(Result::ok).collect())
    .unwrap_or_default()
}

pub fn mark_processed(db_path: &Path, ids: &[i64]) {
    if ids.is_empty() {
        return;
    }
    if let Ok(conn) = open(db_path) {
        let now = now_iso();
        for id in ids {
            let _ = conn.execute(
                "UPDATE raw_events SET processed_at = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            );
        }
    }
}
