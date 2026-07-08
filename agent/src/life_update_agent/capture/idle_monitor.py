"""Idle detection and CPU load monitoring - gates when the inference worker
is allowed to run. A single global listener is started once per process;
`is_safe_to_run_inference` is the only thing callers need."""

from __future__ import annotations

import threading
import time

import psutil
from pynput import keyboard, mouse

_lock = threading.Lock()
_last_input_ts = time.monotonic()
_listeners_started = False


def _touch(*_args, **_kwargs) -> None:
    global _last_input_ts
    with _lock:
        _last_input_ts = time.monotonic()


def start() -> None:
    """Start the global keyboard/mouse listeners exactly once."""
    global _listeners_started
    if _listeners_started:
        return
    keyboard.Listener(on_press=_touch).start()
    mouse.Listener(on_move=_touch, on_click=_touch, on_scroll=_touch).start()
    _listeners_started = True


def seconds_since_last_input() -> float:
    with _lock:
        return time.monotonic() - _last_input_ts


def is_user_idle(threshold_minutes: float) -> bool:
    return seconds_since_last_input() >= threshold_minutes * 60


def is_load_low(ceiling_percent: float, interval: float = 0.5) -> bool:
    return psutil.cpu_percent(interval=interval) <= ceiling_percent


def is_safe_to_run_inference(idle_threshold_minutes: float, cpu_load_ceiling_percent: float) -> bool:
    return is_user_idle(idle_threshold_minutes) and is_load_low(cpu_load_ceiling_percent)
