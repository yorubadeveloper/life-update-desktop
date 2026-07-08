"""Layer 2 - deterministic regex recognizers for known secret shapes.

Mirrors the shapes Presidio's built-in regex recognizers cover, so the fast
inline pass here and the contextual Layer 4 pass agree on what a "secret
shape" looks like.
"""

from __future__ import annotations

import re

# Order matters only in that longer/more specific patterns should come before
# looser ones so a token isn't partially matched by a broader pattern first.
_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("aws_access_key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("github_token", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b")),
    ("openai_key", re.compile(r"\bsk-[A-Za-z0-9]{20,}\b")),
    ("jwt", re.compile(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b")),
    ("private_key_block", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----")),
    ("bearer_token", re.compile(r"\bBearer\s+[A-Za-z0-9\-._~+/]+=*", re.IGNORECASE)),
    ("email", re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")),
    # Candidate 13-19 digit sequences (with optional separators) - validated
    # with Luhn below rather than matched as a bare digit-count pattern to
    # avoid flagging every long number (timestamps, ports, PIDs).
    ("credit_card", re.compile(r"\b(?:\d[ -]?){13,19}\b")),
]


def _luhn_valid(digits: str) -> bool:
    total = 0
    for i, ch in enumerate(reversed(digits)):
        d = int(ch)
        if i % 2 == 1:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    return total % 10 == 0


def redact_known_patterns(text: str) -> str:
    """Replace any substring matching a known secret shape with `[REDACTED]`."""
    if not text:
        return text

    result = text
    for name, pattern in _PATTERNS:
        if name == "credit_card":

            def _replace_if_luhn(m: re.Match[str]) -> str:
                digits = re.sub(r"[ -]", "", m.group(0))
                if len(digits) >= 13 and _luhn_valid(digits):
                    return "[REDACTED]"
                return m.group(0)

            result = pattern.sub(_replace_if_luhn, result)
        else:
            result = pattern.sub("[REDACTED]", result)
    return result
