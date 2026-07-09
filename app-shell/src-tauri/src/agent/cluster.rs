//! Layer 4 - groups raw events into candidate sessions by time gap.
//! Direct port of the Python agent's inference/cluster.py.

use super::db::RawEvent;
use chrono::DateTime;

pub const DEFAULT_GAP_MINUTES: f64 = 10.0;
pub const DEFAULT_MIN_SESSION_SECONDS: f64 = 60.0;

fn parse_ts(ts: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(ts).ok()
}

/// Events must be sorted ascending by `ts`. Sessions shorter than
/// `min_session_seconds` are dropped as noise.
pub fn sessionize(events: Vec<RawEvent>, gap_minutes: f64, min_session_seconds: f64) -> Vec<Vec<RawEvent>> {
    if events.is_empty() {
        return Vec::new();
    }

    let mut sessions: Vec<Vec<RawEvent>> = Vec::new();
    for event in events {
        let start_new = match sessions.last().and_then(|s| s.last()) {
            Some(prev) => match (parse_ts(&prev.ts), parse_ts(&event.ts)) {
                (Some(a), Some(b)) => (b - a).num_seconds() as f64 > gap_minutes * 60.0,
                _ => true,
            },
            None => true,
        };
        if start_new {
            sessions.push(vec![event]);
        } else {
            sessions.last_mut().unwrap().push(event);
        }
    }

    sessions.retain(|s| {
        match (parse_ts(&s[0].ts), parse_ts(&s[s.len() - 1].ts)) {
            (Some(a), Some(b)) => (b - a).num_seconds() as f64 >= min_session_seconds,
            _ => false,
        }
    });
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: i64, ts: &str) -> RawEvent {
        RawEvent {
            id,
            ts: ts.to_string(),
            kind: "window".into(),
            app_name: Some("App".into()),
            window_title: None,
            file_path: None,
            extra_json: None,
        }
    }

    #[test]
    fn splits_on_gap_and_drops_short_sessions() {
        let events = vec![
            ev(1, "2026-07-09T10:00:00+00:00"),
            ev(2, "2026-07-09T10:05:00+00:00"),
            // 30-minute gap starts a new session...
            ev(3, "2026-07-09T10:35:00+00:00"),
            // ...but that one is a single instant (0s duration) - dropped.
        ];
        let sessions = sessionize(events, DEFAULT_GAP_MINUTES, DEFAULT_MIN_SESSION_SECONDS);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].len(), 2);
        assert_eq!(sessions[0][0].id, 1);
    }

    #[test]
    fn empty_input() {
        assert!(sessionize(Vec::new(), 10.0, 60.0).is_empty());
    }
}
