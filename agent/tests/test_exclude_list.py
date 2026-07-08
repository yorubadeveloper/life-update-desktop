from life_update_agent.capture.exclude_list import is_excluded
from life_update_agent.config import ExcludeList

EXCLUDE = ExcludeList(
    apps=["1Password", "Bitwarden"],
    title_patterns=[r"(?i)\bbank\b", r"(?i)\bpassword\b"],
)


def test_excluded_app_by_substring_case_insensitive():
    assert is_excluded(EXCLUDE, "1password 8", "Vault") is True


def test_excluded_title_pattern():
    assert is_excluded(EXCLUDE, "Chrome", "Chase Bank - Sign In") is True


def test_unrelated_app_and_title_not_excluded():
    assert is_excluded(EXCLUDE, "Visual Studio Code", "main.py - life-update-agent") is False


def test_none_values_do_not_crash():
    assert is_excluded(EXCLUDE, None, None) is False
