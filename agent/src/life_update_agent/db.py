"""Layer 3 - local SQLite store.

`raw_events` is append-only and holds only strings that have already passed
through the Layer 2 redaction scanner. `portfolio_event_queue` holds the
distilled events produced by the Layer 4 inference worker, pending sync.
"""

from __future__ import annotations

import sqlite3
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

from life_update_agent.config import DB_PATH

SCHEMA = """
CREATE TABLE IF NOT EXISTS raw_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    kind TEXT NOT NULL,              -- 'window' | 'file' | 'git_commit'
    app_name TEXT,
    window_title TEXT,
    file_path TEXT,
    extra_json TEXT,
    processed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events (ts);
CREATE INDEX IF NOT EXISTS idx_raw_events_processed ON raw_events (processed_at);

CREATE TABLE IF NOT EXISTS portfolio_event_queue (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    ended_at TEXT NOT NULL,
    project TEXT NOT NULL,
    category TEXT NOT NULL,
    focus_score REAL NOT NULL,
    apps_used_json TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL,
    sent_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_queue_unsent ON portfolio_event_queue (sent_at);
"""


def connect() -> sqlite3.Connection:
    Path(DB_PATH).parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    return conn


def init_db() -> None:
    with connect() as conn:
        conn.executescript(SCHEMA)


@contextmanager
def get_conn() -> Iterator[sqlite3.Connection]:
    conn = connect()
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()
