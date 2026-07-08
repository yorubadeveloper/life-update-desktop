import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePullProgress, type PullProgress } from "../hooks/usePullProgress";

const API_URL = "https://life-update.com";

interface ModelInfo {
  name: string;
  size_human: string;
  description: string;
  selected: boolean;
  downloaded: boolean | null;
}

export function Onboarding({ onConnected }: { onConnected: () => void }) {
  const [step, setStep] = useState<"token" | "model">("token");
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [models, setModels] = useState<ModelInfo[]>([]);
  const [pulling, setPulling] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState<PullProgress | null>(null);

  usePullProgress((p) => {
    if (pulling) setPullProgress(p);
  });

  async function handleConnect() {
    if (!token.trim()) {
      setError("Paste your device token first");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await invoke("save_token_settings", { token: token.trim(), apiUrl: API_URL });
      const list = await invoke<ModelInfo[]>("list_models");
      setModels(list);
      setStep("model");
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleChooseModel(name: string) {
    setPulling(name);
    setPullProgress(null);
    setError(null);
    try {
      await invoke("choose_model", { name });
      await invoke("start_agent");
      onConnected();
    } catch (e) {
      setError(String(e));
    } finally {
      setPulling(null);
      setPullProgress(null);
    }
  }

  function pullStatusText(p: PullProgress): string {
    if (p.completed != null && p.total != null && p.total > 0) {
      const pct = Math.round((p.completed / p.total) * 100);
      return `${p.status} - ${pct}%`;
    }
    return p.status;
  }

  if (step === "model") {
    return (
      <div className="min-h-screen flex items-center justify-center p-8">
        <div className="glass rounded-2xl p-8 max-w-md w-full space-y-5">
          <div>
            <h1 className="text-xl font-semibold text-foreground">Choose a local model</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Runs entirely on this machine to summarize your activity into sessions. Nothing
              downloads until you pick one below.
            </p>
          </div>

          <div className="space-y-2">
            {models.map((m) => (
              <button
                key={m.name}
                onClick={() => handleChooseModel(m.name)}
                disabled={pulling !== null}
                className={`w-full text-left rounded-xl px-4 py-3 border transition-colors ${
                  m.selected ? "border-primary/40 bg-primary/5" : "border-black/8 bg-white/40 hover:bg-white/60"
                } disabled:opacity-50`}
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium text-foreground">{m.name}</span>
                  <span className="text-xs text-muted-foreground">{m.size_human}</span>
                </div>
                <p className="text-xs text-muted-foreground mt-0.5">{m.description}</p>
                {pulling === m.name && (
                  <div className="mt-2">
                    <div className="h-1.5 bg-black/5 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all"
                        style={{
                          width:
                            pullProgress?.total
                              ? `${(100 * (pullProgress.completed ?? 0)) / pullProgress.total}%`
                              : "5%",
                        }}
                      />
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      {pullProgress ? pullStatusText(pullProgress) : "Starting download…"}
                    </p>
                  </div>
                )}
              </button>
            ))}
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
      </div>
    );
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
      </div>
    </div>
  );
}
