// アップデート通知バナー。起動時に更新の有無を確認し、あれば画面上部にフロート表示する。
// 「今すぐ更新して再起動」で DL → インストール → relaunch。チェック失敗はサイレント。
// App のルート（AppCtx.Provider 内）で <UpdateBanner /> を一度描画する。

import { useEffect, useState } from "react";
import type { Update } from "@tauri-apps/plugin-updater";
import { translateError, useI18n } from "@/i18n";
import { checkForUpdate, downloadAndApply, type UpdateProgress } from "@/lib/updater";

export function UpdateBanner() {
  const { t } = useI18n();
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<UpdateProgress>({ status: "idle" });
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const u = await checkForUpdate();
      if (!cancelled && u) setUpdate(u);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (!update || dismissed) return null;

  const downloading = progress.status === "downloading";
  const pct =
    downloading && progress.total
      ? Math.round((progress.downloaded / progress.total) * 100)
      : null;

  const onUpdate = async () => {
    try {
      setProgress({ status: "downloading", downloaded: 0, total: null });
      await downloadAndApply(update, setProgress);
      // 成功時は downloadAndApply 内で relaunch するため通常ここには戻らない。
    } catch (e) {
      setProgress({ status: "error", message: translateError(e, t) });
    }
  };

  return (
    <div className="fixed left-1/2 top-4 z-[70] flex -translate-x-1/2 items-center gap-3 rounded-[10px] border border-border-3 bg-surface px-4 py-2.5 text-[12.5px] text-body shadow-[0_20px_50px_rgba(0,0,0,0.5)]">
      <span className="font-medium">{t.update.newVersion(update.version)}</span>
      {update.body ? (
        <span className="max-w-[280px] truncate text-body/60">{update.body}</span>
      ) : null}

      <div className="ml-2 flex items-center gap-2">
        {progress.status === "error" ? (
          <span className="text-red-light">{t.update.failed}</span>
        ) : null}
        <button
          onClick={onUpdate}
          disabled={downloading}
          className="rounded-md border border-green/40 bg-surface px-2.5 py-1 text-green-light hover:border-green/70 disabled:opacity-50"
        >
          {downloading
            ? pct !== null
              ? `${t.update.updating} ${pct}%`
              : t.update.updating
            : t.update.updateNow}
        </button>
        {!downloading ? (
          <button
            onClick={() => setDismissed(true)}
            className="rounded-md px-2 py-1 text-body/60 hover:text-body"
          >
            {t.update.later}
          </button>
        ) : null}
      </div>
    </div>
  );
}
