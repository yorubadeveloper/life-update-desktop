import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Play,
  Pause,
  CheckCircle,
  CloudArrowUp,
  CaretDown,
  AppWindow,
  FileText,
  GitCommit,
  Monitor,
} from "@phosphor-icons/react";

interface AgentStatus {
  unprocessed_raw_events: number;
  total_captured_events: number;
  unsent_portfolio_events: number;
  total_synced_portfolio_events: number;
  last_sync_at: string | null;
}

interface SessionView {
  id: string;
  started_at: string;
  ended_at: string;
  project: string;
  category: string;
  focus_score: number;
  apps_used: string[];
  summary: string;
  sent_at: string | null;
}

interface RawEventView {
  id: number;
  ts: string;
  kind: string;
  app_name: string | null;
  window_title: string | null;
  file_path: string | null;
  extra_json: string | null;
  processed: boolean;
}

const CATEGORY_LABELS: Record<string, string> = {
  deep_work: "Deep work",
  maintenance: "Maintenance",
  meeting: "Meeting",
  learning: "Learning",
  creative: "Creative",
  admin: "Admin",
  personal: "Personal",
  other: "Other",
};

const KIND_ICONS: Record<string, typeof AppWindow> = {
  window: AppWindow,
  file: FileText,
  git_commit: GitCommit,
  screen_text: Monitor,
};

function eventText(e: RawEventView): string {
  switch (e.kind) {
    case "window":
      return [e.app_name, e.window_title].filter(Boolean).join(" - ") || "(window)";
    case "file":
      return e.file_path || "(file)";
    case "git_commit":
      return e.extra_json || "(commit)";
    case "screen_text":
      return e.extra_json || "(screen text)";
    default:
      return e.kind;
  }
}

function timeRange(start: string, end: string): string {
  const fmt = (iso: string) =>
    new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return `${new Date(start).toLocaleDateString([], { month: "short", day: "numeric" })} · ${fmt(start)}–${fmt(end)}`;
}

