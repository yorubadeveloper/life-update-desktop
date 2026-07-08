"""Layer 1 - screen content watching (opt-in, off by default).

Hybrid cadence: a periodic timer, reset immediately whenever the active
window/app changes, so switching context always gets a fresh capture
rather than a stale one from minutes ago.

Two processing paths depending on the configured vision engine:
- Tesseract (default): fast enough to OCR inline, right in this loop.
  Screenshot -> OCR -> image discarded -> text scanned (Layer 2) ->
  persisted. Same shape as window_tracker.py.
- An Ollama vision model: too slow to run inline (multi-second calls), so
  the frame is pushed to a bounded in-memory FrameQueue instead and
  processed later by the idle-gated worker (worker.py). This means
  vision-model descriptions lag behind capture during a long active
  session - a real, accepted trade-off of that engine choice.

Exclude-list is checked before any screenshot is taken, matching every
other capture source.
"""

from __future__ import annotations

import io
import logging
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import mss
import pytesseract
import pywinctl
from PIL import Image

from life_update_agent import db
from life_update_agent.capture.exclude_list import is_excluded
from life_update_agent.capture.frame_queue import FrameQueue, PendingFrame
from life_update_agent.capture.window_tracker import read_active_window
from life_update_agent.config import ExcludeList
from life_update_agent.inference.vision_models import is_ollama_backed
from life_update_agent.redaction.scanner import scan

logger = logging.getLogger(__name__)

POLL_INTERVAL_SECONDS = 2.0


def _configure_bundled_tesseract() -> None:
    """If running as a frozen build with tesseract-runtime/ bundled as a
    sibling resource (see scripts/bundle-tesseract.sh), point pytesseract
    at it instead of relying on a system install. No-op in dev - falls
    back to pytesseract's default PATH lookup for `tesseract`."""
    if not getattr(sys, "frozen", False):
        return

    # Mirrors the sibling-resource layout app-shell/scripts/prepare-resources.sh
    # stages: Contents/Resources/{life-update-agent,ollama-runtime,tesseract-runtime}/
    resources_dir = Path(sys.executable).parent.parent
    bundled = resources_dir / "tesseract-runtime"
    binary = bundled / "tesseract"

    if binary.exists():
        pytesseract.pytesseract.tesseract_cmd = str(binary)
        os.environ["TESSDATA_PREFIX"] = str(bundled / "tessdata")
        logger.info("using bundled tesseract at %s", binary)
    else:
        logger.warning("frozen build but no bundled tesseract-runtime found, falling back to PATH")


_configure_bundled_tesseract()


def should_capture(
    window_key: tuple[str | None, str | None],
    last_window_key: tuple[str | None, str | None] | None,
    now: float,
    last_capture_at: float,
    interval_seconds: float,
) -> bool:
    """The hybrid cadence: capture when the window/app has changed, or
    when the periodic interval has elapsed - whichever comes first."""
    window_changed = window_key != last_window_key
    interval_elapsed = (now - last_capture_at) >= interval_seconds
    return window_changed or interval_elapsed


def _has_screen_recording_permission() -> bool:
    """Checks Screen Recording permission, actively prompting for it if
    missing. CGPreflightScreenCaptureAccess() alone only checks status -
    without also calling CGRequestScreenCaptureAccess(), a user who never
    granted it would just see a silent log warning with no system prompt
    telling them why, since we never actually attempt a capture."""
    try:
        import Quartz

        if Quartz.CGPreflightScreenCaptureAccess():
            return True
        Quartz.CGRequestScreenCaptureAccess()
        return False
    except Exception:
        # Non-macOS, or PyObjC unavailable - let the actual capture attempt
        # surface the real error instead of guessing.
        return True


def _active_window_box() -> tuple[int, int, int, int] | None:
    try:
        window = pywinctl.getActiveWindow()
    except Exception:
        return None
    if window is None:
        return None
    try:
        box = window.box
        if box.width > 0 and box.height > 0:
            return (box.left, box.top, box.width, box.height)
    except Exception:
        pass
    return None


def _capture_screenshot() -> Image.Image | None:
    """Grabs the active window's region, falling back to the primary
    monitor if the window bounds are unavailable/invalid (verified this
    happens in practice, not just theoretically)."""
    try:
        with mss.MSS() as sct:
            box = _active_window_box()
            if box:
                left, top, width, height = box
                region = {"left": left, "top": top, "width": width, "height": height}
            else:
                region = sct.monitors[1]

            shot = sct.grab(region)
            return Image.frombytes("RGB", shot.size, shot.bgra, "raw", "BGRX")
    except Exception:
        logger.warning("failed to capture screenshot", exc_info=True)
        return None


def _ocr_text(image: Image.Image) -> str | None:
    try:
        text = pytesseract.image_to_string(image)
        return text.strip() or None
    except Exception:
        logger.warning("tesseract OCR failed", exc_info=True)
        return None


def _to_png_bytes(image: Image.Image) -> bytes:
    buf = io.BytesIO()
    image.save(buf, format="PNG")
    return buf.getvalue()


def _record_screen_text(text: str, app_name: str | None, title: str | None) -> None:
    with db.get_conn() as conn:
        conn.execute(
            "INSERT INTO raw_events (ts, kind, app_name, window_title, extra_json) VALUES (?, ?, ?, ?, ?)",
            (
                datetime.now(timezone.utc).isoformat(),
                "screen_text",
                scan(app_name),
                scan(title),
                scan(text),
            ),
        )


def run(
    exclude_list: ExcludeList,
    vision_engine: str,
    interval_seconds: float,
    frame_queue: FrameQueue,
    stop_event,
) -> None:
    if not _has_screen_recording_permission():
        logger.warning(
            "Screen Recording permission not granted - screen watching disabled for this run. "
            "Grant it in System Settings > Privacy & Security > Screen Recording, then restart."
        )
        stop_event.wait()
        return

    last_capture_at = 0.0
    last_window_key: tuple[str | None, str | None] | None = None

    while not stop_event.is_set():
        now = time.monotonic()
        app_name, title = read_active_window()

        window_key = (app_name, title)
        capture_due = should_capture(window_key, last_window_key, now, last_capture_at, interval_seconds)
        last_window_key = window_key

        if capture_due and not is_excluded(exclude_list, app_name, title):
            last_capture_at = now

            image = _capture_screenshot()
            if image is not None:
                if is_ollama_backed(vision_engine):
                    frame_queue.push(
                        PendingFrame(
                            png_bytes=_to_png_bytes(image),
                            app_name=app_name,
                            title=title,
                            ts=datetime.now(timezone.utc).isoformat(),
                        )
                    )
                else:
                    text = _ocr_text(image)
                    if text:
                        _record_screen_text(text, app_name, title)
                image.close()

        stop_event.wait(POLL_INTERVAL_SECONDS)
