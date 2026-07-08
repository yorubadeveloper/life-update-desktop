import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Play, Pause, Trash, Plus } from "@phosphor-icons/react";
import { isEnabled as isAutostartEnabled, enable as enableAutostart, disable as disableAutostart } from "@tauri-apps/plugin-autostart";
import { usePullProgress, type PullProgress } from "../hooks/usePullProgress";

interface ModelInfo {
  name: string;
  size_human: string;
  description: string;
  selected: boolean;
  downloaded: boolean | null;
}

interface ExcludeList {
  apps: string[];
  title_patterns: string[];
}

interface AgentStatus {
  unprocessed_raw_events: number;
  total_captured_events: number;
  unsent_portfolio_events: number;
  total_synced_portfolio_events: number;
  last_sync_at: string | null;
}

export function Settings() {
  const [running, setRunning] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [pulling, setPulling] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState<PullProgress | null>(null);
  const [excludeList, setExcludeList] = useState<ExcludeList>({ apps: [], title_patterns: [] });
  const [newApp, setNewApp] = useState("");
  const [newPattern, setNewPattern] = useState("");
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [visionEngines, setVisionEngines] = useState<ModelInfo[]>([]);
  const [visionPulling, setVisionPulling] = useState<string | null>(null);
  const [visionPullProgress, setVisionPullProgress] = useState<PullProgress | null>(null);
  const [screenWatchEnabled, setScreenWatchEnabled] = useState(false);
  const [screenInterval, setScreenInterval] = useState(120);

  useEffect(() => {
    isAutostartEnabled().then(setAutostart).catch(() => {});
  }, []);

  async function toggleAutostart() {
    if (autostart) {
      await disableAutostart();
    } else {
      await enableAutostart();
    }
    setAutostart(await isAutostartEnabled());
  }

  const refreshModels = useCallback(() => {
    invoke<ModelInfo[]>("list_models").then(setModels).catch(() => {});
  }, []);

  const refreshExcludeList = useCallback(() => {
    invoke<ExcludeList>("get_exclude_list").then(setExcludeList).catch(() => {});
  }, []);

  const refreshVisionEngines = useCallback(() => {
    invoke<ModelInfo[]>("list_vision_engines").then(setVisionEngines).catch(() => {});
  }, []);

  const refreshScreenWatchSettings = useCallback(() => {
    invoke<{ enabled: boolean; interval_seconds: number }>("get_screen_watch_settings")
      .then((s) => {
        setScreenWatchEnabled(s.enabled);
        setScreenInterval(s.interval_seconds);
      })
      .catch(() => {});
  }, []);

  const refreshStatus = useCallback(() => {
    invoke<boolean>("is_agent_running").then(setRunning).catch(() => {});
    invoke<AgentStatus>("agent_status").then(setStatus).catch(() => {});
  }, []);

  useEffect(() => {
    refreshModels();
    refreshExcludeList();
    refreshStatus();
    refreshVisionEngines();
    refreshScreenWatchSettings();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  }, [refreshModels, refreshExcludeList, refreshStatus, refreshVisionEngines, refreshScreenWatchSettings]);

  usePullProgress(
    useCallback((p) => {
      if (pulling) {
        setPullProgress(p);
        if (p.status === "success") {
          setPulling(null);
          setPullProgress(null);
          refreshModels();
        }
      }
      if (visionPulling) {
        setVisionPullProgress(p);
        if (p.status === "success") {
          setVisionPulling(null);
          setVisionPullProgress(null);
          refreshVisionEngines();
        }
      }
    }, [pulling, visionPulling, refreshModels, refreshVisionEngines]),
  );

  async function toggleAgent() {
    if (running) {
      await invoke("stop_agent");
    } else {
      await invoke("start_agent");
    }
    refreshStatus();
  }

  async function chooseModel(name: string) {
    setPulling(name);
    setPullProgress(null);
    try {
      await invoke("choose_model", { name });
    } catch (e) {
      alert(String(e));
    } finally {
      setPulling(null);
      refreshModels();
    }
  }

  async function chooseVisionEngine(name: string) {
    setVisionPulling(name);
    setVisionPullProgress(null);
    try {
      await invoke("choose_vision_engine", { name });
    } catch (e) {
      alert(String(e));
    } finally {
      setVisionPulling(null);
      refreshVisionEngines();
    }
  }

  async function toggleScreenWatch() {
    const next = !screenWatchEnabled;
    setScreenWatchEnabled(next);
    await invoke("set_screen_watch_enabled", { enabled: next });
  }

  async function updateScreenInterval(seconds: number) {
    setScreenInterval(seconds);
    if (seconds > 0) {
      await invoke("set_screen_capture_interval", { seconds });
    }
  }

  async function addApp() {
    if (!newApp.trim()) return;
    await invoke("add_exclude_app", { app: newApp.trim() });
    setNewApp("");
    refreshExcludeList();
  }

  async function removeApp(app: string) {
    await invoke("remove_exclude_app", { app });
    refreshExcludeList();
  }

  async function addPattern() {
    if (!newPattern.trim()) return;
    await invoke("add_exclude_title_pattern", { pattern: newPattern.trim() });
    setNewPattern("");
    refreshExcludeList();
  }

  async function removePattern(pattern: string) {
    await invoke("remove_exclude_title_pattern", { pattern });
    refreshExcludeList();
  }

  return (
    <div className="max-w-2xl mx-auto px-4 py-10 space-y-8">
      <div className="glass rounded-2xl p-6 flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-foreground">life-update</h1>
          <p className="text-sm text-muted-foreground mt-0.5">
            {running ? "Running - watching your activity" : "Paused"}
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

      <label className="glass rounded-2xl p-4 flex items-center justify-between cursor-pointer">
        <div>
          <p className="text-sm font-medium text-foreground">Launch at login</p>
          <p className="text-xs text-muted-foreground mt-0.5">Start automatically when you sign in</p>
        </div>
        <input
          type="checkbox"
          checked={autostart}
          onChange={toggleAutostart}
          className="w-4 h-4 accent-primary"
        />
      </label>

      <div className="glass rounded-2xl p-6 space-y-4">
        <h2 className="font-semibold text-foreground text-sm">Local model</h2>
        <p className="text-sm text-muted-foreground">
          Used to cluster your activity into sessions, entirely on this machine.
        </p>
        <div className="space-y-2">
          {models.map((m) => (
            <button
              key={m.name}
              onClick={() => chooseModel(m.name)}
              disabled={pulling !== null}
              className={`w-full text-left rounded-xl px-4 py-3 border transition-colors ${
                m.selected ? "border-primary/40 bg-primary/5" : "border-black/8 bg-white/40 hover:bg-white/60"
              }`}
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-foreground">{m.name}</span>
                <span className="text-xs text-muted-foreground">{m.size_human}</span>
              </div>
              <p className="text-xs text-muted-foreground mt-0.5">
                {m.description}
                {m.downloaded === false && !m.selected ? " · not downloaded" : ""}
              </p>
              {pulling === m.name && pullProgress && (
                <div className="mt-2">
                  <div className="h-1.5 bg-black/5 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{
                        width: pullProgress.total
                          ? `${(100 * (pullProgress.completed ?? 0)) / pullProgress.total}%`
                          : "5%",
                      }}
                    />
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">{pullProgress.status}</p>
                </div>
              )}
            </button>
          ))}
        </div>
      </div>

      <div className="glass rounded-2xl p-6 space-y-4">
        <label className="flex items-center justify-between cursor-pointer">
          <div>
            <h2 className="font-semibold text-foreground text-sm">Screen watching</h2>
            <p className="text-sm text-muted-foreground mt-0.5">
              Off by default. Reads what's on screen so sessions describe the actual work, not just
              which app was open.
            </p>
          </div>
          <input
            type="checkbox"
            checked={screenWatchEnabled}
            onChange={toggleScreenWatch}
            className="w-4 h-4 accent-primary shrink-0 ml-4"
          />
        </label>

        {screenWatchEnabled && (
          <div className="space-y-4 pt-2 border-t border-black/5">
            <label className="block text-sm">
              <span className="text-muted-foreground">Capture every</span>
              <div className="flex items-center gap-2 mt-1">
                <input
                  type="number"
                  min={10}
                  step={10}
                  value={screenInterval}
                  onChange={(e) => updateScreenInterval(Number(e.target.value))}
                  className="w-24 bg-white/60 border border-black/8 rounded-xl px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/20"
                />
                <span className="text-sm text-muted-foreground">
                  seconds (also captures immediately whenever you switch app/window)
                </span>
              </div>
            </label>

            <div className="space-y-2">
              <p className="text-sm text-muted-foreground">Vision engine</p>
              {visionEngines.map((v) => (
                <button
                  key={v.name}
                  onClick={() => chooseVisionEngine(v.name)}
                  disabled={visionPulling !== null}
                  className={`w-full text-left rounded-xl px-4 py-3 border transition-colors ${
                    v.selected ? "border-primary/40 bg-primary/5" : "border-black/8 bg-white/40 hover:bg-white/60"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="text-sm font-medium text-foreground">{v.name}</span>
                    <span className="text-xs text-muted-foreground">{v.size_human}</span>
                  </div>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    {v.description}
                    {v.downloaded === false && !v.selected ? " · not downloaded" : ""}
                  </p>
                  {visionPulling === v.name && visionPullProgress && (
                    <div className="mt-2">
                      <div className="h-1.5 bg-black/5 rounded-full overflow-hidden">
                        <div
                          className="h-full bg-primary transition-all"
                          style={{
                            width: visionPullProgress.total
                              ? `${(100 * (visionPullProgress.completed ?? 0)) / visionPullProgress.total}%`
                              : "5%",
                          }}
                        />
                      </div>
                      <p className="text-xs text-muted-foreground mt-1">{visionPullProgress.status}</p>
                    </div>
                  )}
                </button>
              ))}
            </div>

            <p className="text-xs text-muted-foreground">
              Changes take effect next time you restart the agent (Pause, then Start above).
            </p>
          </div>
        )}
      </div>

      <div className="glass rounded-2xl p-6 space-y-4">
        <h2 className="font-semibold text-foreground text-sm">Exclude-list</h2>
        <p className="text-sm text-muted-foreground">
          Apps and window titles matching these are never captured, at all.
        </p>

        <div className="space-y-2">
          {excludeList.apps.map((app) => (
            <div key={app} className="flex items-center justify-between bg-white/40 rounded-lg px-3 py-2">
              <span className="text-sm text-foreground">{app}</span>
              <button onClick={() => removeApp(app)} className="text-muted-foreground hover:text-destructive">
                <Trash size={14} />
              </button>
            </div>
          ))}
          <div className="flex gap-2">
            <input
              value={newApp}
              onChange={(e) => setNewApp(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addApp()}
              placeholder="app name, e.g. Signal"
              className="flex-1 bg-white/60 border border-black/8 rounded-xl px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/20"
            />
            <button onClick={addApp} className="bg-primary text-primary-foreground rounded-xl px-3 py-2">
              <Plus size={14} weight="bold" />
            </button>
          </div>
        </div>

        <div className="space-y-2 pt-2 border-t border-black/5">
          {excludeList.title_patterns.map((pattern) => (
            <div key={pattern} className="flex items-center justify-between bg-white/40 rounded-lg px-3 py-2">
              <code className="text-xs text-foreground">{pattern}</code>
              <button onClick={() => removePattern(pattern)} className="text-muted-foreground hover:text-destructive">
                <Trash size={14} />
              </button>
            </div>
          ))}
          <div className="flex gap-2">
            <input
              value={newPattern}
              onChange={(e) => setNewPattern(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addPattern()}
              placeholder="title regex, e.g. (?i)\bmedical\b"
              className="flex-1 bg-white/60 border border-black/8 rounded-xl px-3 py-2 text-sm font-mono outline-none focus:ring-2 focus:ring-primary/20"
            />
            <button onClick={addPattern} className="bg-primary text-primary-foreground rounded-xl px-3 py-2">
              <Plus size={14} weight="bold" />
            </button>
          </div>
        </div>
      </div>

      {status && (
        <div className="glass rounded-2xl p-6 space-y-2">
          <h2 className="font-semibold text-foreground text-sm">Status</h2>
          <p className="text-sm text-muted-foreground">
            {status.total_synced_portfolio_events} session{status.total_synced_portfolio_events === 1 ? "" : "s"} synced ·{" "}
            {status.unsent_portfolio_events} pending ·{" "}
            {status.unprocessed_raw_events} raw event{status.unprocessed_raw_events === 1 ? "" : "s"} waiting for idle
          </p>
          {status.last_sync_at && (
            <p className="text-xs text-muted-foreground">Last synced {new Date(status.last_sync_at).toLocaleString()}</p>
          )}
        </div>
      )}
    </div>
  );
}
