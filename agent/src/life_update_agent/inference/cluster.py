"""Layer 4 - groups raw events into candidate sessions by time gap.

Classic session-boundary-on-gap approach: events less than `gap_minutes`
apart belong to the same session. This is the first, cheap pass before
phi3:mini is asked to name the project/category for each session.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any, Mapping, Sequence

DEFAULT_GAP_MINUTES = 10.0
DEFAULT_MIN_SESSION_SECONDS = 60.0


def _parse_ts(ts: str) -> datetime:
    return datetime.fromisoformat(ts)


def sessionize(
    events: Sequence[Mapping[str, Any]],
    gap_minutes: float = DEFAULT_GAP_MINUTES,
    min_session_seconds: float = DEFAULT_MIN_SESSION_SECONDS,
) -> list[list[Mapping[str, Any]]]:
    """Events must be sorted ascending by `ts` (ISO 8601 strings)."""
    if not events:
        return []

    sessions: list[list[Mapping[str, Any]]] = [[events[0]]]
    for event in events[1:]:
        prev_ts = _parse_ts(sessions[-1][-1]["ts"])
        curr_ts = _parse_ts(event["ts"])
        gap_seconds = (curr_ts - prev_ts).total_seconds()

        if gap_seconds > gap_minutes * 60:
            sessions.append([event])
        else:
            sessions[-1].append(event)

    def _duration_seconds(session: list[Mapping[str, Any]]) -> float:
        start = _parse_ts(session[0]["ts"])
        end = _parse_ts(session[-1]["ts"])
        return (end - start).total_seconds()

    return [s for s in sessions if _duration_seconds(s) >= min_session_seconds]
