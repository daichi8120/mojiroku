# frontend — mojiroku UI

Tauri アプリのフロントエンド。**Vite + React + TypeScript + Tailwind CSS + shadcn/ui + zustand**。
Next.js を使わない理由は [`../docs/decisions/ADR-0006_フロントはViteでNextは不採用.md`](../docs/decisions/ADR-0006_フロントはViteでNextは不採用.md) を参照。

UI は Rust コアと **Tauri の `invoke`（要求）/ `event`（進捗）** で通信する。localhost HTTP は持たない。

## ディレクトリ

```
src/
├── features/        # 機能モジュール（縦割り）。各 feature は components/hooks/types を自己完結
│   ├── transcription/   # 文字起こし結果の表示・編集
│   ├── recording/       # マイク録音・ライブ取り込み（Phase 3-4）
│   ├── summary/         # 要約・議事録の生成と表示
│   ├── settings/        # モデル選択・BYOK キー・言語
│   └── history/         # 過去 Recording の一覧・再閲覧
├── components/      # feature 横断の共有 UI（shadcn/ui ベース）
├── lib/             # tauri invoke ラッパ、event 購読、ユーティリティ
├── stores/          # zustand ストア（グローバル状態）
├── types/           # 共有型（Rust とは ts-rs 等で型共有を検討。spec §4 参照）
└── styles/          # Tailwind エントリ・グローバル CSS
```

## 次フェーズで scaffold

`npm create vite@latest`（react-ts）→ Tailwind / shadcn/ui / zustand 導入。本ディレクトリは骨格のみ。
