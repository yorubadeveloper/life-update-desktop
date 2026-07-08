import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function Onboarding({ onConnected }: { onConnected: () => void }) {
  const [token, setToken] = useState("");
  const [apiUrl, setApiUrl] = useState("https://life-update.com");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleConnect() {
    if (!token.trim()) {
      setError("Paste your device token first");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await invoke("save_token_settings", { token: token.trim(), apiUrl: apiUrl.trim() });
      await invoke("start_agent");
      onConnected();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
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

        <div className="space-y-3">
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

          <label className="block text-sm">
            <span className="text-muted-foreground">life-update.com URL</span>
            <input
              type="text"
              value={apiUrl}
              onChange={(e) => setApiUrl(e.target.value)}
              className="mt-1 w-full bg-white/60 border border-black/8 rounded-xl px-4 py-2.5 text-sm outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/40"
            />
          </label>
        </div>

        {error && <p className="text-sm text-destructive">{error}</p>}

        <button
          onClick={handleConnect}
          disabled={saving}
          className="w-full bg-primary text-primary-foreground rounded-xl px-4 py-2.5 text-sm font-medium hover:bg-accent transition-colors disabled:opacity-50"
        >
          {saving ? "Connecting…" : "Connect"}
        </button>
      </div>
    </div>
  );
}
