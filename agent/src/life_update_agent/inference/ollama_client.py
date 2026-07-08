"""Thin wrapper around a local Ollama server's HTTP API."""

from __future__ import annotations

import json
import logging
from typing import Callable, Optional

import httpx

logger = logging.getLogger(__name__)


class OllamaError(RuntimeError):
    pass


def list_local_models(host: str, timeout: float = 10.0) -> set[str]:
    """Model names already pulled and available locally."""
    try:
        response = httpx.get(f"{host.rstrip('/')}/api/tags", timeout=timeout)
        response.raise_for_status()
    except httpx.HTTPError as e:
        raise OllamaError(f"failed to reach ollama at {host}: {e}") from e

    return {m["name"] for m in response.json().get("models", [])}


# Called with (status, completed_bytes, total_bytes) for each progress event;
# `total_bytes` is None for non-download phases (e.g. "pulling manifest").
ProgressCallback = Callable[[str, Optional[int], Optional[int]], None]


def pull_model(host: str, model: str, on_progress: ProgressCallback | None = None, timeout: float = 1800.0) -> None:
    """Pulls a model, streaming progress events. Raises OllamaError on failure."""
    try:
        with httpx.stream(
            "POST", f"{host.rstrip('/')}/api/pull",
            json={"model": model, "stream": True},
            timeout=timeout,
        ) as response:
            response.raise_for_status()
            for line in response.iter_lines():
                if not line:
                    continue
                event = json.loads(line)
                if event.get("error"):
                    raise OllamaError(f"pulling {model} failed: {event['error']}")
                if on_progress:
                    on_progress(event.get("status", ""), event.get("completed"), event.get("total"))
    except httpx.HTTPError as e:
        raise OllamaError(f"failed to reach ollama at {host}: {e}") from e


def ensure_model_available(
    host: str, model: str, on_progress: ProgressCallback | None = None
) -> None:
    """Pulls `model` if it isn't already present locally. No-op otherwise."""
    if model in list_local_models(host):
        return
    pull_model(host, model, on_progress)


def generate_json(host: str, model: str, prompt: str, timeout: float = 60.0) -> dict:
    """Calls Ollama with `format: json` and returns the parsed object.

    Raises OllamaError if the server is unreachable or returns something
    that isn't valid JSON - callers should treat this as "skip this session,
    try again next cycle" rather than crash the worker.
    """
    try:
        response = httpx.post(
            f"{host.rstrip('/')}/api/generate",
            json={"model": model, "prompt": prompt, "stream": False, "format": "json"},
            timeout=timeout,
        )
        response.raise_for_status()
    except httpx.HTTPError as e:
        raise OllamaError(f"failed to reach ollama at {host}: {e}") from e

    body = response.json()
    raw = body.get("response", "")
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        raise OllamaError(f"ollama returned non-JSON response: {raw!r}") from e
