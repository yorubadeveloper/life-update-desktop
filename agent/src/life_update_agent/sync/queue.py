"""Layer 5 - reads unsent rows from portfolio_event_queue, POSTs them, and
marks each sent only after a 200. Offline periods just catch up next run:
unsent rows are retried every time this is called."""

from __future__ import annotations

import json
import logging
from datetime import datetime, timezone

from life_update_agent import db
from life_update_agent.config import Settings
from life_update_agent.sync.client import post_portfolio_event

logger = logging.getLogger(__name__)


def sync_pending(settings: Settings) -> int:
    if not settings.life_update_token:
        logger.warning("no LIFE_UPDATE_TOKEN configured, skipping sync")
        return 0

    with db.get_conn() as conn:
        rows = conn.execute(
            "SELECT * FROM portfolio_event_queue WHERE sent_at IS NULL ORDER BY created_at ASC"
        ).fetchall()

    sent_count = 0
    for row in rows:
        ok = post_portfolio_event(
            settings.life_update_api_url,
            settings.life_update_token,
            id=row["id"],
            started_at=row["started_at"],
            ended_at=row["ended_at"],
            project=row["project"],
            category=row["category"],
            focus_score=row["focus_score"],
            apps_used=json.loads(row["apps_used_json"]),
            summary=row["summary"],
        )
        if ok:
            with db.get_conn() as conn:
                conn.execute(
                    "UPDATE portfolio_event_queue SET sent_at = ? WHERE id = ?",
                    (datetime.now(timezone.utc).isoformat(), row["id"]),
                )
            sent_count += 1

    return sent_count
