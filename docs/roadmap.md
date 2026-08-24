# mojiroku ロードマップ（北極星 → ベータ → 反復）

> 関連: [要件定義書](./requirements.md) ・ [仕様書](./spec.md) ・ [アーキテクチャ](./architecture.md) ・ [ADR](./decisions/)
> 本書は戦略レベルのゴール。機能の詳細は requirements/spec、技術判断は ADR を参照。

## 北極星

**ローカル完結・基本無料で「Notion AI ミーティングノート」の代替になるデスクトップ議事録アプリ。**
録音 → 文字起こし → 要約/議事録に加え、**MCP で Claude 等から自分の議事録を検索・参照**でき、
`mojiroku.com`（取得済み）から配布。友人・研究室で実用検証 → フィードバック → バージョンを上げていく。

- 維持費はローカル推論・静的ホスティング・ローカル MCP で **サーバー費 $0** を維持。
  唯一のランニングコストは Apple Developer Program **$99/年**（2026-07 加入。Developer ID 署名+公証を導入、
  [ADR-0022](./decisions/ADR-0022_AppleDeveloperID署名とnotarization.md)）。
  **自動更新は minisign で Apple 署名と独立に実装済み**（[ADR-0020](./decisions/ADR-0020_自動リリースパイプライン.md)）。
- 競合（Granola / Fireflies / Otter / tl;dv）に対し、武器は **「ローカル完結・基本無料・送信なし・機内モードでも全機能」**。
  クラウド型が課金・送信前提なのに対し、同等体験を端末内で出すことが差別化の核。

## ベータの定義（合意済みスコープ）

| 含む（ベータ） | 後回し（v1.x 以降） |
|---|---|
| マイク録音 + ファイル取込 → 文字起こし | 会議ライブ取込（Zoom/Meet システム音声）→ ⚠️ **下記 UI 刷新で前倒し検討中** |
| 要約/議事録（議事録・要約・アクションアイテム） | ~~署名/公証（$99、スケール時）~~ → **導入済み（2026-07, ADR-0022）**※自動更新は minisign で独立実装済（ADR-0020） |
| 話者分離（誰が発言したか, sherpa-onnx） | Windows 対応 |
| ローカル履歴・検索 | クラウド同期 |
| MCP サーバー（Claude 等から参照） | |

### 増分リリース（モノリシックに作らない）

- **beta-1（最小の有用ループ）**: マイク録音/ファイル → 文字起こし → 要約 → 履歴。「録って議事録が残る」が成立。
- **beta-2（同じ利用者へ追加）**: + 話者分離、+ MCP サーバー。

## UI 刷新（ダーク Studio デザイン）と競合対抗レイヤー（2026-06 デザイン確定）

Claude Design で全画面のリデザイン（**ダークテーマ / indigo ブランド / Noto Sans JP・DM Mono**）を確定。
このデザインは現行の実機能（取り込み・録音・要約・履歴・設定）を作り込むと同時に、
**競合対抗の新機能群を UI として先取り**している。ソースは `claude.ai/design` プロジェクト「Mojiroku UIデザイン検討」の
`Mojiroku 案B Studio.dc.html`（17 画面の完全 spec）と `Mojiroku Prototype.dc.html`（動く状態機械の雛形）。

### 実装方針

- 新デザインは **既存の実バックエンド配線（Tauri `invoke`/`event`）にそのまま統合**する。
  Prototype のモック状態機械を丸ごと移植しない（＝美しいだけで動かない偽アプリを避ける）。
- **外向き機能（AI に送る / 連携 / カレンダー）は「ユーザー操作時のみ送信」**。自動同期はしない。
  「送信はあなたの操作だけ」を全画面で守る（北極星の "送信なし" と両立させる唯一の形）。
- **フォントはローカル解決**（CDN 依存はオフライン訴求と矛盾）。日本語は macOS ネイティブの Hiragino Sans を主、
  DM Mono（数字・タイムスタンプ用・Latin のみで軽量）は `@fontsource` で同梱。
- ダーク固定（ライト/ダーク切替はデザインに無いので追加しない＝スコープ膨張を避ける）。

### 機能 → フェーズ マッピング

