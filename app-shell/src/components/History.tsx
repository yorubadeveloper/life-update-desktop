import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppWindow, FileText, GitCommit, Monitor } from "@phosphor-icons/react";

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

const KIND_META: Record<string, { label: string; icon: typeof AppWindow }> = {
  window: { label: "Window", icon: AppWindow },
  file: { label: "File", icon: FileText },
  git_commit: { label: "Commit", icon: GitCommit },
  screen_text: { label: "Screen", icon: Monitor },
};

function primaryText(e: RawEventView): string {
  switch (e.kind) {
    case "window":
      return e.window_title || e.app_name || "(untitled window)";
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

export function History() {
  const [events, setEvents] = useState<RawEventView[]>([]);
  const [expanded, setExpanded] = useState<number | null>(null);

  const refresh = useCallback(() => {
    invoke<RawEventView[]>("recent_events", { limit: 200 }).then(setEvents).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 5000);
    return () => clearInterval(interval);
  }, [refresh]);

  return (
    <div className="max-w-2xl mx-auto px-6 py-8 space-y-4">
      <div>
        <h1 className="text-lg font-semibold text-foreground">Capture history</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          Everything the agent has recorded on this Mac - already redacted before it was
          stored, and never uploaded raw. Only the on-device session summaries sync.
        </p>
      </div>

      {events.length === 0 ? (
        <div className="glass rounded-2xl p-8 text-center">
          <p className="text-sm text-muted-foreground">
            Nothing captured yet. Start the agent from Home and switch between a few apps.
          </p>
        </div>
      ) : (
        <div className="glass rounded-2xl divide-y divide-black/5">
          {events.map((e) => {
            const meta = KIND_META[e.kind] ?? KIND_META.window;
            const Icon = meta.icon;
            const isOpen = expanded === e.id;
            return (
              <button
                key={e.id}
                onClick={() => setExpanded(isOpen ? null : e.id)}
                className="w-full text-left flex items-start gap-3 px-4 py-2.5 hover:bg-white/40 transition-colors"
              >
                <Icon size={15} className="text-muted-foreground mt-0.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p className={`text-sm text-foreground ${isOpen ? "whitespace-pre-wrap break-words" : "truncate"}`}>
                    {primaryText(e)}
                  </p>
                  <p className="text-[11px] text-muted-foreground mt-0.5">
                    {meta.label}
                    {e.app_name ? ` · ${e.app_name}` : ""}
                    {" · "}
                    {new Date(e.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    {e.processed ? " · summarized" : " · waiting for idle"}
                  </p>
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
