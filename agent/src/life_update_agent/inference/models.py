"""Curated model choices for session clustering/summarization.

This registry is the single source of truth for "what models can the user
pick" - both the CLI (`life-update-agent model list/choose`) and, later,
the Tauri settings dropdown read from it, so the choice only needs to be
made in one place.
"""

from __future__ import annotations

from dataclasses import dataclass

DEFAULT_MODEL = "phi3:mini"


@dataclass(frozen=True)
class ModelChoice:
    name: str
    size_human: str
    description: str


MODEL_CHOICES: list[ModelChoice] = [
    ModelChoice("qwen2.5:0.5b", "398 MB", "fastest, lowest quality"),
    ModelChoice("qwen2.5:1.5b", "986 MB", "good balance for low-end machines"),
    ModelChoice("llama3.2:1b", "1.3 GB", "good balance"),
    ModelChoice("phi3:mini", "2.2 GB", "recommended default"),
    ModelChoice("llama3.2:3b", "2.0 GB", "higher quality, slower"),
]

MODEL_NAMES = {m.name for m in MODEL_CHOICES}


def find_model(name: str) -> ModelChoice | None:
    return next((m for m in MODEL_CHOICES if m.name == name), None)
