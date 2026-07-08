import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { Onboarding } from "./components/Onboarding";
import { Settings } from "./components/Settings";

export default function App() {
  const [loading, setLoading] = useState(true);
  const [hasToken, setHasToken] = useState(false);

  useEffect(() => {
    invoke<{ token: string; api_url: string }>("get_token_settings").then((s) => {
      setHasToken(!!s.token);
      setLoading(false);
    });
  }, []);

  if (loading) {
    return <div className="min-h-screen flex items-center justify-center text-muted-foreground">Loading…</div>;
  }

  return (
    <div className="min-h-screen bg-background text-foreground">
      {hasToken ? <Settings /> : <Onboarding onConnected={() => setHasToken(true)} />}
    </div>
  );
}
