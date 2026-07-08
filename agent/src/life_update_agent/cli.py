from __future__ import annotations

import argparse
import json
import logging
import signal
import threading

from life_update_agent import daemon, db, worker
from life_update_agent.config import load_settings, save_exclude_list, save_selected_model
from life_update_agent.inference.models import MODEL_CHOICES, MODEL_NAMES
from life_update_agent.inference.ollama_client import (
    OllamaError,
    ensure_model_available,
    list_local_models,
)


def _cmd_run(_args: argparse.Namespace) -> None:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    settings = load_settings()

    if not settings.life_update_token:
        logging.warning(
            "LIFE_UPDATE_TOKEN is not set - capture and redaction will still run, "
            "but nothing will sync to life-update.com until it's configured in .env"
        )

    try:
        ensure_model_available(settings.ollama_host, settings.ollama_model, _print_pull_progress)
    except OllamaError as e:
        logging.warning("could not verify/pull %s: %s (will retry inside the worker loop)", settings.ollama_model, e)

    stop_event = threading.Event()
    signal.signal(signal.SIGINT, lambda *_: stop_event.set())
    signal.signal(signal.SIGTERM, lambda *_: stop_event.set())

    daemon.run(settings, stop_event)
    worker_thread = threading.Thread(
        target=worker.run, args=(settings, stop_event), name="inference-worker", daemon=True
    )
    worker_thread.start()

    logging.info("life-update-agent running - press Ctrl+C to stop")
    stop_event.wait()
    logging.info("shutting down")


def _cmd_status(args: argparse.Namespace) -> None:
    from life_update_agent import db

    with db.get_conn() as conn:
        unprocessed = conn.execute(
            "SELECT COUNT(*) AS c FROM raw_events WHERE processed_at IS NULL"
        ).fetchone()["c"]
        total_captured = conn.execute("SELECT COUNT(*) AS c FROM raw_events").fetchone()["c"]
        unsent = conn.execute(
            "SELECT COUNT(*) AS c FROM portfolio_event_queue WHERE sent_at IS NULL"
        ).fetchone()["c"]
        total_synced = conn.execute(
            "SELECT COUNT(*) AS c FROM portfolio_event_queue WHERE sent_at IS NOT NULL"
        ).fetchone()["c"]
        last_sync_at = conn.execute(
            "SELECT MAX(sent_at) AS t FROM portfolio_event_queue"
        ).fetchone()["t"]

    status = {
        "unprocessed_raw_events": unprocessed,
        "total_captured_events": total_captured,
        "unsent_portfolio_events": unsent,
        "total_synced_portfolio_events": total_synced,
        "last_sync_at": last_sync_at,
    }

    if args.json:
        print(json.dumps(status))
    else:
        for key, value in status.items():
            print(f"{key}: {value}")


def _cmd_exclude_list(_args: argparse.Namespace) -> None:
    settings = load_settings()
    print("Excluded apps:")
    for app in settings.exclude_list.apps:
        print(f"  - {app}")
    print("Excluded title patterns:")
    for pattern in settings.exclude_list.title_patterns:
        print(f"  - {pattern}")


def _cmd_exclude_add(args: argparse.Namespace) -> None:
    settings = load_settings()
    exclude_list = settings.exclude_list
    if args.app:
        exclude_list.apps.append(args.app)
    if args.title_pattern:
        exclude_list.title_patterns.append(args.title_pattern)
    save_exclude_list(exclude_list)
    print("Updated.")


def _cmd_exclude_remove(args: argparse.Namespace) -> None:
    settings = load_settings()
    exclude_list = settings.exclude_list
    exclude_list.apps = [a for a in exclude_list.apps if a != args.app] if args.app else exclude_list.apps
    exclude_list.title_patterns = (
        [p for p in exclude_list.title_patterns if p != args.title_pattern]
        if args.title_pattern
        else exclude_list.title_patterns
    )
    save_exclude_list(exclude_list)
    print("Updated.")