| デザイン画面 | 機能 | 競合対抗 | 実装状態 / フェーズ |
|---|---|---|---|
| 02 取り込み / 03 録音中 / 04 詳細 / 05 履歴 / 07 設定 | 既存実機能の新デザイン化 | — | **Phase 5（UI 刷新, 進行中）** |
| 01 初回オンボーディング（モデル DL 専用画面） | — | 初回 DX | **意図的に先送り**。専用画面は作らず、初回 DL 進捗は処理時の `ProcessingOverlay`（DL バー）に集約。専用オンボーディングが要るかはベータ反応で判断 |
| 06 テンプレ選択 / 10 AI に送る・コピー | テンプレ選択モーダル・Markdown 整形・ChatGPT/Claude で開く・MCP 向けコピー | 差別化の核（MCP） | **Phase 5（フロント完結, 本実装）** |
| 03 処理パイプライン可視化 | decode→transcribe→[diarization]→merge の進捗可視化 | 「ローカルで動いてる安心感」 | **Phase 5（実イベントに配線）** |
| 08 会議モード | システム音声ローカルキャプチャ + ライブ AI ノート | Granola / Fireflies | ⚠️ **要 ADR + スパイク**（macOS システム音声 API・TCC 権限・未署名挙動）。Phase 7 候補 |
| 09 話者ライブラリ | 端末内声紋で Meet/Zoom 参加者を自動識別 | Otter | 🚧 **スパイク完了（[ADR-0018](./decisions/ADR-0018_話者ライブラリの声紋照合.md) 方向性 go）**＝Phase 8 実装可（現行 TitaNet・サジェスト先行・τ 実運用較正）。 |
| 11 ミーティングに質問 | ローカル RAG・引用つきチャット | Otter Chat / AskFred | ⚠️ **要 ADR + スパイク**（埋め込み + 検索 + LLM sidecar）。Phase 9 候補 |
| 12 連携 | Google カレンダー / Notion / Slack / Obsidian 書き出し | Fireflies / tl;dv | Phase 6 候補（外向き・操作時送信） |
| 13 メニューバー常駐 | 常駐 + グローバルショートカット録音（⌥⌘R） | 即録音体験 | Phase 11 候補（Tauri tray / global-shortcut。TCC・未署名注記） |
| 14 多言語 / 翻訳字幕 | 言語自動検出・日本語訳・.srt | 多国籍チーム | Phase 10 候補 |
| 15 トピック自動チャプター | LLM による章立て | 長尺会議の閲覧性 | Phase 10 候補 |
| 16 横断ダイジェスト | 定例横断の決定事項/未完了アクション集計 | **独自価値**（ローカル RAG の応用） | Phase 9 候補（RAG 基盤を共有） |
| 17 オフライン完全動作 / 比較 | 機内モード訴求・対クラウド比較 | 訴求（マーケ） | ランディング / オンボーディングに反映 |

### モック先行（プレビュー）画面 — ⚠️ 配布前に要判断

合意により、**まだバックエンド未実装の機能画面はモックデータで作り込む**（デザイン全体像を先に可視化する選択）。
対象: **会議モード / 話者ライブラリ / 連携 / 横断ダイジェスト**（＋詳細内の翻訳トグル・自動チャプター・RAG 質問ドロワー）。
これらは極小の「プレビュー」マーカーを付け、**実機能と区別**する。

> **配布ゲートへの申し送り**: ベータは実会議の品質フィードバック収集が目的。動かないモック画面はテスターを誤認させ
> フィードバックを汚すリスクがある（合意の上での選択）。**友人・研究室へ配る版では、モック画面を「準備中」化 or 非表示に切り替える**
> 判断を配布直前に行う（プレビューマーカーと feature flag で容易に切替できる構成にしておく）。

## フェーズ順序と進捗

