import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

export interface PullProgress {
  status: string;
  completed?: number;
  total?: number;
}

export function usePullProgress(onProgress: (p: PullProgress) => void) {
  useEffect(() => {
    const unlisten = listen<PullProgress>("model-pull-progress", (event) => onProgress(event.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, [onProgress]);
}
