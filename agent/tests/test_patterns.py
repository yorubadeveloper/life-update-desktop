from life_update_agent.redaction.patterns import redact_known_patterns


def test_aws_key_redacted():
    text = "export AWS_ACCESS_KEY_ID=AKIAABCDEFGHIJKLMNOP"
    result = redact_known_patterns(text)
    assert "AKIAABCDEFGHIJKLMNOP" not in result
    assert "[REDACTED]" in result


def test_github_token_redacted():
    text = "token: ghp_" + "a" * 36
    result = redact_known_patterns(text)
    assert "ghp_" not in result


def test_openai_key_redacted():
    text = "OPENAI_API_KEY=sk-" + "x" * 30
    assert "sk-" not in redact_known_patterns(text)


def test_email_redacted():
    assert "[REDACTED]" in redact_known_patterns("contact me at jane.doe@example.com please")
    assert "jane.doe@example.com" not in redact_known_patterns("contact me at jane.doe@example.com please")


def test_jwt_redacted():
    jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
    result = redact_known_patterns(f"Authorization: {jwt}")
    assert jwt not in result


def test_valid_credit_card_redacted():
    # 4111 1111 1111 1111 is a well-known Luhn-valid test Visa number
    text = "card on file: 4111 1111 1111 1111"
    result = redact_known_patterns(text)
    assert "4111 1111 1111 1111" not in result
    assert "[REDACTED]" in result


def test_random_long_number_not_treated_as_card():
    # Fails Luhn, should be left alone (e.g. an order id or timestamp)
    text = "order id 1234567890123456"
    assert redact_known_patterns(text) == text


def test_plain_text_untouched():
    text = "refactored the auth middleware and added tests"
    assert redact_known_patterns(text) == text
