# mojiroku

**文字起こし → 要約・議事録作成を「基本無料」で行うデスクトップアプリ。**
ローカルモデルで推論を完結させ、サーバー維持費 $0・プライバシー優先を実現する。

> 表示名・ロゴは検討中（`文字録` 等の漢字表記も候補）。技術名・ディレクトリ名は `mojiroku`。

---

## これは何

録音や音声ファイルを入れると、**話者付きの文字起こし**と**整った議事録**が、追加費用なし・
インストール後すぐに得られる。要約は既定でローカル（同梱モデル）。品質を求める場合のみ
自分の API キー（BYOK）でクラウド LLM を使える。

## ダウンロード / インストール

- **配布ページ**: [`mojiroku.com`](https://mojiroku.com)（→ `mojiroku.com/download` から最新 `.dmg`）
- Apple Silicon Mac（M1 以降）/ macOS 11 以降。**Apple Developer ID 署名 + 公証（notarization）済み**のため、
  ダウンロードしてそのまま開けます（v0.4.0 以降）。手順は [docs/install-macos.md](./docs/install-macos.md) を参照
  （[ADR-0022](./docs/decisions/ADR-0022_AppleDeveloperID署名とnotarization.md)）。
- ランディングのソースは [`landing/`](./landing/)（Astro / Cloudflare Workers 静的アセット）。

## アーキテクチャ（案B: Rust 単一ランタイム）

- **デスクトップ**: Tauri v2（[ADR-0002](./docs/decisions/ADR-0002_デスクトップ基盤にTauri-v2を採用.md)）
- **フロント**: Vite + React + TS + Tailwind v4（[ADR-0006](./docs/decisions/ADR-0006_フロントはViteでNextは不採用.md)）。状態は素の React state（zustand/shadcn は未導入）
- **ML**: すべて Rust ネイティブで in-process 実行。**Python / サイドカー / localhost HTTP なし**（[ADR-0003](./docs/decisions/ADR-0003_MLをRust単一ランタイムに集約.md)）
  - STT: whisper.cpp / `whisper-rs`（Core ML/Metal、[ADR-0005](./docs/decisions/ADR-0005_STTエンジンにwhisper-cppを採用.md)）
  - 話者分離: sherpa-onnx（pyannote seg-3.0 の ONNX 重み、torch なし。[ADR-0004](./docs/decisions/ADR-0004_話者分離はsherpa-onnxで実現.md)）
  - 要約: llama.cpp / `llama-cpp-2`（GGUF）＋ BYOK（OpenAI/Anthropic）

```
frontend/            Vite + React UI（Tauri invoke/event でコアと通信）
src-tauri/           Tauri v2 シェル（ウィンドウ/権限/音声取り込み/配布）
crates/mojiroku-core/ ML パイプライン（STT/話者分離/要約、UI 非依存）
docs/                フラット構成（roadmap/requirements/spec/architecture/運用・decisions/=ADR）
```

## ドキュメント

- [ロードマップ（北極星・ベータ・ゲート）](./docs/roadmap.md)
- [要件定義書](./docs/requirements.md)
- [仕様書](./docs/spec.md)
- [アーキテクチャ](./docs/architecture.md)
- [ADR](./docs/decisions/)
- [コントリビューションガイド](./docs/CONTRIBUTING.md)
- [CLAUDE.md（開発ガイド）](./CLAUDE.md)
- [AGENTS.md（AI エージェント向け共通入口・文書/データの置き場ルール）](./AGENTS.md)
- [docs/（どこに何を置くか・3層モデル）](./docs/README.md)

## ステータス

文字起こし（whisper.cpp/Metal）→ 要約・議事録（ローカル llama.cpp sidecar + BYOK）→ VAD → マイク録音 +
履歴/検索（beta-1）→ 話者分離（beta-2）→ MCP サーバー → **配布（Phase 4）まで完了**。`mojiroku.com` から
署名・公証済み `.dmg` を公開中。次はベータを配って実会議で品質評価 → 反復。現在地と今後は [docs/roadmap.md](./docs/roadmap.md) を参照。

### セットアップ & 実行

```bash
# 依存インストール（ルート = Tauri CLI、frontend = Vite アプリ）
npm install
npm --prefix frontend install

# 開発起動（ウィンドウが立ち上がる）
npm run dev          # = tauri dev

# 配布バンドル（ローカルは無署名。配布用の署名+公証はリリース CI が行う）
npm run build        # = tauri build

# フロントのみ / Rust のみ
npm --prefix frontend run build   # → frontend/dist
cargo build                       # ワークスペース
cargo test --workspace
```

> `just` を入れていれば `just dev` / `just build` / `just test` でも可（`brew install just`）。

## 開発フェーズ

フェーズ定義・状態の正は [docs/roadmap.md](./docs/roadmap.md)（フェーズ番号もそちらに準拠）。
2026-06 時点: scaffold → 文字起こし → 要約 / VAD → 録音 + 履歴/検索（beta-1）→ 話者分離 → MCP（beta-2）→
配布（Phase 4, `mojiroku.com` 公開）まで完了。2026-07 に Apple Developer ID 署名 + 公証を導入（ADR-0022）。
次はベータ運用 → 反復。

## ライセンス

**[AGPL-3.0-or-later](./LICENSE)**（GNU Affero General Public License v3.0 以降）。

「ローカル完結・送信なし」という mojiroku の主張は、ソースを読めなければ検証できません。
検証可能であること自体が製品価値なので、コピーレフトのライセンスを選んでいます。
判断の経緯は [ADR-0027](./docs/decisions/ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md) にあります。

- 同梱・依存する第三者ソフトウェアの帰属表示は [NOTICE](./NOTICE) を参照してください。
- コントリビュートには [CLA](./CLA.md) への同意が必要です。詳細は
  [docs/CONTRIBUTING.md](./docs/CONTRIBUTING.md)。
- **配布バイナリは v0.6.0 以降が AGPL-3.0 です。** v0.5.1 以前は旧 EULA
  （バイナリのみ配布・再配布不可）で提供されていました。
