"""Layer 4 - turns a clustered session into the structured "portfolio event"
shape the life-update.com API expects.

`category` and `project`/`summary` come from a single phi3:mini call (the
part suited to an LLM: naming and classifying). `focus_score` is computed
deterministically from duration and context-switch frequency - spec calls
for scoring on "duration, revisit frequency, time-of-day patterns", which
are numeric signals an LLM shouldn't be asked to guess.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any, Mapping, Sequence

from life_update_agent.inference.ollama_client import OllamaError, generate_json
from life_update_agent.inference.presidio_pass import redact_contextual_pii

VALID_CATEGORIES = {"deep_work", "maintenance", "meeting", "other"}

PROMPT_TEMPLATE = """You are summarizing a developer's work session from redacted activity logs.
Respond with ONLY a JSON object with exactly these keys:
- "project": a short (2-6 word) name for what they were working on
- "category": one of "deep_work", "maintenance", "meeting", "other"
- "summary": one or two plain sentences describing what was done, written like a changelog entry

Activity log:
{activity}

JSON:"""


def _parse_ts(ts: str) -> datetime:
    return datetime.fromisoformat(ts)


def build_session_text(session: Sequence[Mapping[str, Any]]) -> str:
    lines: list[str] = []
    seen: set[str] = set()

    for event in session:
        if event["kind"] == "window":
            entry = f"- worked in {event.get('app_name') or 'an app'}: {event.get('window_title') or ''}".strip()
        elif event["kind"] == "file":
            entry = f"- edited file: {event.get('file_path') or ''}"
        elif event["kind"] == "git_commit":
            entry = f"- committed: {event.get('extra_json') or ''}"
        else:
            continue

        if entry not in seen:
            seen.add(entry)
            lines.append(entry)

    return "\n".join(lines)


def compute_focus_score(session: Sequence[Mapping[str, Any]]) -> float:
    duration_minutes = (_parse_ts(session[-1]["ts"]) - _parse_ts(session[0]["ts"])).total_seconds() / 60
    duration_minutes = max(duration_minutes, 1.0)

    window_events = [e for e in session if e["kind"] == "window"]
    switches_per_minute = len(window_events) / duration_minutes

    duration_component = min(duration_minutes / 45.0, 1.0)
    switch_component = 1.0 / (1.0 + switches_per_minute)

    score = 0.6 * duration_component + 0.4 * switch_component
    return round(min(max(score, 0.0), 1.0), 3)


def distinct_apps_used(session: Sequence[Mapping[str, Any]]) -> list[str]:
    apps = {e.get("app_name") for e in session if e["kind"] == "window" and e.get("app_name")}
    return sorted(apps)


def summarize_session(
    session: Sequence[Mapping[str, Any]],
    ollama_host: str,
    ollama_model: str,
    spacy_model: str = "en_core_web_sm",
) -> dict[str, Any] | None:
    """Returns a portfolio-event dict, or None if the session couldn't be
    summarized (Ollama unreachable/bad output) - caller should leave the
    underlying raw_events unprocessed and retry next cycle."""
    activity_text = build_session_text(session)
    if not activity_text:
        return None

    redacted_text = redact_contextual_pii(activity_text, spacy_model)
    prompt = PROMPT_TEMPLATE.format(activity=redacted_text)

    try:
        result = generate_json(ollama_host, ollama_model, prompt)
    except OllamaError:
        return None

    category = result.get("category")
    if category not in VALID_CATEGORIES:
        category = "other"

    project = str(result.get("project") or "Untitled session").strip()[:120]
    summary = str(result.get("summary") or redacted_text).strip()[:300]

    return {
        "started_at": session[0]["ts"],
        "ended_at": session[-1]["ts"],
        "project": project,
        "category": category,
        "focus_score": compute_focus_score(session),
        "apps_used": distinct_apps_used(session),
        "summary": summary,
    }
