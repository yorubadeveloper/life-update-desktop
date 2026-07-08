"""Curated engine choices for turning a screenshot into text/description.

Mirrors `inference/models.py`'s pattern exactly. "tesseract" is a special
case, not an Ollama tag: it's a bundled local binary, runs inline in the
capture loop, and never needs pulling. The other two are Ollama vision
models, pulled the same way session-summarization models are.

qwen2.5vl was chosen over moondream/llava for the vision options: moondream
is explicitly weak on dense text, and our content (code, docs, terminals)
is exactly dense text - qwen2.5vl is specifically strong at structured,
text-heavy screenshots.
"""

from __future__ import annotations

from dataclasses import dataclass

DEFAULT_VISION_ENGINE = "tesseract"

TESSERACT_ENGINE = "tesseract"


@dataclass(frozen=True)
class VisionChoice:
    name: str
    size_human: str
    description: str


VISION_CHOICES: list[VisionChoice] = [
    VisionChoice(TESSERACT_ENGINE, "~35 MB", "fast, text-only, runs inline (recommended default)"),
    VisionChoice("qwen2.5vl:3b", "3.2 GB", "reads screen content semantically, runs when idle"),
    VisionChoice("qwen2.5vl:7b", "6.0 GB", "higher quality, slower"),
]

VISION_ENGINE_NAMES = {v.name for v in VISION_CHOICES}


def find_vision_engine(name: str) -> VisionChoice | None:
    return next((v for v in VISION_CHOICES if v.name == name), None)


def is_ollama_backed(name: str) -> bool:
    return name != TESSERACT_ENGINE
