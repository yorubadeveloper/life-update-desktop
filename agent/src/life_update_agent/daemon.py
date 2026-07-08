"""Layer 1 orchestrator - runs the capture loops (window tracker, file
watcher, idle monitor) as background threads. All writes go through the
Layer 2 scanner before touching SQLite; that happens inside each capture
module, not here.
"""

from __future__ import annotations

import logging
import threading

from life_update_agent import db
from life_update_agent.capture import file_watcher, idle_monitor, window_tracker
from life_update_agent.config import Settings

logger = logging.getLogger(__name__)


def run(settings: Settings, stop_event: threading.Event) -> list[threading.Thread]:
    db.init_db()
    idle_monitor.start()

    threads = [
        threading.Thread(
            target=window_tracker.run, args=(settings.exclude_list, stop_event),
            name="window-tracker", daemon=True,
        ),
        threading.Thread(
            target=file_watcher.run, args=(settings.watch_dirs, stop_event),
            name="file-watcher", daemon=True,
        ),
    ]
    for t in threads:
        t.start()
        logger.info("started %s", t.name)

    return threads
