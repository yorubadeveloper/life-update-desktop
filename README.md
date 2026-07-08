# life-update-desktop

The local capture agent for [life-update.com](https://life-update.com) - a
public changelog for your life. It quietly watches how you work, builds a
local memory of it, and turns that into a portfolio timeline - without any
raw screen, audio, or text ever leaving your machine unredacted or
unsummarized.

**This is the open-source client.** Nothing here requires life-update.com -
you can point it at a self-hosted instance, or read the code to see exactly
what's captured, what's redacted, and what's sent, before you ever run it.

## What it does

1. **Watches** your active window/app, file saves, and git commits.
2. **Redacts** anything sensitive, in three layers, before it's ever sent
   anywhere (see below).
3. **Clusters** your activity into work sessions using a local LLM
   (phi3:mini via [Ollama](https://ollama.com)) - entirely on your machine.
4. **Syncs** only the distilled result - a project name, category, a rough
   focus score, and a one-sentence summary - to life-update.com.

Nothing about *how* you did the work (keystrokes, screen contents, file
contents, exact URLs visited) ever leaves your machine. Only the distilled
summary of *what* session happened, and when.

## The privacy model - three layers, in order

1. **Exclude-list** (`life_update_agent/capture/exclude_list.py`) - checked
   *before* anything is captured. Password managers, banking sites, and
   anything you add yourself never become an event in the first place.
2. **Pattern + entropy scan** (`life_update_agent/redaction/`) - runs
   inline, before any write to local storage. Regex recognizers catch known
   secret shapes (API keys, tokens, card numbers, emails, JWTs); a Shannon
   entropy check catches high-randomness strings that don't match a known
   shape.
3. **Contextual PII redaction** (`life_update_agent/inference/presidio_pass.py`)
   - catches what the first two layers can't: names, addresses, anything
   sensitive by context rather than format. Uses
   [Microsoft Presidio](https://microsoft.github.io/presidio/) with a local
   spaCy model. Runs only inside the idle-triggered inference worker, never
   in the real-time capture path.

Only after all three layers have run does anything get handed to the local
LLM for clustering/summarization, and only the LLM's structured output -
never the underlying activity log - is queued for sync.

## Architecture

```
capture (always-on, no models)
  -> pattern + entropy scan (deterministic, inline)
  -> local SQLite store (raw_events)
  -> [idle + low CPU load only]
     -> Presidio contextual PII pass
     -> session clustering (time-gap based)
     -> phi3:mini via Ollama -> {project, category, summary}
     -> focus score (computed, not LLM-guessed)
  -> sync queue -> POST /api/portfolio-events (life-update.com)
```

Everything above the idle gate runs continuously and cheaply. Everything
below it only runs when your machine is idle and under low load, so it
never competes with what you're actually doing.

## Setup

Requires Python 3.13+, [`uv`](https://docs.astral.sh/uv/), and
[Ollama](https://ollama.com) installed locally.

```bash
cd agent
uv sync
cp .env.example .env
```

Edit `.env`:

| Variable | Description |
|---|---|
| `LIFE_UPDATE_TOKEN` | From life-update.com → Settings → Devices → "Generate token" |
| `LIFE_UPDATE_API_URL` | Defaults to `https://life-update.com`; point at `http://localhost:3000` for a local dev server |
| `OLLAMA_HOST` | Defaults to `http://localhost:11434` |
| `OLLAMA_MODEL` | Optional - overrides the model chosen via `model choose` below |
| `IDLE_THRESHOLD_MINUTES` | How long the machine must be idle before the inference worker runs (default 3) |
| `CPU_LOAD_CEILING_PERCENT` | CPU ceiling the worker also waits under (default 30) |
| `WATCH_DIRS` | Comma-separated project directories to watch for file/git activity (defaults to the current directory) |

### macOS permissions

Window-title tracking uses the Accessibility APIs. The first time you run
the agent, macOS will prompt you to grant Accessibility (and possibly
Screen Recording) permission to your terminal - this is required for
`window_tracker.py` to read window titles. If you skip it, the agent keeps
running but logs a warning instead of crashing.

### Choose a model

The agent uses a local LLM (via Ollama) to cluster your activity into
sessions. Pick one - pulled automatically if not already present:

```bash
uv run life-update-agent model list
uv run life-update-agent model choose phi3:mini   # or qwen2.5:0.5b, llama3.2:1b, ...
```

### Run it

```bash
uv run life-update-agent run
```

Runs in the foreground; `Ctrl+C` to stop. Manage the exclude-list with:

```bash
uv run life-update-agent exclude list
uv run life-update-agent exclude add --app "Signal"
uv run life-update-agent exclude add --title-pattern '(?i)\bmedical\b'
```

## The desktop shell (`app-shell/`)

A [Tauri](https://tauri.app) app that wraps the Python daemon above with an
actual UI: onboarding (paste your device token), a model picker with live
pull progress, exclude-list management, a "launch at login" toggle, and a
tray icon (pause/resume, quit).

```bash
cd app-shell
nvm use   # picks up Node 24 via .nvmrc
npm install
npm run tauri dev
```

### Building the real installer

The dev workflow above shells out to `uv run life-update-agent ...`. A real
build instead bundles a frozen Python binary and the Ollama runtime as
resources, so the `.app`/`.dmg` runs standalone - no `uv`/`python`/`ollama`
required on the target machine:

```bash
../agent/build.sh                # freezes agent/ -> agent/dist/life-update-agent/ (PyInstaller)
./scripts/fetch-ollama.sh        # downloads Ollama's macOS runtime -> src-tauri/ollama-runtime/
./scripts/prepare-resources.sh   # stages both into src-tauri/resources/ for bundling
npm run tauri build              # -> src-tauri/target/release/bundle/{macos,dmg}/
```

`agent.rs`/`ollama_process.rs` resolve the bundled resources at runtime if
present and fall back to `uv run`/system `ollama` otherwise - the same dev
workflow above keeps working unchanged. If something is already serving on
`OLLAMA_HOST` (a system Ollama install), it's reused rather than
double-spawned, and never killed on quit unless we started it.

Known trade-off: bundling Ollama's own binary means macOS surfaces it by
its own name in places like the "background activity" notification on
first launch - it's invisible in the sense that the user never installs it
separately, but not invisible at the OS level. Switching to an in-process
inference library (e.g. `llama-cpp-python`) would remove that, at the cost
of reworking the model-pull UI around raw GGUF downloads instead of Ollama
tags - not done, by choice, for now.

## Status

Layers 1-5 (capture, redaction, local storage, idle-gated inference, sync)
and Layer 7 (packaging) are done: `npm run tauri build` produces a working
`.dmg` with a frozen Python daemon and bundled Ollama runtime, verified to
launch, show the correct name/icon in the Dock, and correctly reuse (not
duplicate) an already-running system Ollama. Model weights are
download-on-first-run rather than bundled, to keep the installer small.
Voice/ASR is deferred to a later phase entirely.

## Contributing

This is open source specifically so you can verify the privacy claims above
yourself - read `life_update_agent/redaction/` and
`life_update_agent/inference/presidio_pass.py` before trusting any of this
with your screen. Issues and PRs welcome.
