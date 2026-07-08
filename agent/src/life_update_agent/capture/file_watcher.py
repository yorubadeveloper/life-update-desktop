"""Layer 1 - file saves and git commits, via watchdog.

Watches the configured project directories. Directory/file names in
`IGNORED_DIR_NAMES` are skipped entirely (dependency/build noise). A change
to `<repo>/.git/logs/HEAD` is treated as a commit and resolved to its
subject line via `git log`, rather than logged as a plain file write.
"""

from __future__ import annotations

import logging
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from watchdog.events import FileSystemEvent, FileSystemEventHandler
from watchdog.observers import Observer

from life_update_agent import db
from life_update_agent.redaction.scanner import scan

logger = logging.getLogger(__name__)

IGNORED_DIR_NAMES = {
    "node_modules", ".git", "__pycache__", ".venv", "venv", "dist", "build",
    ".next", ".idea", ".vscode", ".pytest_cache", "target", ".turbo",
}


def _is_ignored(path: Path) -> bool:
    return any(part in IGNORED_DIR_NAMES for part in path.parts)


def _is_git_head_log(path: Path) -> bool:
    return path.name == "HEAD" and path.parent.name == "logs" and path.parent.parent.name == ".git"


def _latest_commit_subject(repo_root: Path) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(repo_root), "log", "-1", "--pretty=%s"],
            capture_output=True, text=True, timeout=5, check=True,
        )
        return result.stdout.strip() or None
    except Exception:
        logger.warning("failed to read latest commit subject for %s", repo_root, exc_info=True)
        return None


def _record(kind: str, file_path: str | None, extra: str | None = None) -> None:
    with db.get_conn() as conn:
        conn.execute(
            "INSERT INTO raw_events (ts, kind, file_path, extra_json) VALUES (?, ?, ?, ?)",
            (datetime.now(timezone.utc).isoformat(), kind, scan(file_path), scan(extra)),
        )


class _Handler(FileSystemEventHandler):
    def on_modified(self, event: FileSystemEvent) -> None:
        self._handle(event)

    def on_created(self, event: FileSystemEvent) -> None:
        self._handle(event)

    def _handle(self, event: FileSystemEvent) -> None:
        if event.is_directory:
            return

        path = Path(str(event.src_path))
        if _is_git_head_log(path):
            repo_root = path.parent.parent.parent
            subject = _latest_commit_subject(repo_root)
            if subject:
                _record("git_commit", str(repo_root), subject)
            return

        if _is_ignored(path) or path.name.startswith("."):
            return

        _record("file", str(path))


def run(watch_dirs: list[str], stop_event) -> None:
    observer = Observer()
    handler = _Handler()
    watched_any = False

    for directory in watch_dirs:
        p = Path(directory).expanduser()
        if not p.is_dir():
            logger.warning("watch dir does not exist, skipping: %s", p)
            continue
        observer.schedule(handler, str(p), recursive=True)
        watched_any = True

    if not watched_any:
        logger.warning("no valid watch directories configured; file watcher idle")
        stop_event.wait()
        return

    observer.start()
    try:
        stop_event.wait()
    finally:
        observer.stop()
        observer.join(timeout=5)
