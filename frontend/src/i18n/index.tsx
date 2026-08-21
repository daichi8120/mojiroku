// アプリ UI の i18n（依存追加なしの型付き辞書 + Context）。
// - 言語の真実は settings.json（Rust 側 settings::Settings.language）。
//   App.tsx が起動時に load し、未設定なら detectLocale() で解決して保存する。
// - 各コンポーネントは useI18n() で { t, lang, setLang } を得る（useApp と同型のパターン）。
import { createContext, useContext } from "react";
import ja, { type Dict } from "./ja";
import en from "./en";

export type Locale = "ja" | "en";

export const dicts: Record<Locale, Dict> = { ja, en };

/** OS/webview の言語から初期ロケールを推定（ja 系のみ ja、それ以外は en）。 */
export function detectLocale(): Locale {
  return (navigator.language ?? "").toLowerCase().startsWith("ja") ? "ja" : "en";
}

/** settings.json の language 値（"" を含む）を Locale へ解決する。 */
export function resolveLocale(value: string): Locale {
  return value === "en" ? "en" : value === "ja" ? "ja" : detectLocale();
}

/**
 * Rust の Err（"error.<domain>.<cause>[: detail]"）をアプリ言語の文言へ。未知キーは原文をそのまま返す。
 * detail 自体がキーのこともある（例 "error.recording.mic_start: error.mic.busy"）ため再帰的に翻訳する
 * （未知の detail は原文のまま "文言 (詳細)" で表示される）。
 */
export function translateError(e: unknown, t: Dict): string {
  const raw = String(e);
  const idx = raw.indexOf(": ");
  const key = idx === -1 ? raw : raw.slice(0, idx);
  const detail = idx === -1 ? "" : raw.slice(idx + 2);
  const msg = (t.errors as Record<string, string>)[key];
  if (!msg) return raw; // 未キー化エラーは原文フォールバック
  if (!detail) return msg;
  return `${msg} (${translateError(detail, t)})`;
}

export interface I18nApi {
  lang: Locale;
  t: Dict;
  /** 言語を切り替え、settings.json に永続化する（App.tsx が実装を供給）。 */
  setLang: (lang: Locale) => void;
}

export const I18nCtx = createContext<I18nApi | null>(null);

export function useI18n(): I18nApi {
  const ctx = useContext(I18nCtx);
  if (!ctx) throw new Error("useI18n must be used within <I18nCtx.Provider>");
  return ctx;
}
