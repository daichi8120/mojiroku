# 0006. フロントは Vite + React（Next.js を使わない）

- ステータス: 採用
- 日付: 2026-06-24

## Context

dev-hub のフロント標準は Next.js だが、mojiroku は Tauri デスクトップアプリで、フロントは
**静的書き出し（`output: 'export'`）**で配信される。この形態では Next.js の主要価値
（SSR / RSC / サーバーアクション / API ルート）が**すべて無効**になり、ルーティングとビルドの
重さだけが残る。

Tauri フロントの事実上の標準は **Vite + React**。HMR が速く、静的出力がクリーンで、
shadcn/ui・zustand・Tailwind CSS はそのまま動く。失うのは「dev-hub 標準が Next.js」という
**一貫性のみ**だが、本アプリは元々 dev-hub 初の Rust・初デスクトップで既に外れ値であり、
一貫性の利得は小さい。

## Decision

フロントエンドは **Vite + React + TypeScript** を採用する（Next.js 不採用）。
UI ライブラリは shadcn/ui、状態管理は zustand、スタイルは Tailwind CSS。

## Consequences

- ✅ ビルドが軽く HMR が速い。静的出力が Tauri に素直に載る。
- ✅ React/TS/Tailwind/shadcn/zustand の資産・知見はそのまま活かせる。
- ⚠️ dev-hub の「Next.js 標準」から外れる（本 ADR で根拠化）。
- ルーティングは軽量ライブラリ（react-router 等）を次フェーズで選定。
