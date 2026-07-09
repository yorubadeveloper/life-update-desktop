//! Layer 4 - turns a clustered session into the portfolio-event shape the
//! life-update.com API expects. Naming/classifying goes to the selected
//! engine (Apple Intelligence by default, or an Ollama model); the focus
//! score is computed deterministically from duration and switch frequency.

use super::apple_ai;
use super::db::RawEvent;
use chrono::DateTime;
use std::collections::BTreeSet;
use std::path::Path;

const VALID_CATEGORIES: &[&str] = &["deep_work", "maintenance", "meeting", "learning", "creative", "admin", "personal", "other"];

const OLLAMA_PROMPT_TEMPLATE: &str = r#"You are summarizing a user's work session from redacted activity logs.
Respond with ONLY a JSON object with exactly these keys:
- "project": a short (2-6 word) name for what they were working on
- "category": one of "deep_work" (focused building/creation), "learning" (research, reading, studying), "creative" (design, art, music, writing), "meeting" (calls, live collaboration), "admin" (email, planning, errands), "personal" (travel, shopping, health, finances), "other"
- "summary": one or two plain sentences describing what was done, written like a changelog entry

Critical rules (note: no example names appear below on purpose - never copy a
name from these instructions into your output):
- The apps in the log (terminals, editors, browsers) are TOOLS the user was using - the user does not build or work for those apps. Never say they improved, enhanced, or worked on the tool itself.
- Window titles usually contain the real project, file, or document name. Take the project name from there, never from a tool or app name.
- Never output a placeholder or invented name. If no project name is visible in the log, use a short plain description of the activity as the project instead.
- Never include personal names, email addresses, usernames, or any other personally identifying details in your output - describe the work, not the people.
- The input may contain a [RECENT SESSIONS] section (previous summarized sessions). Summarize ONLY the [CURRENT SESSION LOG]; if it continues a recent session, reuse that session's exact project name and reflect the continuation.
- Recent sessions may contain mistakes. Never copy their wording or claims - reuse only a project name, and never a tool/app name even if a recent session used one.

{document}

JSON:"#;

pub enum SummaryEngine<'a> {
    Apple { helper: &'a Path },
    Ollama { host: &'a str, model: &'a str },
}

/// A previously summarized session, injected as memory so the model can
/// produce continuity ("returned to...", "continuing...") and reuse stable
/// project names instead of re-inventing one per session.
pub struct RelatedSession {
    pub project: String,
    pub summary: String,
    pub ended_at: String,
}

fn humanize_age(ended_at: &str) -> String {
    let Some(then) = parse_ts(ended_at) else { return "earlier".into() };
    let mins = (chrono::Utc::now().timestamp() - then.timestamp()) / 60;
    match mins {
        m if m < 60 => format!("{m}m ago"),
        m if m < 60 * 24 => format!("{}h ago", m / 60),
        m => format!("{}d ago", m / (60 * 24)),
    }
}

/// The document handed to the model: optional recent-session memory, then
/// the current log. Both engines consume the same shape.
pub fn build_prompt_document(related: &[RelatedSession], activity: &str) -> String {
    let mut doc = String::new();
    if !related.is_empty() {
        doc.push_str("[RECENT SESSIONS]\n");
        for r in related.iter().take(5) {
            doc.push_str(&format!(
                "- {} · {}: {}\n",
                humanize_age(&r.ended_at),
                r.project,
                r.summary
            ));
        }
        doc.push('\n');
    }
    doc.push_str("[CURRENT SESSION LOG]\n");
    doc.push_str(activity);
    doc
}

// The on-device model's context window is small (~4k tokens combined). A
// long session with screen watching on can exceed it, which would silently
// truncate the log. Map-reduce instead: condense chunks, then summarize.
const MAX_ACTIVITY_CHARS: usize = 6000;
const CHUNK_CHARS: usize = 4000;