| フェーズ | 内容 | 状態 |
|---|---|---|
| scaffold | Tauri v2 + Vite/React + Rust ワークスペースの土台 | ✅ `9b03758` |
| **Phase 1a** | 音声ファイル → 文字起こし（whisper.cpp/Metal, 日本語） | ✅ `ccd20b9` |
| **Phase 1b** | 要約/議事録（llama.cpp ローカル sidecar + BYOK） | ✅ `f8f2127`（ADR-0007） |
| **VAD** | 無音ハルシネーション対策（whisper.cpp 内蔵 Silero） | ✅ `a53521f`（ADR-0008） |
| **Phase 1c** | マイク録音 + 永続化/履歴/検索（SQLite FTS5）→ **beta-1** | ✅ |
| **Phase 2** | 話者分離（sherpa-onnx, pyannote seg-3.0 ONNX）→ **beta-2** | ✅ `924ad81`（ADR-0009） |
| **Phase 3** | MCP サーバー（履歴DBに依存＝Phase 1c 後）→ **beta-2** | ✅ `c9b31a7`（ADR-0010） |
| **Phase 4** | `mojiroku.com` ランディング + ダウンロード配布 | ✅ 公開（ADR-0011。当初は未署名 .dmg → 2026-07 に Developer ID 署名+公証へ移行・ADR-0022） |
| **Phase 5** | **UI 刷新（ダーク Studio）**＋ AI に送る/コピー（フロント完結）＋ 処理可視化＋未実装機能のモック先行＋**アプリ内アップデート（Tauri v2 updater）/ 自動リリースCI（ADR-0020）** | 🚧 **進行中**（UI 刷新）。**アプリ内更新は完了**: 自動リリースパイプライン（main の version 上げ→ビルド/署名検証/公開 `mojiroku-releases`/Worker プロキシ動的配信）で **v0.3.0 を公開し 0.2.0→0.3.0 のアプリ内更新を実機 E2E 実証（2026-06-30）**。**v0.4.0（2026-07-03, Apple 署名+公証入り）で publish まで含む完全自動リリースを実証**（[ADR-0020](./decisions/ADR-0020_自動リリースパイプライン.md)/[ADR-0022](./decisions/ADR-0022_AppleDeveloperID署名とnotarization.md)）。 |
| **i18n（前倒し）** | **サイト・アプリの日英2言語対応**（Cloudflare Analytics で米国・カナダからのアクセス増を受け、Phase 10「翻訳字幕」とは別に UI/コンテンツの言語対応を前倒し） | ✅ **2026-07-04 develop マージ済み**: ①landing `/en/`（Astro 組み込み i18n・hreflang・og.en.png。ja は `/` 不変＝焼き付き URL 維持）②アプリ UI = 型付き辞書（`frontend/src/i18n`、ja が形の正・en は tsc 検証）③コンテンツ言語 = settings.language が要約出力/話者ラベル/エクスポート見出しを、transcribe_language が whisper 言語（既定アプリ言語追従・auto 可）を決定。Qwen2.5-7B の英語要約品質はスパイクで PASS（sidecar `--lang`）④主要エラー 29 件を `error.*` キー化（未知キーは原文フォールバック）。**出荷済み**で、EN landing（`landing/src/pages/en`）も公開中。「アプリが日本語のみのうちに英語 LP を出さない」という順序制約は解消済み。 |
| **Phase 6** | 連携 / 書き出し（Notion / Slack / Obsidian / Google カレンダー、外向き・操作時送信） | 🚧 **進行中**: ①設定永続化（settings.json）+ BYOK キーをキーチェーン保管 + 要約のローカル/クラウド分岐を実配線（ADR-0012）。②ローカル/Obsidian 実ファイル書き出し（frontmatter ノート / .md / .txt / .srt）完了。③Notion 書き出し（内部トークン BYOK・page 親・コア exporter、ADR-0013。実トークン疎通確認済み）完了。④Slack 送信（Incoming Webhook BYOK・要約のみ・mrkdwn 変換、ADR-0014。実 webhook 投稿を実機確認済み）完了。⑤PDF 書き出し（ブラウザ印刷＝トップレベル window.print + capability `core:webview:allow-print` + `@media print` content-only、CJK はシステムフォント・$0・選択可テキスト、ADR-0015。実 PDF を目視検証済み）完了。⑥カレンダー取り込み（限定公開 iCal URL = 秘密 URL を貼るだけ・読み取り専用・$0・OAuth 不要、RRULE は DAILY/WEEKLY を壁時計展開、予定タイトルで「記録を準備」、ADR-0016。fixture テスト緑・実フィードで品質確認は配布ゲート）完了。⑦**連携の OAuth ワンクリック化（Slack/Google/Notion、ADR-0019）**: トークン貼付の摩擦を解消し「◯◯と連携」ボタン化。共通 loopback+PKCE 基盤＋Slack/Notion は mojiroku.com Worker ブローカー・Google は Desktop PKCE 直接（iCal→Calendar API、iCal はフォールバック維持）。維持費 $0 のまま。**3連携とも実機 E2E 成功**。**Phase 6 完了** |
| **Phase 7** | 会議モード（システム音声ローカルキャプチャ + ライブノート） | 🚧 **スパイク完了→実装可（ADR-0017 採用）**: ScreenCaptureKit インプロセス Rust（`screencapturekit` v8）。実機 macOS 26.5 で**未署名(ad-hoc) .app でシステム音声キャプチャ成立**を実証（48kHz 連続・非無音）。更新(再ビルド=新cdhash)後は**サイレント拒否でなく `get()` 明示エラー＝検出可能な再プロンプト**（GUI で remove/re-add 要・中程度の摩擦）。→ **2026-07 に Developer ID 署名を導入（ADR-0022）**: 安定 DR により更新を跨いで TCC 許可が永続し、この摩擦は解消見込み。残: 実通話互換・mic+system デュアルトラックのクロックドリフト・Swift ランタイム同梱は実装時。スパイクは `spikes/meeting-audio/` |
| **Phase 8** | 話者ライブラリ（端末内声紋識別） | ✅ **出荷済み（[ADR-0018](./decisions/ADR-0018_話者ライブラリの声紋照合.md)）**: ①声紋を `DiarizationResult` に露出（consolidation 既計算・再抽出なし）②store v4（speaker_embeddings/library/matches・cosine 1:N・leave-one-recording-out・最小エンロール尺ゲート）③Tauri コマンド＋3保存経路に声紋永続化④SpeakersView 実 CRUD＋詳細でサジェスト照合（τ 非自動確定＝サジェスト先行）。基盤=スパイク実証（TitaNet 0.926 vs 別人 ≤0.61・新規 DL 不要）。⚠️ **実機の UI クリック経路での確認は未実施**・τ/最小尺は実運用較正。 |
| **話者訂正** | 発言単位で話者を付け直し、訂正を精度改善に還元する（Issue #19） | 🚧 **増分1 出荷（v0.5.2）**: 発言の話者チップを押して付け直せる。訂正は `segments.speaker_id` に入り、同値の選び直しは要約を stale にしない。再分離で訂正が消えるのを防ぐのは UI の `canDiarize`（[docs/spec.md](./spec.md) §9.1）。増分2（訂正の蓄積を精度へ還元）・増分3 は未着手。 |
| **Phase 9** | ミーティングに質問（ローカル RAG）＋ 横断ダイジェスト | ⬜ ⚠️ ADR + スパイク先行 |
| **Phase 10** | 多言語 / 翻訳字幕 ・ トピック自動チャプター | ⬜ |
| **Phase 11** | メニューバー常駐 + グローバルショートカット録音 | ⬜（TCC 挙動を確認。署名は導入済み・ADR-0022） |
| **継続** | フィードバック→反復、その先で Windows | ⬜ |

