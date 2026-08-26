// モック画面（未実装機能のプレビュー）のサンプルデータ。
// ⚠️ これらの機能はバックエンド未実装。デザイン全体像を可視化するためのプレビュー用。
// 実機能の配線（lib/tauri.ts）とは無関係。
//
// MOCK_PREVIEW は**単一スイッチ**: false にすると全モック UI が到達不能になる。
//   - DetailView: 「質問する」/ 横断ダイジェスト / チャプター / 翻訳（AskDrawer・DigestView の入口ごと消える）
//   - IntegrationsView: 会議プラットフォーム欄（セクションごと消える）
//   - PreviewTag（components/composite.tsx）: null を返す
//
// ⚠️ **手で false にする運用はやめた。** 以前は「配布ビルド時にこの 1 行を false にすればよい」
// と書いてあったが、**一度も実行されないまま v0.5.2 まで配布された**（Issue #21）。
// 利用者の手元に、押しても固定文しか返らない「質問する」「チャプター」「翻訳」等が
// 出ていた。人が覚えていることを前提にした手順は、いずれ必ず抜ける。
//
// 開発サーバー（`just dev`）でだけ true になる。`vite build` を通る配布ビルドでは
// 自動的に false。切り替えを覚えておく必要はない。
export const MOCK_PREVIEW = import.meta.env.DEV;

// ── 会議モード（08） ────────────────────────────────────────────────────
export interface MockLiveLine {
  speakerId: string;
  name: string;
  text: string;
}

export const MEETING = {
  title: "週次プロダクト定例",
  platform: "Google Meet",
  source: "システム音声 + マイク",
  elapsedSec: 18 * 60 + 42,
  participants: [
    { id: "S1", name: "田中" },
    { id: "S2", name: "佐藤" },
    { id: "S3", name: "鈴木" },
  ],
  // ライブ文字起こし（1.7 秒ごとに 1 行ずつ増える演出に使う）
  liveLines: [
    { speakerId: "S1", name: "田中", text: "では今週のリリース範囲を確認します。" },
    { speakerId: "S2", name: "佐藤", text: "オンボーディングの DL 進捗バーは入りました。" },
    { speakerId: "S3", name: "鈴木", text: "話者分離のモデルは必要時 DL に変えています。" },
    { speakerId: "S1", name: "田中", text: "検証ゲートは実会議で評価という方針で合っていますか。" },
    { speakerId: "S2", name: "佐藤", text: "はい、友人と研究室に配って品質を見ます。" },
    { speakerId: "S3", name: "鈴木", text: "未署名の起動手順は xattr が主、と README に書きました。" },
  ] as MockLiveLine[],
  // ライブ AI ノート
  notePoints: [
    "今週のリリース範囲を確認",
    "オンボーディングに DL 進捗バーを追加済み",
    "話者分離モデルは必要時 DL に変更",
  ],
  noteActions: [
    { text: "ベータを友人・研究室に配布", assignee: "田中" },
    { text: "未署名起動手順を README へ反映", assignee: "鈴木", done: true },
  ],
};

// 話者ライブラリ（09）は Phase 8 で実機能化（ADR-0018）。モックは撤去
// （features/speakers/SpeakersView.tsx と features/detail/SpeakerPanel.tsx が実 API を使う）。

// ── 連携（12） ──────────────────────────────────────────────────────────
// カレンダーは実機能（lib/types.ts の CalendarEvent / lib/tauri.ts の listCalendarEvents）。
// ここに残すのは会議プラットフォーム / 書き出し先の**プレビュー（モック）**のみ。
export interface IntegrationItem {
  id: string;
  name: string;
  connected: boolean;
  note?: string;
}

export const INTEGRATIONS = {
  platforms: [
    { id: "meet", name: "Google Meet", connected: true },
    { id: "zoom", name: "Zoom", connected: true },
    { id: "teams", name: "Microsoft Teams", connected: false },
  ] as IntegrationItem[],
  exports: [
    { id: "notion", name: "Notion", connected: true },
    { id: "slack", name: "Slack", connected: false },
    { id: "obsidian", name: "Obsidian", connected: false },
    { id: "mcp", name: "MCP（Claude）", connected: true, note: "履歴を read-only 公開" },
  ] as IntegrationItem[],
};

// ── 横断ダイジェスト（16） ───────────────────────────────────────────────
export interface DigestSession {
  date: string;
  title: string;
}

export interface CrossAction {
  text: string;
  assignee: string;
  age: string; // 経過の自然文言（「今週」「先週」「2週間前」など）
  overdue?: boolean; // 滞留が長く注意が必要なもの（赤表示）
}

export interface RecurringTopic {
  topic: string;
  count: number;
}

export const DIGEST = {
  series: "週次プロダクト定例",
  sessions: [
    { date: "9/03", title: "配布方針の確定" },
    { date: "9/10", title: "話者分離の品質確認" },
    { date: "9/17", title: "MCP サーバー連携" },
    { date: "9/24", title: "UI 刷新キックオフ" },
  ] as DigestSession[],
  decisions: [
    "配布は未署名 .dmg + xattr 手順を主とする",
    "要約は Qwen2.5-7B をローカル既定、品質パスは BYOK",
    "UI はダーク Studio デザインへ全面刷新",
  ],
  openActions: [
    { text: "実会議でのベータ品質評価", assignee: "田中", age: "2週間前から", overdue: true },
    { text: "会議モードのシステム音声スパイク", assignee: "鈴木", age: "先週から" },
    { text: "ローカル RAG の構成検証", assignee: "佐藤", age: "今週から" },
  ] as CrossAction[],
  recurring: [
    { topic: "オンボーディング", count: 4 },
    { topic: "配布・署名", count: 3 },
    { topic: "話者分離の精度", count: 3 },
  ] as RecurringTopic[],
};
