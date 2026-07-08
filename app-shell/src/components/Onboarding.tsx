import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePullProgress, type PullProgress } from "../hooks/usePullProgress";

const API_URL = "https://life-update.com";

export function Onboarding({ onConnected }: { onConnected: () => void }) {
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState<PullProgress | null>(null);

  usePullProgress((p) => {
    if (saving) setPullProgress(p);
  });

  async function handleConnect() {
    if (!token.trim()) {
      setError("Paste your device token first");
      return;
    }
    setSaving(true);
    setError(null);
    setPullProgress(null);
    try {
      await invoke("save_token_settings", { token: token.trim(), apiUrl: API_URL });
      await invoke("start_agent");
      onConnected();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function pullStatusText(p: PullProgress): string {
    if (p.completed != null && p.total != null && p.total > 0) {
      const pct = Math.round((p.completed / p.total) * 100);
      return `${p.status} - ${pct}%`;
    }
    return p.status;
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-8">
      <div className="glass rounded-2xl p-8 max-w-md w-full space-y-5">
        <div>
          <h1 className="text-xl font-semibold text-foreground">Connect this device</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Go to life-update.com → Settings → Devices → "Generate token", then paste it below.
          </p>
        </div>

        <label className="block text-sm">
          <span className="text-muted-foreground">Device token</span>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="paste your token here"
            className="mt-1 w-full bg-white/60 border border-black/8 rounded-xl px-4 py-2.5 text-sm outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/40"
          />
        </label>

        {error && <p className="text-sm text-destructive">{error}</p>}

        <button
          onClick={handleConnect}
          disabled={saving}
          className="w-full bg-primary text-primary-foreground rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-accent transition-colors disabled:opacity-50"
        >
          {saving ? "Connecting…" : "Connect"}
        </button>

        {saving && (
          <p className="text-xs text-muted-foreground text-center">
            {pullProgress
              ? pullStatusText(pullProgress)
              : "Setting up - may download a local AI model (~2.2 GB) the first time, which can take a few minutes."}
          </p>
        )}
      </div>
    </div>
  );
}
