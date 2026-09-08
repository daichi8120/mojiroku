import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LiveTranslationController, type TranslationTarget, type TranslationTransport, type TranslationView } from "./liveTranslation";
import type { LiveSnapshot } from "./tauri";

const transport: TranslationTransport = {
  begin: (target) => invoke("begin_live_translation", { target }),
  end: (epoch) => invoke("end_live_translation", { epoch }),
  cancel: (epoch, requestId) => invoke("cancel_live_translation_request", { epoch, requestId }),
  listen: (event, handler) => listen(event, (e) => handler(e.payload as Parameters<typeof handler>[0])),
  translate: (epoch, requestId, text) => invoke("translate_live_line", { epoch, requestId, text }),
};

export function useLiveTranslation(capturing: boolean, snapshot: LiveSnapshot | null) {
  const [view, setView] = useState<TranslationView>({ enabled: false, starting: false, rows: [], progress: null, failed: false, unavailable: false, pending: 0, skipped: 0, historyFull: false });
  const controller = useMemo(() => new LiveTranslationController(transport, setView), []);
  useEffect(() => () => controller.stop(), [controller]);
  useEffect(() => {
    if (!capturing) controller.stop();
  }, [capturing, controller]);
  useEffect(() => {
    if (capturing && snapshot) controller.update(snapshot.session_id, snapshot.lines);
  }, [capturing, snapshot, controller]);
  return {
    ...view,
    start: (target: TranslationTarget) => { if (capturing) void controller.start(target); },
    stop: () => controller.stop(),
    completed: () => controller.completed(),
    reset: () => controller.reset(),
    retry: (target: TranslationTarget) => { if (capturing) controller.retry(target); },
  };
}
