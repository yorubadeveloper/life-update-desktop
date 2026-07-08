"""Layer 5 - HTTP client for the life-update.com portfolio-events API.

Matches the contract in life-update.com's app/api/portfolio-events/route.ts
exactly: bearer auth, camelCase JSON body, idempotent upsert by `id` on the
server side.
"""

from __future__ import annotations

import logging

import httpx

logger = logging.getLogger(__name__)


def post_portfolio_event(
    api_url: str,
    token: str,
    *,
    id: str,
    started_at: str,
    ended_at: str,
    project: str,
    category: str,
    focus_score: float,
    apps_used: list[str],
    summary: str,
    timeout: float = 15.0,
) -> bool:
    """Returns True on a 200 (or a 200-equivalent already-synced state)."""
    url = f"{api_url.rstrip('/')}/api/portfolio-events"
    body = {
        "id": id,
        "startedAt": started_at,
        "endedAt": ended_at,
        "project": project,
        "category": category,
        "focusScore": focus_score,
        "appsUsed": apps_used,
        "summary": summary,
    }

    try:
        response = httpx.post(
            url,
            json=body,
            headers={"Authorization": f"Bearer {token}"},
            timeout=timeout,
        )
    except httpx.HTTPError:
        logger.warning("sync request failed (network), will retry next cycle", exc_info=True)
        return False

    if response.status_code == 200:
        return True

    logger.warning("sync rejected by server: %s %s", response.status_code, response.text[:200])
    return False
