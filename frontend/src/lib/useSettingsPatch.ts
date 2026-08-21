import type { Dispatch, SetStateAction } from "react";

import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { getSettings, setSettings } from "@/lib/tauri";
import type { Settings } from "@/lib/types";

/**
 * 設定の部分更新 + 永続化フック。SettingsView / IntegrationsView / 言語切替（App.tsx）が
 * 同じ settings.json を触るため、「保存直前に読み直し → 変更フィールドだけ差し替え」の
 * read-modify-write にして、この画面の古いスナップショットで他画面の変更を巻き戻さない
 * ようにする。cfg が null の間は no-op。保存失敗は toast（settings.saveFailed）。
 */
export function useSettingsPatch(
  cfg: Settings | null,
  setCfg: Dispatch<SetStateAction<Settings | null>>,
) {
  const { toast } = useApp();
  const { t } = useI18n();
  return (p: Partial<Settings>) => {
    if (!cfg) return;
    setCfg({ ...cfg, ...p });
    void (async () => {
      try {
        const cur = await getSettings();
        await setSettings({ ...cur, ...p });
      } catch (e) {
        toast(t.settings.saveFailed(translateError(e, t)), "error");
      }
    })();
  };
}
