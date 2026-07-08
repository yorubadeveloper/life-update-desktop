"""Layer 4 orchestrator - runs only when the machine is idle and under low
load. Pulls unprocessed raw_events, sessionizes them, summarizes each
session via Presidio + phi3:mini, writes the result to the sync queue, then
hands off to Layer 5 to push anything queued.
"""

from __future__ import annotations

import base64
import json
import logging
import threading
import uuid
from datetime import datetime, timezone

from life_update_agent import db
from life_update_agent.capture.frame_queue import FrameQueue
from life_update_agent.capture.idle_monitor import is_safe_to_run_inference
from life_update_agent.config import Settings
from life_update_agent.inference.cluster import sessionize
from life_update_agent.inference.ollama_client import OllamaError, describe_image
from life_update_agent.inference.summarize import summarize_session
from life_update_agent.redaction.scanner import scan
from life_update_agent.sync.queue import sync_pending

logger = logging.getLogger(__name__)

CHECK_INTERVAL_SECONDS = 60.0
DEVICE_ID_NAMESPACE = uuid.UUID("6f7f9e2c-6c1a-4c2b-9a1e-3d6b6c2f9a11")

VISION_DESCRIBE_PROMPT = (
    "Describe what is being worked on in this screenshot in one or two plain "
    "sentences. Focus on the task or problem, not the UI chrome."
)


def _fetch_unprocessed_events() -> list[dict]:
    with db.get_conn() as conn:
        rows = conn.execute(
            "SELECT * FROM raw_events WHERE processed_at IS NULL ORDER BY ts ASC"
        ).fetchall()
    return [dict(r) for r in rows]


def _mark_processed(event_ids: list[int]) -> None:
    if not event_ids:
        return
    with db.get_conn() as conn:
        conn.executemany(
            "UPDATE raw_events SET processed_at = ? WHERE id = ?",
            [(datetime.now(timezone.utc).isoformat(), eid) for eid in event_ids],
        )


def _enqueue_portfolio_event(event: dict) -> None:
    event_id = str(uuid.uuid5(DEVICE_ID_NAMESPACE, f"{event['started_at']}:{event['ended_at']}"))
    with db.get_conn() as conn:
        conn.execute(
            """INSERT INTO portfolio_event_queue
               (id, started_at, ended_at, project, category, focus_score, apps_used_json, summary, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(id) DO NOTHING""",
            (
                event_id,
                event["started_at"],
                event["ended_at"],
                event["project"],
                event["category"],
                event["focus_score"],
                json.dumps(event["apps_used"]),
                event["summary"],
                datetime.now(timezone.utc).isoformat(),
            ),
        )


def _record_screen_text(ts: str, app_name: str | None, title: str | None, text: str) -> None:
    with db.get_conn() as conn:
        conn.execute(
            "INSERT INTO raw_events (ts, kind, app_name, window_title, extra_json) VALUES (?, ?, ?, ?, ?)",
            (ts, "screen_text", scan(app_name), scan(title), scan(text)),
        )


def process_pending_frames(settings: Settings, frame_queue: FrameQueue) -> int:
    """Drains screenshots queued by screen_watcher.py for the vision-model
    path (too slow to OCR inline - see screen_watcher.py). Each frame's
    image is discarded immediately after the description comes back,
    whether that succeeds or fails."""
    frames = frame_queue.drain()
    described = 0

    for frame in frames:
        try:
            image_b64 = base64.b64encode(frame.png_bytes).decode()
            description = describe_image(settings.ollama_host, settings.vision_engine, image_b64, VISION_DESCRIBE_PROMPT)
        except OllamaError:
            logger.warning("vision description failed for a queued frame, discarding", exc_info=True)
            continue

        if description:
            _record_screen_text(frame.ts, frame.app_name, frame.title, description)
            described += 1

    return described


def run_once(settings: Settings) -> int:
    """Runs a single inference pass. Returns the number of sessions processed.

    The worker only runs once the machine has been idle past the threshold,
    so every event fetched here belongs to a *completed* period of activity
    - nothing is still "in progress". That means sessions `sessionize()`
    drops as too short/noisy can be marked processed immediately (they're
    genuinely low-signal, not incomplete); only a live Ollama failure should
    leave events unprocessed for a retry next cycle.
    """
    events = _fetch_unprocessed_events()
    if not events:
        return 0

    sessions = sessionize(events)
    attempted_ids = {e["id"] for session in sessions for e in session}
    noise_ids = [e["id"] for e in events if e["id"] not in attempted_ids]
    _mark_processed(noise_ids)

    processed_count = 0

    for session in sessions:
        result = summarize_session(
            session, settings.ollama_host, settings.ollama_model, settings.presidio_spacy_model
        )
        if result is None:
            logger.warning("skipping session (ollama unavailable or bad output), will retry next cycle")
            continue

        _enqueue_portfolio_event(result)
        _mark_processed([e["id"] for e in session])
        processed_count += 1

    if processed_count:
        sync_pending(settings)

    return processed_count


def run(settings: Settings, stop_event: threading.Event, frame_queue: FrameQueue | None = None) -> None:
    while not stop_event.is_set():
        if is_safe_to_run_inference(settings.idle_threshold_minutes, settings.cpu_load_ceiling_percent):
            if frame_queue is not None and len(frame_queue):
                try:
                    process_pending_frames(settings, frame_queue)
                except Exception:
                    logger.error("processing pending vision frames failed", exc_info=True)

            try:
                count = run_once(settings)
                if count:
                    logger.info("inference pass processed %d session(s)", count)
            except Exception:
                logger.error("inference pass failed", exc_info=True)

        stop_event.wait(CHECK_INTERVAL_SECONDS)
