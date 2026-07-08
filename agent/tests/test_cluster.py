from life_update_agent.inference.cluster import sessionize


def _event(ts: str, **extra):
    return {"ts": ts, "kind": "window", **extra}


def test_single_session_when_gaps_small():
    events = [
        _event("2026-07-08T10:00:00+00:00"),
        _event("2026-07-08T10:05:00+00:00"),
        _event("2026-07-08T10:09:00+00:00"),
    ]
    sessions = sessionize(events, gap_minutes=10, min_session_seconds=60)
    assert len(sessions) == 1
    assert len(sessions[0]) == 3


def test_splits_on_large_gap():
    events = [
        _event("2026-07-08T10:00:00+00:00"),
        _event("2026-07-08T10:05:00+00:00"),
        _event("2026-07-08T14:00:00+00:00"),
        _event("2026-07-08T14:03:00+00:00"),
    ]
    sessions = sessionize(events, gap_minutes=10, min_session_seconds=60)
    assert len(sessions) == 2
    assert len(sessions[0]) == 2
    assert len(sessions[1]) == 2


def test_short_sessions_dropped():
    events = [
        _event("2026-07-08T10:00:00+00:00"),
        _event("2026-07-08T10:00:10+00:00"),  # 10s total - below min_session_seconds
    ]
    sessions = sessionize(events, gap_minutes=10, min_session_seconds=60)
    assert sessions == []


def test_empty_input():
    assert sessionize([]) == []
