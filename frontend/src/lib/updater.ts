// アプリ内アップデート（Tauri v2 updater plugin）のロジック。
// 設計・配信方式の詳細は docs/04_operations/updater-plan.md を参照。
// - checkForUpdate(): 更新の有無を取得（最新 or 失敗時は null を返して UI を邪魔しない）
// - downloadAndApply(): DL → インストール → 再起動

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateProgress =
  | { status: "idle" }
  | { status: "downloading"; downloaded: number; total: number | null }
  | { status: "ready" }
  | { status: "error"; message: string };

/**
 * 更新の有無を確認する。最新なら null。
 * チェック失敗（オフライン・エンドポイント不達など）も握りつぶして null を返し、UI を邪魔しない。
 */
export async function checkForUpdate(): Promise<Update | null> {
  try {
    return await check();
  } catch (e) {
    console.warn("[updater] check failed:", e);
    return null;
  }
}

/**
 * 更新をダウンロード → インストールし、アプリを再起動する。
 * 進捗は onProgress で通知。成功時は relaunch するため通常この関数は戻らない。
 */
export async function downloadAndApply(
  update: Update,
  onProgress?: (p: UpdateProgress) => void,
): Promise<void> {
  let downloaded = 0;
  let total: number | null = null;

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        onProgress?.({ status: "downloading", downloaded: 0, total });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({ status: "downloading", downloaded, total });
        break;
      case "Finished":
        onProgress?.({ status: "ready" });
        break;
    }
  });

  await relaunch();
}
