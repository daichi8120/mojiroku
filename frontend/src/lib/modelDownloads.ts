import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ModelDownload {
  file: string;
  size_bytes: number;
  downloaded_bytes: number;
  status: "missing" | "downloading" | "ready" | "error";
  error: string | null;
}

export const TRANSLATION_MODEL_FILE = "translation-Qwen3.5-9B-Q4_K_M.gguf";
export const LIVE_MODEL_FILE = "ggml-large-v3-turbo-q5_0.bin";
export const VAD_MODEL_FILE = "ggml-silero-v5.1.2.bin";
export const startModelDownload = (file: string) => invoke<void>("start_model_download", { file });

// The native task owns downloads. Each new view obtains an authoritative snapshot.
export function useModelDownloads() {
  const [downloads, setDownloads] = useState<Record<string, ModelDownload>>({});
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    const changed = new Set<string>();
    void (async () => {
      unlisten = await listen<ModelDownload>("model://download", ({ payload }) => {
        if (!active) return;
        changed.add(payload.file);
        setDownloads((current) => ({ ...current, [payload.file]: payload }));
      });
      if (!active) { unlisten(); return; }
      const snapshot = await invoke<ModelDownload[]>("list_model_downloads");
      if (active) setDownloads((current) => {
        const next = { ...current };
        for (const model of snapshot) if (!changed.has(model.file)) next[model.file] = model;
        return next;
      });
    })().catch((reason: unknown) => { if (active) setError(String(reason)); });
    return () => { active = false; unlisten?.(); };
  }, []);
  return { downloads, error };
}
