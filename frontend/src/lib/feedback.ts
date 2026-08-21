// アプリ内フィードバック導線。ベータ用 Google フォームをデフォルトブラウザで開く。
// 送信はすべてユーザー操作起点（北極星 "送信なし" と両立。プリフィルは URL に載るだけで自動送信しない）。
//
// トリアージしやすいよう、アプリ/OS 情報をフォームの「事前入力」機能でプリフィルする。
// 公開 /viewform に `?usp=pp_url&entry.<id>=<値>` を付ける方式で、必須ラジオも選択肢文字列の
// 完全一致で選択される（実機検証済み）。自由記述（para）項目はプリフィルしない。
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { arch, version as osVersion } from "@tauri-apps/plugin-os";

/** ベータ版フィードバックフォーム（公開 /viewform。編集 URL ではプリフィルが効かない）。 */
const FORM_BASE =
  "https://docs.google.com/forms/d/e/1FAIpQLSewPo-AEf6Hg3qRGi24eB5i4qBKkqLrtKrdI59BdKzEDae4IQ/viewform";

// entry ID はフォーム構造（FB_PUBLIC_LOAD_DATA_）から確認済み。
const ENTRY_APP_VERSION = "entry.414119274"; // 「アプリバージョン（自動入力）」短文（フォーム末尾）
const ENTRY_OS_VERSION = "entry.12617430"; // 「macOSのバージョン（任意・短文）」
const ENTRY_MAC_TYPE = "entry.1946377959"; // 「お使いのMacは？」（必須ラジオ）

// arch → ラジオ選択肢。値は**フォームの選択肢ラベルと完全一致**させること
// （owner がフォームのラベルを変えると無言でプリフィルが効かなくなる）。
const MAC_TYPE_BY_ARCH: Record<string, string> = {
  aarch64: "Apple Silicon（M1以降）",
  x86_64: "Intel",
};

/**
 * フィードバックフォームをブラウザで開く。アプリ/OS 情報をプリフィルする。
 * 各情報の取得は失敗しても致命的でない（プリフィルが1つ減るだけ）→ 個別に握りつぶす。
 */
export async function openFeedbackForm(): Promise<void> {
  // 空白は %20 で符号化する（実機検証はこの形式。URLSearchParams の "+" は使わない）。
  const parts = ["usp=pp_url"];
  const add = (key: string, value: string) => parts.push(`${key}=${encodeURIComponent(value)}`);

  if (ENTRY_APP_VERSION) {
    try {
      add(ENTRY_APP_VERSION, await getVersion());
    } catch {
      /* バージョン取得不可でもフォームは開く */
    }
  }

  // plugin-os の version()/arch() は同期関数（Promise ではない）。
  try {
    add(ENTRY_OS_VERSION, osVersion());
  } catch {
    /* OS バージョン取得不可 */
  }
  try {
    const macType = MAC_TYPE_BY_ARCH[arch()];
    if (macType) add(ENTRY_MAC_TYPE, macType); // 未知 arch は付けずユーザー選択に委ねる
  } catch {
    /* arch 取得不可 */
  }

  await openUrl(`${FORM_BASE}?${parts.join("&")}`);
}
