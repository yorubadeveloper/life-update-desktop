//! Layer 1 - active window/app tracking. Polls the OS and records a row
//! only when the (app, title) pair changes. Exclude-list is checked before
//! an event is built; whatever survives goes through the Layer 2 scanner.

use super::{db, is_excluded, redaction::scan, AgentConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub fn read_active_window() -> (Option<String>, Option<String>) {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => {
            let app = if w.app_name.is_empty() { None } else { Some(w.app_name) };
            let title = if w.title.is_empty() { None } else { Some(w.title) };
            (app, title)
        }
        Err(_) => (None, None),
    }
}

pub fn run(cfg: Arc<AgentConfig>, stop: Arc<AtomicBool>) {
    let mut last_seen: Option<(Option<String>, Option<String>)> = None;

    while !stop.load(Ordering::Relaxed) {
        let (app_name, title) = read_active_window();

        let current = (app_name.clone(), title.clone());
        if last_seen.as_ref() != Some(&current) && (app_name.is_some() || title.is_some()) {
            last_seen = Some(current);

            if !is_excluded(&cfg, app_name.as_deref(), title.as_deref()) {
                db::insert_raw(
                    &cfg.db_path,
                    &db::now_iso(),
                    "window",
                    scan(app_name.as_deref()).as_deref(),
                    scan(title.as_deref()).as_deref(),
                    None,
                    None,
                );
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}
