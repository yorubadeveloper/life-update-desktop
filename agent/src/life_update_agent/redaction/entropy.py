"""Shannon entropy check for high-randomness strings that don't match a
known secret shape (e.g. a home-grown token format, a random session id)."""

from __future__ import annotations

import math
import re
from collections import Counter

TOKEN_PATTERN = re.compile(r"[A-Za-z0-9_\-+/=]{20,}")

MIN_ENTROPY_BITS_PER_CHAR = 4.0


def shannon_entropy(s: str) -> float:
    if not s:
        return 0.0
    counts = Counter(s)
    length = len(s)
    return -sum((n / length) * math.log2(n / length) for n in counts.values())


def redact_high_entropy_tokens(text: str, min_entropy: float = MIN_ENTROPY_BITS_PER_CHAR) -> str:
    """Replace long tokens whose entropy exceeds the threshold with `[REDACTED]`."""
    if not text:
        return text

    def _replace(m: re.Match[str]) -> str:
        token = m.group(0)
        if shannon_entropy(token) >= min_entropy:
            return "[REDACTED]"
        return token

    return TOKEN_PATTERN.sub(_replace, text)