fn condense_if_needed(activity: String, engine: &SummaryEngine) -> String {
    let SummaryEngine::Apple { helper } = engine else { return activity };
    if activity.len() <= MAX_ACTIVITY_CHARS {
        return activity;
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in activity.lines() {
        if current.len() + line.len() + 1 > CHUNK_CHARS && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    let condensed: Vec<String> = chunks
        .into_iter()
        .map(|c| super::apple_ai::condense(helper, &c).unwrap_or(c))
        .collect();
    let joined = condensed.join("\n");
    if joined.len() > MAX_ACTIVITY_CHARS {
        joined.chars().take(MAX_ACTIVITY_CHARS).collect()
    } else {
        joined
    }
}

pub struct PortfolioEventDraft {
    pub started_at: String,
    pub ended_at: String,
    pub project: String,
    pub category: String,
    pub focus_score: f64,
    pub apps_used: Vec<String>,
    pub summary: String,
}

fn parse_ts(ts: &str) -> Option<DateTime<chrono::FixedOffset>> {
    DateTime::parse_from_rfc3339(ts).ok()
}

pub fn build_session_text(session: &[RawEvent]) -> String {
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    for event in session {
        let entry = match event.kind.as_str() {
            "window" => format!(
                "- worked in {}: {}",
                event.app_name.as_deref().unwrap_or("an app"),
                event.window_title.as_deref().unwrap_or("")
            )
            .trim_end()
            .to_string(),
            "file" => format!("- edited file: {}", event.file_path.as_deref().unwrap_or("")),
            "git_commit" => format!("- committed: {}", event.extra_json.as_deref().unwrap_or("")),
            "screen_text" => format!("- on screen: {}", event.extra_json.as_deref().unwrap_or(""))
                .trim_end()
                .to_string(),
            _ => continue,
        };
        if seen.insert(entry.clone()) {
            lines.push(entry);
        }
    }
    lines.join("\n")
}

pub fn compute_focus_score(session: &[RawEvent]) -> f64 {
    let (Some(start), Some(end)) = (parse_ts(&session[0].ts), parse_ts(&session[session.len() - 1].ts)) else {
        return 0.0;
    };
    let duration_minutes = ((end - start).num_seconds() as f64 / 60.0).max(1.0);
    let window_events = session.iter().filter(|e| e.kind == "window").count() as f64;
    let switches_per_minute = window_events / duration_minutes;

    let duration_component = (duration_minutes / 45.0).min(1.0);
    let switch_component = 1.0 / (1.0 + switches_per_minute);

    let score = (0.6 * duration_component + 0.4 * switch_component).clamp(0.0, 1.0);
    (score * 1000.0).round() / 1000.0
}

pub fn distinct_apps_used(session: &[RawEvent]) -> Vec<String> {
    session
        .iter()
        .filter(|e| e.kind == "window")
        .filter_map(|e| e.app_name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ollama_generate_json(host: &str, model: &str, prompt: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/api/generate", host.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
            // Unload the model immediately after responding instead of
            // keeping multi-GB weights resident - the single biggest RAM
            // win over the old implementation.
            "keep_alive": 0,
        }))
        .send()
        .map_err(|e| format!("ollama unreachable: {e}"))?;
    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let raw = body.get("response").and_then(|v| v.as_str()).unwrap_or("");
    serde_json::from_str(raw).map_err(|e| format!("ollama returned non-JSON: {e}"))
}

/// Returns None if the session couldn't be summarized (engine unavailable
/// or bad output) - caller should leave the raw events unprocessed and
/// retry next cycle.
pub fn summarize_session(
    session: &[RawEvent],
    engine: &SummaryEngine,
    related: &[RelatedSession],
) -> Option<PortfolioEventDraft> {
    let activity = build_session_text(session);
    if activity.is_empty() {
        return None;
    }
    let activity = condense_if_needed(activity, engine);
    let document = build_prompt_document(related, &activity);

    let (project, category, summary) = match engine {
        SummaryEngine::Apple { helper } => match apple_ai::summarize(helper, &document) {
            Ok(s) => (s.project, s.category, s.summary),
            Err(e) => {
                log::warn!("apple intelligence summarization failed: {e}");
                return None;
            }
        },
        SummaryEngine::Ollama { host, model } => {
            match ollama_generate_json(host, model, &OLLAMA_PROMPT_TEMPLATE.replace("{document}", &document)) {
                Ok(v) => (
                    v.get("project").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    v.get("category").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    v.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                ),
                Err(e) => {
                    log::warn!("ollama summarization failed: {e}");
                    return None;
                }
            }
        }
    };

    let category = if VALID_CATEGORIES.contains(&category.as_str()) {
        category
    } else {
        "other".to_string()
    };
    let project = {
        let p = project.trim();
        // Models occasionally emit placeholder names despite instructions -
        // catch the classics and fall back rather than publish them.
        let lowered = p.to_lowercase();
        let is_placeholder = matches!(
            lowered.as_str(),
            "myapp" | "my app" | "project" | "project name" | "projectname" | "the project" | "untitled" | "unknown" | "n/a"
        );
        let p = if p.is_empty() || is_placeholder { "Untitled session" } else { p };
        p.chars().take(120).collect::<String>()
    };
    let summary = {
        let s = summary.trim();
        let s = if s.is_empty() { activity.as_str() } else { s };
        s.chars().take(300).collect::<String>()
    };

    Some(PortfolioEventDraft {
        started_at: session[0].ts.clone(),
        ended_at: session[session.len() - 1].ts.clone(),
        project,
        category,
        focus_score: compute_focus_score(session),
        apps_used: distinct_apps_used(session),
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: &str, kind: &str, app: Option<&str>, title: Option<&str>) -> RawEvent {
        RawEvent {
            id: 0,
            ts: ts.into(),
            kind: kind.into(),
            app_name: app.map(Into::into),
            window_title: title.map(Into::into),
            file_path: None,
            extra_json: None,
        }
    }

    #[test]
    fn session_text_dedupes_and_labels() {
        let session = vec![
            ev("2026-07-09T10:00:00+00:00", "window", Some("PyCharm"), Some("main.py")),
            ev("2026-07-09T10:01:00+00:00", "window", Some("PyCharm"), Some("main.py")),
        ];
        let text = build_session_text(&session);
        assert_eq!(text, "- worked in PyCharm: main.py");
    }

    #[test]
    fn focus_score_rewards_long_low_switch_sessions() {
        let long_focused: Vec<RawEvent> = vec![
            ev("2026-07-09T10:00:00+00:00", "window", Some("A"), None),
            ev("2026-07-09T10:45:00+00:00", "window", Some("A"), None),
        ];
        let short_scattered: Vec<RawEvent> = (0..20)
            .map(|i| ev(&format!("2026-07-09T10:00:{:02}+00:00", i * 2), "window", Some("A"), None))
            .collect();
        assert!(compute_focus_score(&long_focused) > compute_focus_score(&short_scattered));
    }
}
