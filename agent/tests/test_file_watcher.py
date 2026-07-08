from pathlib import Path

from life_update_agent.capture.file_watcher import _is_git_head_log, _is_ignored


def test_ignores_node_modules():
    assert _is_ignored(Path("/repo/node_modules/pkg/index.js")) is True


def test_ignores_venv():
    assert _is_ignored(Path("/repo/.venv/lib/site-packages/x.py")) is True


def test_does_not_ignore_regular_source_file():
    assert _is_ignored(Path("/repo/src/main.py")) is False


def test_detects_git_head_log():
    assert _is_git_head_log(Path("/repo/.git/logs/HEAD")) is True


def test_does_not_confuse_other_head_files():
    assert _is_git_head_log(Path("/repo/.git/HEAD")) is False
    assert _is_git_head_log(Path("/repo/src/HEAD")) is False
