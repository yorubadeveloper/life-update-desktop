//! Layer 5 - pushes queued portfolio events to life-update.com. Matches
//! app/api/portfolio-events/route.ts exactly: bearer auth, camelCase JSON,
//! idempotent upsert by id server-side. Unsent rows just retry next cycle.

use super::db;
use std::path::Path;
use uuid::Uuid;

// Same namespace as the Python agent, so event ids stay stable across the
// rewrite and the server-side upsert dedupes correctly.
const DEVICE_ID_NAMESPACE: Uuid = Uuid::from_u128(0x6f7f9e2c_6c1a_4c2b_9a1e_3d6b6c2f9a11);

pub fn enqueue(db_path: &Path, draft: &super::summarize::PortfolioEventDraft) {
    let event_id = Uuid::new_v5(
        &DEVICE_ID_NAMESPACE,
        format!("{}:{}", draft.started_at, draft.ended_at).as_bytes(),
    )
    .to_string();

    if let Ok(conn) = db::open(db_path) {
        let _ = conn.execute(
            "INSERT INTO portfolio_event_queue
             (id, started_at, ended_at, project, category, focus_score, apps_used_json, summary, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                event_id,
                draft.started_at,
                draft.ended_at,
                draft.project,
                draft.category,
                draft.focus_score,
                serde_json::to_string(&draft.apps_used).unwrap_or_else(|_| "[]".into()),
                draft.summary,
                db::now_iso(),
            ],
        );
    }
}

pub fn sync_pending(db_path: &Path, api_url: &str, token: &str) -> usize {
    if token.is_empty() {
        return 0;
    }
    let Ok(conn) = db::open(db_path) else { return 0 };
    let rows: Vec<(String, String, String, String, String, f64, String, String)> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, started_at, ended_at, project, category, focus_score, apps_used_json, summary
             FROM portfolio_event_queue WHERE sent_at IS NULL ORDER BY created_at ASC",
        ) else {
            return 0;
        };
        stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    };

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let url = format!("{}/api/portfolio-events", api_url.trim_end_matches('/'));

    let mut sent = 0;
    for (id, started_at, ended_at, project, category, focus_score, apps_json, summary) in rows {
        let apps: serde_json::Value = serde_json::from_str(&apps_json).unwrap_or(serde_json::json!([]));
        let resp = client
            .post(&url)
            .bearer_auth(token)
            .json(&serde_json::json!({
                "id": id,
                "startedAt": started_at,
                "endedAt": ended_at,
                "project": project,
                "category": category,
                "focusScore": focus_score,
                "appsUsed": apps,
                "summary": summary,
            }))
            .send();

        match resp {
            Ok(r) if r.status().is_success() => {
                let _ = conn.execute(
                    "UPDATE portfolio_event_queue SET sent_at = ?1 WHERE id = ?2",
                    rusqlite::params![db::now_iso(), id],
                );
                sent += 1;
            }
            Ok(r) => {
                log::warn!("sync rejected by server: {}", r.status());
            }
            Err(e) => {
                log::warn!("sync request failed (network), will retry next cycle: {e}");
                break; // network is down; no point trying the rest now
            }
        }
    }
    sent
}

/// Down-sync: reconcile the local session store against the server so the
/// two sides agree. Sessions edited on the web get their local copies
/// updated; sessions deleted on the web get deleted locally. Only rows
/// that have already been sent are touched (unsent rows haven't reached
/// the web yet), and deletion is bounded to the window the server actually
/// returned so old local history is never mass-deleted by a short feed.
pub fn pull_remote_sessions(db_path: &Path, api_url: &str, token: &str) {
    if token.is_empty() {
        return;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    else {
        return;
    };
    let Ok(resp) = client
        .get(format!("{}/api/portfolio-events?limit=200", api_url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
    else {
        return;
    };
    if !resp.status().is_success() {
        return;
    }
    let Ok(body) = resp.json::<serde_json::Value>() else { return };
    let Some(events) = body.get("events").and_then(|v| v.as_array()) else { return };

    let Ok(conn) = db::open(db_path) else { return };

    let mut server_ids = std::collections::HashSet::new();
    let mut window_min: Option<String> = None;
    for e in events {
        let (Some(id), Some(project), Some(category), Some(summary), Some(started_at)) = (
            e.get("id").and_then(|v| v.as_str()),
            e.get("project").and_then(|v| v.as_str()),
            e.get("category").and_then(|v| v.as_str()),
            e.get("summary").and_then(|v| v.as_str()),
            e.get("startedAt").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let focus = e.get("focusScore").and_then(|v| v.as_f64()).unwrap_or(0.0);
        server_ids.insert(id.to_string());
        if window_min.as_deref().map(|m| started_at < m).unwrap_or(true) {
            window_min = Some(started_at.to_string());
        }
        let _ = conn.execute(
            "UPDATE portfolio_event_queue SET project = ?1, category = ?2, summary = ?3, focus_score = ?4 WHERE id = ?5",
            rusqlite::params![project, category, summary, focus, id],
        );
    }

    // Deletions: any *sent* local row inside the server's window that the
    // server no longer returns was deleted on the web.
    let Some(window_min) = window_min else { return };
    let local_ids: Vec<String> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM portfolio_event_queue WHERE sent_at IS NOT NULL AND started_at >= ?1",
        ) else {
            return;
        };
        stmt.query_map([&window_min], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    for id in local_ids {
        if !server_ids.contains(&id) {
            let _ = conn.execute("DELETE FROM portfolio_event_queue WHERE id = ?1", rusqlite::params![id]);
        }
    }
}