> **順序は確定でない**。Phase 6〜11 の優先度は、ベータのフィードバックと下記スパイクの結果で並べ替える。
> 特に **会議モード（Phase 7）はもともと "v1.x 後回し" だったものをデザインが前倒し**しているため、
> 「友人・研究室での実会議キャプチャ需要」と「実装リスク（macOS システム音声 + 未署名 TCC）」を天秤にかけて優先度を決める。

## 必須ゲート（飛ばさない）

- **🎯 品質ゲート**: 実在の複数話者・日本語会議音声で要約品質を実機評価。→ **実施済み: PASS**（Qwen2.5-7B Q4_K_M）。
- **📦 配布ゲート**: `.dmg` をクリーンな Mac で隔離属性付きで開けるか。
  → 未署名時代の実機確認（macOS 26, 2026-06-27）: 初回「damaged」表示だが `xattr -dr com.apple.quarantine` で起動可
  （[ADR-0011](./decisions/ADR-0011_配布は未署名dmgでCloudflareとReleases.md)）。
  → **2026-07 に Developer ID 署名+notarization を導入（[ADR-0022](./decisions/ADR-0022_AppleDeveloperID署名とnotarization.md)）**:
  以後の配布ゲートは「quarantine 付き DL を**警告なしダブルクリック起動**できるか」。署名版の実機確認はリリース時に実施。
