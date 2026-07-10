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

1. **Watches** your active window/app, file saves, git commits, and
   (opt-in, off by default) what's actually on screen.
2. **Redacts** anything sensitive before it's ever stored (see below).
3. **Summarizes** your activity into work sessions using
   **Apple Intelligence** (the on-device model built into macOS 26+) -
   entirely on your machine, nothing downloaded, nothing sent to any AI
   provider. Macs without Apple Intelligence can use a local model through
   [Ollama](https://ollama.com) instead.
4. **Syncs** only the distilled result - a project name, category, a rough
   focus score, and a one-sentence summary - to life-update.com.

Nothing about *how* you did the work (keystrokes, screen contents, file
contents, exact URLs visited) ever leaves your machine. Only the distilled
summary of *what* session happened, and when. The **History** tab in the
app shows every single thing that was captured, and clicking any session on
**Home** shows exactly which events its summary was based on - full
transparency, verifiable in the UI, not just in the code.

## Architecture: one small Rust process

The whole agent lives inside the Tauri app as a handful of Rust threads -
there is no separate daemon, no Python runtime, no bundled model server.
The app is ~18MB installed and idles around 100MB of memory (mostly the
settings UI's webview).

```
capture threads (always-on, no models, native OS APIs)
  window tracker · file/git watcher · screen watcher (opt-in)
  -> pattern + entropy redaction (deterministic, inline)
  -> local SQLite store (raw_events)
sync (every 60s, cheap)
  -> unsent summaries -> POST /api/portfolio-events
inference (only when idle + low CPU load)
  -> session clustering (time-gap based)
  -> Apple Intelligence (or Ollama) -> {project, category, summary}
  -> focus score (computed, not LLM-guessed)
  -> sync queue
```

Screenpipe-style native-API choices, and what each replaced:

| Concern | Now | Previously |
|---|---|---|
| Screen OCR | **Apple Vision framework**, in-process, ~100ms, hardware-accelerated | Bundled Tesseract binary |
| Session summaries | **Apple Intelligence** (FoundationModels), managed by macOS, zero download, zero resident RAM | Bundled Ollama runtime + multi-GB model held in memory |
| Idle detection | One `CGEventSourceSecondsSinceLastEventType` call | Always-on pynput keyboard/mouse listeners |
| Contextual PII pass | Extended regex + entropy scan | Presidio + spaCy (~900MB resident) |
| The agent itself | Rust threads in the app | PyInstaller-frozen Python daemon child process |

**Ollama is no longer required, bundled, or spawned.** It's an optional
alternative engine: if you pick an Ollama model in Settings (for Macs
without Apple Intelligence), the app talks to *your own* Ollama install at
`localhost:11434`, pulls with visible progress, and passes `keep_alive: 0`
so the model unloads the moment each summarization batch finishes.

## The privacy model

1. **Exclude-list** (`agent/mod.rs::is_excluded`) - checked *before*
   anything is captured. Password managers, banking sites, and anything you
   add yourself never become an event in the first place. Excluded apps are
   never screenshotted at all.
2. **Pattern + entropy scan** (`agent/redaction.rs`) - runs inline, before
   any write to local storage. Regex recognizers catch known secret shapes
   (API keys, tokens, Luhn-validated card numbers, emails, JWTs, private
   key blocks); a Shannon entropy check catches high-randomness strings
   that don't match a known shape.
3. **On-device summarization** - the (already-redacted) activity log is
   summarized by a model that never leaves the machine, and only the
   model's structured output - never the underlying log - is queued for
   sync.

## The app

A [Tauri](https://tauri.app) app with a sidebar:

- **Home** - agent status, start/pause, capture counters, and every
  summarized session. Click a session to see the exact (redacted) events
  the summary was based on.
- **History** - a live feed of everything captured on this Mac, with
  whether it's been summarized yet. Click a row to expand its full text.
- **Settings** - AI engine picker (Apple Intelligence default; Ollama
  models optional, with live pull progress), screen watching (off by
  default, interval + vision engine), exclude-list management, launch at
  login, and a danger zone that deletes all local data.

Plus a tray icon (open settings / pause / resume / quit). Closing the
window hides it; capture keeps running in the tray.

### Screen watching (optional, off by default)

Window titles alone only say *which app* was open. Screen watching reads
what's on screen so a session says "debugging a memory leak in the queue
implementation" instead of "used PyCharm". Capture is hybrid: on a timer
(default 120s) and immediately on every app/window switch.

| Engine | What it does | When it runs |
|---|---|---|
| `native` (default) | Apple Vision OCR - literal screen text, on-device, instant | Inline in the capture loop |
| `qwen2.5vl:3b` / `:7b` | A local vision model describes the screen semantically | Deferred to the idle-gated worker (seconds per frame); frames wait in a small in-memory queue (capped at 20, never written to disk), requires the Ollama app |

Raw images are discarded the moment text comes back, and that text goes
through the same redaction as everything else before touching storage.
macOS will prompt for Screen Recording permission the first time the
watcher starts; if denied, screen watching disables itself for the session
and everything else keeps working.

## Development

Requires Rust (arm64 toolchain), Node 24, and Xcode Command Line Tools with
the macOS 26 SDK (for the Apple Intelligence helper - see below).

```bash
cd app-shell
nvm use
npm install
npm run tauri dev      # builds the AI helper automatically first
```

Config lives in `~/.life-update-agent/`: `config.json` (engine choices,
exclude-list, screen watching - managed by the UI) and `.env`:

| Variable | Description |
|---|---|
| `LIFE_UPDATE_TOKEN` | Written by onboarding; from life-update.com → Settings → Devices |
| `LIFE_UPDATE_API_URL` | Defaults to `https://life-update.com` |
| `OLLAMA_HOST` | Defaults to `http://localhost:11434` (only used with an Ollama engine) |
| `WATCH_DIRS` | Comma-separated project directories to watch for file/git activity |
| `IDLE_THRESHOLD_MINUTES` | Idle time before the inference worker runs (default 3) |
| `CPU_LOAD_CEILING_PERCENT` | CPU ceiling the worker also waits under (default 50) |

### The Apple Intelligence helper

The FoundationModels framework is Swift-only, so the one non-Rust piece is
`swift/LifeUpdateAI.swift` (~90 lines): reads an activity log on stdin,
prints `{project, category, summary}` JSON on stdout. The Rust worker
shells out to it per session. It deliberately avoids the `@Generable`
guided-generation macro because that macro's compiler plugin ships only
with full Xcode - plain prompting keeps the build working with Command
Line Tools alone. `scripts/build-ai-helper.sh` compiles it into
`src-tauri/resources/ai-helper/` (wired into `beforeDevCommand`/
`beforeBuildCommand`, so it's automatic).

On Macs where Apple Intelligence is unavailable (pre-26 macOS, Intel, or
not enabled in System Settings), the helper reports why, the UI surfaces
it, and the Ollama engines remain as the alternative.

### Signing identity (required to build)

Builds are signed with a local certificate named `Life-Update Signing`
rather than ad-hoc. This matters: macOS ties Screen Recording permission
to the signature's designated requirement, and ad-hoc signatures change
every build - which silently invalidated the permission on every update.
A stable identity keeps grants alive across releases. Create yours once:

```bash
openssl req -x509 -newkey rsa:2048 -keyout k.pem -out c.pem -days 3650 -nodes \
  -subj "/CN=Life-Update Signing" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
openssl pkcs12 -export -legacy -out i.p12 -inkey k.pem -in c.pem -passout pass:x
security import i.p12 -k ~/Library/Keychains/login.keychain-db -P x -T /usr/bin/codesign
rm k.pem c.pem i.p12
```

(Or set `bundle.macOS.signingIdentity` back to `"-"` for ad-hoc local builds.)

### Building the installer

```bash
npm run tauri:build    # -> src-tauri/target/aarch64-apple-darwin/release/bundle/{macos,dmg}/
```

The `.dmg` is ~7MB. **Always `npm run tauri:build`, never plain
`npm run tauri build`**: on a machine where rustup itself runs under
Rosetta, a plain build silently produces an Intel-only binary that fails
on other Apple Silicon Macs with "app is damaged" (this shipped once).
`tauri:build` pins `--target aarch64-apple-darwin`. The binary is named
`Life-Update` (via `mainBinaryName`), which is also what macOS shows in
login-items/background notifications.

## Uninstalling

1. In the app: Settings → **Delete all local data**. This removes
   `~/.life-update-agent/` (capture database, device token, settings) and
   the launch-at-login entry.
2. Drag **Life-Update** from Applications to the Bin.
3. Only if you ever picked an Ollama model: those live in the Ollama app's
   own storage - remove them there (`ollama rm <model>`), since
   uninstalling Life-Update never touches another app's data.

## Status

The full pipeline (capture → redaction → local store → idle-gated
on-device summarization → sync) is pure Rust + one Swift helper, verified
end-to-end on macOS 26: real windows/files/commits captured, real Apple
Vision OCR of screen content, a real Apple Intelligence summarization
producing correct `{project, category, summary}` output, and real syncs to
life-update.com. Voice/ASR is deferred to a later phase entirely.

## Contributing

This is open source specifically so you can verify the privacy claims
above yourself - read `app-shell/src-tauri/src/agent/redaction.rs` and
`agent/summarize.rs` before trusting any of this with your screen. Issues
and PRs welcome.
