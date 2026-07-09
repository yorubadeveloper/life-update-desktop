import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { House, ClockCounterClockwise, GearSix, ArrowClockwise } from "@phosphor-icons/react";
import "./App.css";
import { Onboarding } from "./components/Onboarding";
import { Home } from "./components/Home";
import { History } from "./components/History";
import { Settings } from "./components/Settings";

type Tab = "home" | "history" | "settings";

const TABS: { id: Tab; label: string; icon: typeof House }[] = [
  { id: "home", label: "Home", icon: House },
  { id: "history", label: "History", icon: ClockCounterClockwise },
  { id: "settings", label: "Settings", icon: GearSix },
];

export default function App() {
  const [loading, setLoading] = useState(true);
  const [hasToken, setHasToken] = useState(false);
  const [tab, setTab] = useState<Tab>("home");
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);

  useEffect(() => {
    invoke<{ token: string; api_url: string }>("get_token_settings").then((s) => {
      setHasToken(!!s.token);
      setLoading(false);
    });
    const unlisten = listen<string>("update-ready", (e) => setUpdateVersion(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  if (loading) {
    return <div className="min-h-screen flex items-center justify-center text-muted-foreground">Loading…</div>;
  }

  if (!hasToken) {
    return (
      <div className="min-h-screen bg-background text-foreground">
        <Onboarding onConnected={() => setHasToken(true)} />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background text-foreground flex">
      <aside className="w-44 shrink-0 border-r border-black/5 flex flex-col py-5 px-3 gap-1 sticky top-0 h-screen">
        <div className="flex items-center gap-2 px-2 pb-4">
          <span className="text-lg">🌱</span>
          <span className="text-sm font-semibold text-foreground">Life-Update</span>
        </div>
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`flex items-center gap-2.5 rounded-xl px-3 py-2 text-sm transition-colors text-left ${
              tab === id
                ? "bg-primary/10 text-primary font-medium"
                : "text-muted-foreground hover:bg-black/5 hover:text-foreground"
            }`}
          >
            <Icon size={16} weight={tab === id ? "fill" : "regular"} />
            {label}
          </button>
        ))}
      </aside>

      <main className="flex-1 min-w-0 overflow-y-auto h-screen">
        {tab === "home" && <Home />}
        {tab === "history" && <History />}
        {tab === "settings" && <Settings />}
      </main>

      {updateVersion && (
        <div className="fixed bottom-4 right-4 glass rounded-2xl px-4 py-3 flex items-center gap-3 shadow-lg border border-black/5 z-50">
          <p className="text-sm text-foreground">
            Update <span className="font-semibold">v{updateVersion}</span> is ready
          </p>
          <button
            onClick={() => invoke("restart_app")}
            className="flex items-center gap-1.5 bg-primary text-primary-foreground rounded-xl px-3 py-1.5 text-sm font-medium hover:bg-accent transition-colors"
          >
            <ArrowClockwise size={14} weight="bold" />
            Restart
          </button>
        </div>
      )}
    </div>
  );
}
