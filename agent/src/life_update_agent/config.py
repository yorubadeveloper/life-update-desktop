"""Settings and local persisted state (exclude-list, selected model).

Settings come from environment variables (loaded from a `.env` file via
python-dotenv). The exclude-list and selected model are user-editable local
state in `~/.life-update-agent/config.json`, seeded on first run from
shipped defaults. `OLLAMA_MODEL` in `.env`, if set, always overrides the
persisted selection - the env var is meant for scripting/CI, the persisted
value is what the CLI's `model choose` (and later, the Tauri dropdown)
writes to.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path

from dotenv import load_dotenv

from life_update_agent.inference.models import DEFAULT_MODEL
from life_update_agent.inference.vision_models import DEFAULT_VISION_ENGINE

load_dotenv()

STATE_DIR = Path(os.environ.get("LIFE_UPDATE_AGENT_HOME", Path.home() / ".life-update-agent"))
DB_PATH = STATE_DIR / "agent.db"
STATE_PATH = STATE_DIR / "config.json"

# Mirrored in config/exclude_list.default.json at the repo root for humans to
# read/edit as a template - the seed logic below embeds it directly so it
# survives packaging (PyInstaller/wheel) without relying on a path relative
# to the source tree.
DEFAULT_EXCLUDE_LIST = {
    "apps": [
        "1Password",
        "Bitwarden",
        "KeePassXC",
        "LastPass",
        "Keychain Access",
    ],
    "title_patterns": [
        r"(?i)\bbank\b",
        r"(?i)\bpaypal\b",
        r"(?i)\bwise\b",
        r"(?i)\bcoinbase\b",
        r"(?i)\brobinhood\b",
        r"(?i)\bvenmo\b",
        r"(?i)\bchase\b",
        r"(?i)\bwells fargo\b",
        r"(?i)\bpassword\b",
        r"(?i)\bincognito\b",
        r"(?i)\bprivate browsing\b",
    ],
}


@dataclass
class ExcludeList:
    apps: list[str] = field(default_factory=list)
    title_patterns: list[str] = field(default_factory=list)


@dataclass
class Settings:
    life_update_token: str
    life_update_api_url: str
    ollama_host: str
    ollama_model: str
    idle_threshold_minutes: float
    cpu_load_ceiling_percent: float
    watch_dirs: list[str]
    presidio_spacy_model: str
    screen_watch_enabled: bool
    screen_capture_interval_seconds: float
    vision_engine: str
    exclude_list: ExcludeList


def _read_state() -> dict:
    if not STATE_PATH.exists():
        STATE_DIR.mkdir(parents=True, exist_ok=True)
        STATE_PATH.write_text(json.dumps({**DEFAULT_EXCLUDE_LIST, "ollama_model": DEFAULT_MODEL}, indent=2))
    return json.loads(STATE_PATH.read_text())


def _write_state(patch: dict) -> None:
    state = _read_state()
    state.update(patch)
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    STATE_PATH.write_text(json.dumps(state, indent=2))


def load_exclude_list() -> ExcludeList:
    state = _read_state()
    return ExcludeList(apps=state.get("apps", []), title_patterns=state.get("title_patterns", []))


def save_exclude_list(exclude_list: ExcludeList) -> None:
    _write_state({"apps": exclude_list.apps, "title_patterns": exclude_list.title_patterns})


def load_selected_model() -> str:
    return _read_state().get("ollama_model", DEFAULT_MODEL)


def save_selected_model(name: str) -> None:
    _write_state({"ollama_model": name})


def load_vision_engine() -> str:
    return _read_state().get("vision_engine", DEFAULT_VISION_ENGINE)


def save_vision_engine(name: str) -> None:
    _write_state({"vision_engine": name})


def load_screen_watch_enabled() -> bool:
    return bool(_read_state().get("screen_watch_enabled", False))


def save_screen_watch_enabled(enabled: bool) -> None:
    _write_state({"screen_watch_enabled": enabled})


def load_screen_capture_interval_seconds() -> float:
    return float(_read_state().get("screen_capture_interval_seconds", 120))


def save_screen_capture_interval_seconds(seconds: float) -> None:
    _write_state({"screen_capture_interval_seconds": seconds})


def load_settings() -> Settings:
    token = os.environ.get("LIFE_UPDATE_TOKEN", "")
    watch_dirs_raw = os.environ.get("WATCH_DIRS", "").strip()
    watch_dirs = [d.strip() for d in watch_dirs_raw.split(",") if d.strip()] or [str(Path.cwd())]
    return Settings(
        life_update_token=token,
        life_update_api_url=os.environ.get("LIFE_UPDATE_API_URL", "https://life-update.com"),
        ollama_host=os.environ.get("OLLAMA_HOST", "http://localhost:11434"),
        ollama_model=os.environ.get("OLLAMA_MODEL") or load_selected_model(),
        idle_threshold_minutes=float(os.environ.get("IDLE_THRESHOLD_MINUTES", "3")),
        cpu_load_ceiling_percent=float(os.environ.get("CPU_LOAD_CEILING_PERCENT", "30")),
        watch_dirs=watch_dirs,
        presidio_spacy_model=os.environ.get("PRESIDIO_SPACY_MODEL", "en_core_web_sm"),
        screen_watch_enabled=(
            os.environ.get("SCREEN_WATCH_ENABLED", "").lower() in ("1", "true")
            if "SCREEN_WATCH_ENABLED" in os.environ
            else load_screen_watch_enabled()
        ),
        screen_capture_interval_seconds=float(
            os.environ.get("SCREEN_CAPTURE_INTERVAL_SECONDS") or load_screen_capture_interval_seconds()
        ),
        vision_engine=os.environ.get("VISION_ENGINE") or load_vision_engine(),
        exclude_list=load_exclude_list(),
    )
