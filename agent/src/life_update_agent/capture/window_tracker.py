"""Layer 1 - active window/app tracking.

Polls the OS for the active window on an interval and records a row only
when it changes (app or title), so a session spent in one window doesn't
flood the store. Exclude-list is checked before an event is built; whatever
survives is passed through the Layer 2 scanner before being written.
"""

from __future__ import annotations

import logging
import time
from datetime import datetime, timezone

import pywinctl

from life_update_agent import db
from life_update_agent.capture.exclude_list import is_excluded
from life_update_agent.config import ExcludeList
from life_update_agent.redaction.scanner import scan

logger = logging.getLogger(__name__)

POLL_INTERVAL_SECONDS = 2.0


def _read_active_window() -> tuple[str | None, str | None]:
    """Returns (app_name, title), or (None, None) if unavailable (no window
    focused, or the OS denied permission - logged once, never raised)."""
    try:
        window = pywinctl.getActiveWindow()
    except Exception:
        logger.warning("failed to read active window (permissions?)", exc_info=True)
        return None, None

    if window is None:
        return None, None

    try:
        app_name = window.getAppName()
    except Exception:
        app_name = None

    title = getattr(window, "title", None) or None
    return app_name, title


def run(exclude_list: ExcludeList, stop_event) -> None:
    last_seen: tuple[str | None, str | None] | None = None

    while not stop_event.is_set():
        app_name, title = _read_active_window()

        if (app_name, title) != last_seen and (app_name or title):
            last_seen = (app_name, title)

            if not is_excluded(exclude_list, app_name, title):
                with db.get_conn() as conn:
                    conn.execute(
                        "INSERT INTO raw_events (ts, kind, app_name, window_title) VALUES (?, ?, ?, ?)",
                        (
                            datetime.now(timezone.utc).isoformat(),
                            "window",
                            scan(app_name),
                            scan(title),
                        ),
                    )

        stop_event.wait(POLL_INTERVAL_SECONDS)
