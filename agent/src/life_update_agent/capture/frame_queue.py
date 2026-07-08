"""Bounded in-memory holding area for screenshots awaiting a vision-model
description. Never touches disk. Capped so a long uninterrupted work
session can't accumulate frames without limit - oldest is dropped first.
"""

from __future__ import annotations

import threading
from collections import deque
from dataclasses import dataclass

DEFAULT_MAXLEN = 20


@dataclass
class PendingFrame:
    png_bytes: bytes
    app_name: str | None
    title: str | None
    ts: str


class FrameQueue:
    def __init__(self, maxlen: int = DEFAULT_MAXLEN) -> None:
        self._deque: deque[PendingFrame] = deque(maxlen=maxlen)
        self._lock = threading.Lock()

    def push(self, frame: PendingFrame) -> None:
        with self._lock:
            self._deque.append(frame)

    def drain(self) -> list[PendingFrame]:
        with self._lock:
            items = list(self._deque)
            self._deque.clear()
        return items

    def __len__(self) -> int:
        with self._lock:
            return len(self._deque)
