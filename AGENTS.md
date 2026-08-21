# AGENTS

このリポジトリで作業する AI エージェント（Claude Code / Cursor / Codex など）向けの、
ツール非依存の共通入口。

> mojiroku 固有の深い開発ガイド（アーキ・重要な落とし穴・ビルド手順・現在地）は
> [`CLAUDE.md`](./CLAUDE.md) が正本。このファイルは「どこに何を置くか」と基本方針だけを扱い、
> 詳細は CLAUDE.md と `docs/` へリンクする（重複させない）。

## 最優先ルール：文書とデータの置き場

正本は **repo / GitHub Issues・Projects / ローカル＆配布物** の3層に分かれる。
repo は「全部の家」ではない。

- **repo（ここ）= コード＋確定文書**：`crates/` `frontend/` `src-tauri/` `landing/`
  `workers/` のコード、`docs/`（要件・仕様・設計・運用・ADR）、`LICENSE` / `NOTICE` / `CLA.md`。
- **GitHub Issues / Projects = タスクと進行中の議論**：これからやること、バグ、機能要望、
  設計の検討中スレッド。**確定したら ADR か `docs/` に落とし、Issue からはリンクする。**
- **大容量バイナリは repo に置かない**：モデルは**実行時 DL**
  （`~/Library/Application Support/com.daichi0812.mojiroku/models/`）、sidecar / `.app` /
  `.dmg` は**ビルド成果物**、配布は公開 `mojiroku-releases` の Releases。
  `*.gguf` `*.bin` `*.onnx` `src-tauri/binaries/` は gitignore。

→ 迷ったときの決定表：[`docs/README.md`](./docs/README.md)

## 作業時の基本方針

- **確定した技術判断は `docs/decisions/ADR-NNNN_*.md` に残す**
  （→ [ADR の流儀](./docs/decisions/README.md)）。背景・比較案・判断理由・影響が追える形で書く。
- 設計・運用・再現に関わる確定文書は `docs/` の該当ファイルに置く。**新しいカテゴリを勝手に増やさない。**
- ブランチ運用は `feat/` `fix/` → `develop`（`--no-ff`）→ マイルストーンで `main`。
  **`main` へは必ず PR 経由**（ruleset で強制されている）。詳細は
  [`docs/CONTRIBUTING.md`](./docs/CONTRIBUTING.md)。
- コミットは Conventional Commits（`feat(core): ...` / `fix(mojiroku-llm): ...`）。
- **性能・互換性の前提は断定前に裏取りする。**「動いた」は実音声・実データで確認する
  （→ CLAUDE.md「検証の心構え」）。過去に「faster-whisper は Apple Metal 対応」という
  誤った前提で設計しかけた例がある。
- **「ビルドが通る」は作業ツリーではなくコミット対象ツリーで確認する。**
  過去に `models/` の gitignore で同名ソースが漏れた前例がある（手順は CLAUDE.md）。

## ライセンス

本プロジェクトは **AGPL-3.0-or-later**。コードを提供する場合は
[`CLA.md`](./CLA.md) への同意が必要になる。判断の経緯は
[ADR-0027](./docs/decisions/ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md)。

生成したコードに第三者のコードを含める場合は、そのライセンスを [`NOTICE`](./NOTICE) に
追記できる形で明示すること。**ライセンス不明のコードを持ち込まない。**

## 詳細ルール

- [`CLAUDE.md`](./CLAUDE.md) — アーキ・重要な落とし穴・ビルド/実行・現在地（**開発の正本**）
- [`docs/CONTRIBUTING.md`](./docs/CONTRIBUTING.md) — ブランチ / コミット / タグ / PR の規約
- [`docs/README.md`](./docs/README.md) — 文書の置き場の決定表
- [`docs/decisions/README.md`](./docs/decisions/README.md) — ADR の流儀と一覧