def _print_pull_progress(status: str, completed: int | None, total: int | None) -> None:
    if total:
        pct = 100 * (completed or 0) / total
        print(f"\r{status}: {pct:.0f}% ({(completed or 0) // (1024*1024)}MB/{total // (1024*1024)}MB)", end="", flush=True)
    else:
        print(f"\r{status}" + " " * 20, end="", flush=True)
    if status in ("success",):
        print()


def _cmd_model_list(args: argparse.Namespace) -> None:
    settings = load_settings()
    try:
        local_models = list_local_models(settings.ollama_host)
    except OllamaError:
        local_models = None  # Ollama unreachable - still show the registry

    if args.json:
        print(json.dumps([
            {
                "name": m.name,
                "size": m.size_human,
                "description": m.description,
                "selected": m.name == settings.ollama_model,
                "downloaded": None if local_models is None else m.name in local_models,
            }
            for m in MODEL_CHOICES
        ]))
        return

    if local_models is None:
        print("(could not reach ollama - showing registry only, not download status)")

    for m in MODEL_CHOICES:
        marker = "*" if m.name == settings.ollama_model else " "
        downloaded = "" if local_models is None else (" [downloaded]" if m.name in local_models else "")
        print(f"{marker} {m.name:<16} {m.size_human:>8}   {m.description}{downloaded}")


def _cmd_model_choose(args: argparse.Namespace) -> None:
    if args.name not in MODEL_NAMES:
        valid = ", ".join(sorted(MODEL_NAMES))
        raise SystemExit(f"unknown model {args.name!r} - choose one of: {valid}")

    settings = load_settings()
    try:
        ensure_model_available(settings.ollama_host, args.name, _print_pull_progress)
    except OllamaError as e:
        raise SystemExit(f"failed to pull {args.name}: {e}") from e

    save_selected_model(args.name)
    print(f"switched to {args.name}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="life-update-agent")
    sub = parser.add_subparsers(dest="command", required=True)

    run_parser = sub.add_parser("run", help="run the capture daemon and inference worker in the foreground")
    run_parser.set_defaults(func=_cmd_run)

    status_parser = sub.add_parser("status", help="show local capture/sync counts")
    status_parser.add_argument("--json", action="store_true", help="machine-readable output")
    status_parser.set_defaults(func=_cmd_status)

    exclude_parser = sub.add_parser("exclude", help="manage the exclude-list")
    exclude_sub = exclude_parser.add_subparsers(dest="exclude_command", required=True)

    list_parser = exclude_sub.add_parser("list", help="show the current exclude-list")
    list_parser.set_defaults(func=_cmd_exclude_list)

    add_parser = exclude_sub.add_parser("add", help="add an app name and/or title regex to the exclude-list")
    add_parser.add_argument("--app", help="app name substring to exclude (case-insensitive)")
    add_parser.add_argument("--title-pattern", help="regex to match against window titles")
    add_parser.set_defaults(func=_cmd_exclude_add)

    remove_parser = exclude_sub.add_parser("remove", help="remove an entry from the exclude-list")
    remove_parser.add_argument("--app")
    remove_parser.add_argument("--title-pattern")
    remove_parser.set_defaults(func=_cmd_exclude_remove)

    model_parser = sub.add_parser("model", help="manage the local LLM used for session summarization")
    model_sub = model_parser.add_subparsers(dest="model_command", required=True)

    model_list_parser = model_sub.add_parser("list", help="show the curated model choices")
    model_list_parser.add_argument("--json", action="store_true", help="machine-readable output")
    model_list_parser.set_defaults(func=_cmd_model_list)

    model_choose_parser = model_sub.add_parser("choose", help="select a model, pulling it via Ollama if needed")
    model_choose_parser.add_argument("name")
    model_choose_parser.set_defaults(func=_cmd_model_choose)

    return parser


def main() -> None:
    db.init_db()
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)
