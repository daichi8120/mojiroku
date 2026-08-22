// アプリ横断の状態（ルーティング + トースト）を提供するコンテキスト。
// 値は App.tsx が供給する。各ビューは useApp() で navigate / toast を得る。
import { createContext, useContext } from "react";

export type ViewKind =
  | "home"
  | "recording"
  | "history"
  | "detail"
  | "settings"
  | "meeting"
  | "speakers"
  | "integrations"
  | "digest";

export interface Route {
  view: ViewKind;
  /** detail のとき表示する録音 id。 */
  id?: string;
  /** recording のとき、停止後の文字起こしに話者分離を行うか。 */
  diarize?: boolean;
  /** recording のとき、保存時に付ける予定タイトル（カレンダー「記録を準備」由来）。 */
  title?: string;
  /** recording のとき、停止時に文字起こしせず音声だけ保存するか（後から処理・ADR-0024 増分5）。 */
  recordOnly?: boolean;
}

export type ToastKind = "info" | "success" | "error";

/**
 * 会議モードの録音状態（アプリ全体で共有）。
 * 録音実体はバックエンド（Tauri managed state）に常駐するため、画面遷移しても継続する。
 * MeetingView の mount/unmount からは独立させ、遷移で破棄されない（idle のときだけ開始画面）。
 */
export type MeetingStatus = "idle" | "capturing" | "stopping";

export interface MeetingState {
  status: MeetingStatus;
  /** capturing になった時刻（Date.now()）。経過時間はここから算出（遷移しても連続）。 */
  startedAt: number | null;
  /**
   * 保存時に使う録音タイトル（カレンダー予定名など）。null なら既定の「会議」/"Meeting"。
   * 開始から停止まで持ち回る（マイク録音が Route で title を運ぶのと同じ役割）。
   */
  title: string | null;
}

/** startMeeting の結果。denied は許可待ち（呼び出し側で誘導 UI を出す）。 */
export type MeetingStartResult = "started" | "denied" | "error";

export interface AppApi {
  route: Route;
  navigate: (route: Route) => void;
  /** 軽量トースト（2 秒で消える）。 */
  toast: (message: string, kind?: ToastKind) => void;
  /** サイドバーの「最近」を再取得（録音作成・削除後に呼ぶ）。 */
  refreshRecents: () => void;
  /** 会議モードの録音状態（idle/capturing/stopping）。 */
  meeting: MeetingState;
  /** 許可確認 → システム音声＋マイクのキャプチャ開始。画面遷移はしない。 */
  /** title を渡すとその名前で保存する（未指定なら既定の「会議」）。 */
  startMeeting: (title?: string | null) => Promise<MeetingStartResult>;
  /** 停止 → 文字起こし保存 → 詳細へ遷移。 */
  stopMeeting: () => Promise<void>;
  /** 破棄（保存しない）。誤開始のやり直し用。 */
  discardMeeting: () => Promise<void>;
}

export const AppCtx = createContext<AppApi | null>(null);

export function useApp(): AppApi {
  const ctx = useContext(AppCtx);
  if (!ctx) throw new Error("useApp must be used within <AppCtx.Provider>");
  return ctx;
}
