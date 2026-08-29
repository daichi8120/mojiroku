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

## Code Review Rules

PR の自動レビュー（Codex のコードレビュー）が読む規則。**見出し名は Codex 側の仕様で
固定**なので英語のまま置く（→ [Review GitHub pull requests with Codex](https://learn.chatgpt.com/docs/third-party/github)）。

ここに書くのは「**壊れ方が静かで、知らないと踏む**」制約だけにする。書式・lint のような
機械的な検査は CI に任せる。内容は [`CLAUDE.md`](./CLAUDE.md) の「重要な制約・落とし穴」と
重なるが、レビューで効かせたいものだけを再掲している。**正本は CLAUDE.md と ADR。**

### ML ランタイムの分離

- `crates/mojiroku-core`（whisper.cpp を含む）に **llama.cpp 系の依存を足さない**。
  ggml のシンボルが衝突し、**リンクは通るのに実行時に whisper が壊れて 0 セグメントになる**。
  安全な道は、要約 LLM を別バイナリ sidecar `crates/mojiroku-llm` の側で動かすこと
  （[ADR-0007](./docs/decisions/ADR-0007_要約llamaを別バイナリsidecarに分離.md)）。

### C++ FFI の呼び出し

- whisper / sherpa-onnx を呼ぶ経路を新しく足すときは、必ず
  `mojiroku_core::ffi_guard::guard` を通す。C++ 例外は Rust を素通りして**プロセスごと
  abort する**（v0.3.0 の実機クラッシュ 3 件の根本原因）。例外を `Err` に変えられるのは
  C++ 側の try/catch だけで、Rust の `catch_unwind` では捕まらない
  （[ADR-0021](./docs/decisions/ADR-0021_FFI例外シールドと重処理直列化.md)）。
- 重い ML ジョブ（文字起こし・話者分離・ローカル要約）を新しく足すときは、`HEAVY_ML_JOB`
  セマフォでアプリ全体 1 本に直列化する。並走させると 16GB 機でメモリが枯れ、
  クラッシュやスワップ固着になる。**取り方は経路で 2 つに分かれる。**
  Tauri コマンドから直接呼ぶなら `commands::acquire_heavy_job`（待ちを `stage="queued"` の
  進捗イベントで UI に通知する）。バックグラウンドワーカー（`src-tauri/src/jobs.rs`）からなら
  `commands::acquire_heavy_job_permit`（通知は呼び出し側が `job://update` で行うため、
  ヘルパは permit を返すだけ）。**経路に合わない方を使うと、通知が二重になるか消える。**

### 文字起こしの時刻

- whisper-rs の `state.full()` は whisper.cpp 内蔵の VAD を**バイパスする**。
  VAD を効かせるには `WhisperVadContext` で speech 区間を抜き、無音を除いた PCM を
  whisper に渡したうえで、**タイムスタンプを元の時刻へ再マッピングする**
  （[ADR-0008](./docs/decisions/ADR-0008_VADはwhisper内蔵Sileroを独立適用.md)）。
  再マッピングを省くと、除いた無音の長さぶん字幕がずれる。

### GitHub Actions（このリポジトリは public）

- `pull_request_target` を使わない。fork のコードを base の secrets 付きで動かすことになり、
  「fork PR から secrets に到達する経路が無い」という前提が壊れる。
- レビュー用ワークフロー（`claude-code-review.yml`）を変更したら、**同じ内容をデフォルト
  ブランチ（`main`）にも届ける**。`anthropics/claude-code-action` は、実行しようとした
  ワークフローファイルが default branch 上の版と**内容まで一致**していないと、警告を出して
  自ら終了する。`develop` 側だけ直すと、以後の全 PR が「job は success なのにレビュー 0 件」
  という**静かな無効化**に入る（2026-08-30 に実際に起きた）。
- **PR の required status check になりうるワークフロー**でジョブを条件付きに飛ばすときは、
  ワークフローレベルの `paths:` / `branches:` ではなく **job レベルの `if:`** で書く。
  前者でスキップされた check はそもそも報告されないため、PR が
  `Waiting for status to be reported` で永久にブロックされる。job レベルの `if:` で
  飛ばした job は success 扱いになる。
  なお push トリガーのデプロイ用ワークフロー（`deploy-landing.yml` など）はこの制約の
  対象外で、ワークフローレベルのフィルタを使ってよい。

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
