"""Layer 2 orchestrator - runs inline, before anything touches local storage.

Applies the regex secret-shape pass first, then the entropy pass on
whatever's left, and returns the scrubbed string. Callers pass every
free-text field of a raw event through `scan()` before it's persisted.
"""

from __future__ import annotations

from life_update_agent.redaction.entropy import redact_high_entropy_tokens
from life_update_agent.redaction.patterns import redact_known_patterns


def scan(text: str | None) -> str | None:
    if text is None:
        return None
    return redact_high_entropy_tokens(redact_known_patterns(text))
