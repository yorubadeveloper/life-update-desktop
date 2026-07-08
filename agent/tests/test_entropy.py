from life_update_agent.redaction.entropy import redact_high_entropy_tokens, shannon_entropy


def test_low_entropy_word_untouched():
    assert redact_high_entropy_tokens("this is a normal english sentence") == \
        "this is a normal english sentence"


def test_repeated_char_low_entropy_untouched():
    token = "a" * 30
    assert redact_high_entropy_tokens(token) == token


def test_high_entropy_token_redacted():
    token = "aZ3kQ9mN2pR7vL1xT8wU4yB6"  # random-looking, 24 chars
    result = redact_high_entropy_tokens(f"key={token}")
    assert token not in result
    assert "[REDACTED]" in result


def test_short_tokens_ignored_regardless_of_entropy():
    assert redact_high_entropy_tokens("aZ3kQ9") == "aZ3kQ9"


def test_shannon_entropy_uniform_higher_than_repeated():
    assert shannon_entropy("aZ3kQ9mN2pR7vL1xT8wU4yB6") > shannon_entropy("a" * 24)
