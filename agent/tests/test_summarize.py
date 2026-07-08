from life_update_agent.inference.summarize import (
    build_session_text,
    compute_focus_score,
    distinct_apps_used,
)


def _window_event(ts, app_name, title):
    return {"ts": ts, "kind": "window", "app_name": app_name, "window_title": title}


def test_focus_score_high_for_long_single_app_session():
    session = [
        _window_event("2026-07-08T10:00:00+00:00", "VS Code", "main.py"),
        _window_event("2026-07-08T10:44:00+00:00", "VS Code", "main.py"),
    ]
    score = compute_focus_score(session)
    assert score > 0.7


def test_focus_score_low_for_short_thrashing_session():
    session = [
        _window_event(f"2026-07-08T10:00:{i:02d}+00:00", f"App{i}", "x")
        for i in range(20)
    ]
    score = compute_focus_score(session)
    assert score < 0.5


def test_distinct_apps_used_dedupes_and_sorts():
    session = [
        _window_event("2026-07-08T10:00:00+00:00", "Zed", "x"),
        _window_event("2026-07-08T10:01:00+00:00", "Chrome", "y"),
        _window_event("2026-07-08T10:02:00+00:00", "Zed", "z"),
    ]
    assert distinct_apps_used(session) == ["Chrome", "Zed"]


def test_build_session_text_dedupes_lines():
    session = [
        _window_event("2026-07-08T10:00:00+00:00", "VS Code", "main.py"),
        _window_event("2026-07-08T10:01:00+00:00", "VS Code", "main.py"),
        {"ts": "2026-07-08T10:02:00+00:00", "kind": "file", "file_path": "main.py"},
    ]
    text = build_session_text(session)
    assert text.count("worked in VS Code") == 1
    assert "edited file: main.py" in text