- **🔬 実現性スパイク（Phase 7〜9 に着手する前）**: 性能・互換性の前提を**断定前に裏取り**（過去の faster-whisper × Metal 非対応の教訓）。
  - **会議モード**: ✅ **スパイク完了（2026-06-27, [ADR-0017](./decisions/ADR-0017_会議モードのシステム音声キャプチャ.md) 採用）**。ScreenCaptureKit（`screencapturekit` v8 インプロセス Rust）採用、Core Audio tap は不要。
    実機 macOS 26.5 で**未署名(ad-hoc) .app でシステム音声キャプチャ成立**・更新後は**サイレント拒否でなく検出可能な再プロンプト**（GUI remove/re-add 要）を実証。→ その後 Developer ID 署名を導入（ADR-0022）し TCC 摩擦は解消見込み。スパイク: `spikes/meeting-audio/`。
    ⚠️ **de-risked は「単一ソース（システム音声）の取得」まで**。**mic+system 同時キャプチャの 2 クロックドリフト**（60 分会議の整合）は**未計測の第2リスク**で、実装初期にホスト時刻スタンプで早期に裏取りする（"実装可" は同時取得の保証ではない）。
  - **話者ライブラリ**: ✅ **スパイク完了（2026-06-28, [ADR-0018](./decisions/ADR-0018_話者ライブラリの声紋照合.md) 採用＝方向性 go）**。別会議の同一人物を埋め込み cosine で 1:N 照合（実音声2本・現行 TitaNet 最良 0.926 vs 別人 ≤0.61・新規 DL 不要）。脆さは音声量＝**最小エンロール秒数ゲート**が設計レバー。⚠️ 独立クロス正例 n=1・同一デバイス＝**定量ゲートは統計的に未達・τ は実運用較正**（サジェスト先行で実装）。
  - **ローカル RAG**: 埋め込みモデル + 検索 + LLM sidecar の構成と引用精度を、whisper/llama の ggml 衝突制約（[ADR-0007](./decisions/ADR-0007_要約llamaを別バイナリsidecarに分離.md)）と矛盾しない形で検証。
  - いずれも**日付を確約せず**、スパイク → ADR → 実装の順で進める。

## 主要な決定（ADR）

- MCP は**早期**（差別化の核）。ただし履歴DB（Phase 1c）に依存するので順序はその後。
- ローカル要約 llama.cpp は **whisper.cpp と ggml 衝突**するため別バイナリ sidecar に分離（[ADR-0007](./decisions/ADR-0007_要約llamaを別バイナリsidecarに分離.md)）。
- VAD は whisper-rs の `state.full()` が内蔵VADをバイパスするため `WhisperVadContext` で独立適用（[ADR-0008](./decisions/ADR-0008_VADはwhisper内蔵Sileroを独立適用.md)）。
- フロントは Vite、ML は Rust 単一ランタイム（[ADR-0003](./decisions/ADR-0003_MLをRust単一ランタイムに集約.md) / [0006](./decisions/ADR-0006_フロントはViteでNextは不採用.md)）。
- 自動リリース/アプリ内アップデートは GitHub Actions（gate=ubuntu → build-publish=macos-26）+ mojiroku.com Worker プロキシで $0 維持（[ADR-0020](./decisions/ADR-0020_自動リリースパイプライン.md)・v0.3.0 で稼働実証）。
- **起票済み ADR（スパイク後）**: 会議モードのシステム音声方式（[ADR-0017](./decisions/ADR-0017_会議モードのシステム音声キャプチャ.md)）/ 話者ライブラリの声紋方式（[ADR-0018](./decisions/ADR-0018_話者ライブラリの声紋照合.md)）。
- **未起票の ADR（スパイク後に書く）**: ローカル RAG 構成 / 外向き連携の認証・送信境界。