function SessionCard({ session }: { session: SessionView }) {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<RawEventView[] | null>(null);

  async function toggle() {
    const next = !open;
    setOpen(next);
    if (next && events === null) {
      try {
        const rows = await invoke<RawEventView[]>("session_events", {
          startedAt: session.started_at,
          endedAt: session.ended_at,
        });
        setEvents(rows);
      } catch {
        setEvents([]);
      }
    }
  }

  return (
    <div className="glass rounded-2xl overflow-hidden">
      <button onClick={toggle} className="w-full text-left p-4 space-y-1.5 hover:bg-white/40 transition-colors">
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm font-medium text-foreground truncate">{session.project}</span>
          <span className="text-xs text-muted-foreground shrink-0 flex items-center gap-1.5">
            {session.sent_at && <CheckCircle size={13} weight="fill" className="text-primary" />}
            {timeRange(session.started_at, session.ended_at)}
            <CaretDown size={12} className={`transition-transform ${open ? "rotate-180" : ""}`} />
          </span>
        </div>
        <p className="text-sm text-muted-foreground">{session.summary}</p>
        <div className="flex items-center gap-2 pt-0.5">
          <span className="text-[11px] px-2 py-0.5 rounded-full bg-primary/10 text-primary">
            {CATEGORY_LABELS[session.category] ?? session.category}
          </span>
          <span className="text-[11px] text-muted-foreground">
            focus {(session.focus_score * 100).toFixed(0)}%
          </span>
          {session.apps_used.slice(0, 4).map((app) => (
            <span key={app} className="text-[11px] text-muted-foreground">{app}</span>
          ))}
        </div>
      </button>

      {open && (
        <div className="border-t border-black/5 px-4 py-3 space-y-2 bg-white/30">
          <p className="text-[11px] font-medium text-muted-foreground uppercase tracking-wide">
            What it saw ({events?.length ?? "…"} events, redacted)
          </p>
          {events === null ? (
            <p className="text-xs text-muted-foreground">Loading…</p>
          ) : events.length === 0 ? (
            <p className="text-xs text-muted-foreground">No raw events kept for this window.</p>
          ) : (
            <div className="space-y-1.5 max-h-64 overflow-y-auto">
              {events.map((e) => {
                const Icon = KIND_ICONS[e.kind] ?? AppWindow;
                return (
                  <div key={e.id} className="flex items-start gap-2">
                    <Icon size={13} className="text-muted-foreground mt-0.5 shrink-0" />
                    <p className="text-xs text-foreground/80 whitespace-pre-wrap break-words min-w-0">
                      {eventText(e)}
                    </p>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function Home() {
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [sessions, setSessions] = useState<SessionView[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    invoke<boolean>("is_agent_running").then(setRunning).catch(() => {});
    invoke<AgentStatus>("agent_status").then(setStatus).catch(() => {});
    invoke<SessionView[]>("recent_sessions", { limit: 20 }).then(setSessions).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 5000);
    return () => clearInterval(interval);
  }, [refresh]);

  async function toggleAgent() {
    setError(null);
    try {
      if (running) {
        await invoke("stop_agent");
      } else {
        await invoke("start_agent");
      }
    } catch (e) {
      setError(String(e));
    }
    refresh();
  }

  return (
    <div className="max-w-2xl mx-auto px-6 py-8 space-y-6">
      <div className="glass rounded-2xl p-6 flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-foreground">
            {running ? "Watching your activity" : "Paused"}
          </h1>
          <p className="text-sm text-muted-foreground mt-0.5">
            {running
              ? "Capturing locally. Queued events are summarized and synced automatically once you've been away from the keyboard for ~3 minutes."
              : "Nothing is being captured right now."}
          </p>
        </div>
        <button
          onClick={toggleAgent}
          className="flex items-center gap-2 bg-primary text-primary-foreground rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-accent transition-colors"
        >
          {running ? <Pause size={16} weight="bold" /> : <Play size={16} weight="bold" />}
          {running ? "Pause" : "Start"}
        </button>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {status && (
        <div className="grid grid-cols-3 gap-3">
          <div className="glass rounded-2xl p-4">
            <p className="text-2xl font-semibold text-foreground">{status.total_captured_events}</p>
            <p className="text-xs text-muted-foreground mt-0.5">events captured</p>
          </div>
          <div className="glass rounded-2xl p-4">
            <p className="text-2xl font-semibold text-foreground">{status.unprocessed_raw_events}</p>
            <p className="text-xs text-muted-foreground mt-0.5">queued for summary</p>
          </div>
          <div className="glass rounded-2xl p-4">
            <p className="text-2xl font-semibold text-foreground">{status.total_synced_portfolio_events}</p>
            <p className="text-xs text-muted-foreground mt-0.5">
              sessions synced
              {status.unsent_portfolio_events > 0 && ` · ${status.unsent_portfolio_events} pending`}
            </p>
          </div>
        </div>
      )}

      {status?.last_sync_at && (
        <p className="text-xs text-muted-foreground flex items-center gap-1.5">
          <CloudArrowUp size={14} />
          Last synced {new Date(status.last_sync_at).toLocaleString()}
        </p>
      )}

      <div className="space-y-3">
        <h2 className="text-sm font-semibold text-foreground">Sessions</h2>
        <p className="text-xs text-muted-foreground -mt-2">
          Summarized on-device. Click one to see exactly what the summary was based on.
        </p>
        {sessions.length === 0 ? (
          <div className="glass rounded-2xl p-8 text-center">
            <p className="text-sm text-muted-foreground">
              No summarized sessions yet. Work for a bit, then step away - sessions are
              summarized on-device while you're idle.
            </p>
          </div>
        ) : (
          sessions.map((s) => <SessionCard key={s.id} session={s} />)
        )}
      </div>
    </div>
  );
}
