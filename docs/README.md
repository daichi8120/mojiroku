# docs

`docs/` は、このプロジェクトの文書を整理するためのディレクトリです。
要件、設計、運用ルール、意思決定など、継続的に参照したい文書をここに配置します。

## ファイル構成

`docs/` 直下にトピックごとの文書を置き、確定した技術判断（ADR）と更新運用は `decisions/` にまとめます。

- `roadmap.md`: 背景・目的・全体像・ロードマップ（北極星・ベータ・ゲート。進捗の正）
- `requirements.md`: 課題整理・要件定義
- `spec.md`: 仕様（機能・非機能・宿題の §）
- `architecture.md`: 構成設計・データフロー・コンポーネント責務
- `CONTRIBUTING.md`: 開発フロー・ブランチ運用・コミット/タグ規約
- `install-macos.md`: .dmg の macOS インストール手順（v0.4.0 以降は署名+公証済み）
- `updater-plan.md`: アプリ内アップデート（Tauri v2 updater）の運用・Secrets・E2E
- `mcp.md`: ローカル MCP サーバーの参照（設定・公開スキーマ）
- `decisions/`: ADR（採用/不採用判断。`ADR-NNNN_*.md`）

## どこに何を置くか（3層モデル）

mojiroku の文書・データは **GitHub repo（ここ）/ GitHub Issues・Projects / ローカル＆配布物**
の3層に分かれる。「ある文書/データをどこに置くか」を迷ったら、まずタイブレーカーで判定する。

> **タイブレーカー（迷ったらこの順で判定）**
> 1. **小さく diff できる**テキスト/設定・**確定した**設計や判断 → **repo**（この `docs/` かコード）
> 2. **大容量バイナリ**（モデル・配布物）→ **repo に置かない**（実行時 DL / ビルド成果物 / 公開 Releases）
> 3. **これからやること・進行中の議論** → **GitHub Issues / Projects**

### 配置の決定表

| 内容 | 正本（置き場所） | 補足 |
|---|---|---|
| コード・設定（`crates/` `frontend/` `src-tauri/` `landing/`・`tauri.conf.json` 等） | repo | |
| 背景・目的・ロードマップ | repo `docs/roadmap.md` | 進捗の正 |
| 要件定義 | repo `docs/requirements.md` | |
| 仕様・アーキ設計 | repo `docs/spec.md` / `docs/architecture.md` | コードに同期する分 |
| 運用手順・配布・更新・コントリビュート | repo `docs/CONTRIBUTING.md` / `docs/install-macos.md` / `docs/updater-plan.md` | |
| 技術判断（ADR・commit に紐づく確定判断） | repo `docs/decisions/` | `ADR-NNNN_*.md` |
| 品質ゲートの評価ハーネス（話者分離の A/B 等） | repo `eval/` | 例: `eval/diarization/`。**音声・モデル・生成物は含めない**（結果の正本は ADR） |
| 用語・参照・補助情報（MCP 等） | repo `docs/mcp.md` | |
| ライセンス・帰属表示 | repo `LICENSE` / `NOTICE` / `CLA.md` | [ADR-0027](decisions/ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md) |
| タスク・バグ・機能要望 | **GitHub Issues** | 横断ビューは GitHub Projects |
| 進行中の設計議論 | **GitHub Issues / PR** | 確定したら ADR か `docs/` に落とす |
| 推論モデル（whisper / 要約 / VAD / 話者分離） | **実行時 DL**（`~/Library/Application Support/com.daichi0812.mojiroku/models/`） | `*.gguf` `*.bin` `*.onnx` は gitignore |
| sidecar / `.app` / `.dmg` | **ビルド成果物 / 公開 Releases** | `src-tauri/binaries/` は gitignore。配布は `mojiroku-releases` |

> 確定前の走り書きや個人的な作業メモは repo に持ち込まない。**確定した判断だけ**が
> ADR か `docs/` に降りてくる。

## 用語

- **dev-hub**: 作者の個人リポジトリ群の呼称。`docs/requirements.md` と初期の ADR
  （0001 / 0002 / 0003 / 0006）で「他プロダクトの前例」を指す文脈に出てくる。
  mojiroku 自体の構成要素ではない。
