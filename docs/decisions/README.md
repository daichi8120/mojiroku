# 意思決定

このディレクトリは、ADR と採用判断、不採用判断を記録するための場所です。

検証や設計の結果として最終判断が確定したものを配置し、背景と理由を後から追えるようにします。

## 配置方針

- 最終的な採用判断、不採用判断を置く
- 背景、比較案、判断理由、影響が追える形で残す
- 詳細な比較・検証の結果は該当 ADR（例: [`ADR-0009_話者分離スパイク結果.md`](ADR-0009_話者分離スパイク結果.md)）に併記する
- 更新運用（Tauri v2 updater）の手順は [`updater-plan.md`](../updater-plan.md)

## 命名の目安

- `ADR-NNNN_タイトル.md`（番号は 4 桁。既存 ID 表記に揃える）

## 現在の ADR

- [`ADR-0001_アーキテクチャ決定記録を残す.md`](ADR-0001_アーキテクチャ決定記録を残す.md) — 重要な技術判断を ADR として時系列で記録する方針
- [`ADR-0002_デスクトップ基盤にTauri-v2を採用.md`](ADR-0002_デスクトップ基盤にTauri-v2を採用.md) — ローカル ML・$0 維持費・ネイティブ高速化のため Tauri v2
- [`ADR-0003_MLをRust単一ランタイムに集約.md`](ADR-0003_MLをRust単一ランタイムに集約.md) — PyInstaller/torch を避け全 ML を Rust in-process で実装（案B）
- [`ADR-0004_話者分離はsherpa-onnxで実現.md`](ADR-0004_話者分離はsherpa-onnxで実現.md) — pyannote seg-3.0 ONNX 重み + sherpa-onnx で torch なし話者分離
- [`ADR-0005_STTエンジンにwhisper-cppを採用.md`](ADR-0005_STTエンジンにwhisper-cppを採用.md) — faster-whisper が Metal 非対応のため STT は whisper.cpp
- [`ADR-0006_フロントはViteでNextは不採用.md`](ADR-0006_フロントはViteでNextは不採用.md) — Tauri 静的出力向けに Next.js を外し Vite + React
- [`ADR-0007_要約llamaを別バイナリsidecarに分離.md`](ADR-0007_要約llamaを別バイナリsidecarに分離.md) — whisper.cpp と llama.cpp の ggml 衝突回避で要約を別バイナリ化
- [`ADR-0008_VADはwhisper内蔵Sileroを独立適用.md`](ADR-0008_VADはwhisper内蔵Sileroを独立適用.md) — `state.full()` が内蔵 VAD をバイパスするため独立適用
- [`ADR-0009_話者分離スパイク結果.md`](ADR-0009_話者分離スパイク結果.md) — 日本語会議で分離可能・consolidation で purity 改善 → sherpa-onnx で go（PoC）
- [`ADR-0010_ローカルMCPサーバーをstdio別バイナリで提供.md`](ADR-0010_ローカルMCPサーバーをstdio別バイナリで提供.md) — 履歴 DB を読み取り専用・stdio 別バイナリで Claude から検索参照
- [`ADR-0011_配布は未署名dmgでCloudflareとReleases.md`](ADR-0011_配布は未署名dmgでCloudflareとReleases.md) — 未署名 .dmg + Cloudflare + GitHub Releases で $0 配布
- [`ADR-0012_設定永続化とBYOKキーチェーンと要約分岐.md`](ADR-0012_設定永続化とBYOKキーチェーンと要約分岐.md) — settings.json + OS キーチェーン + 要約のローカル/クラウド分岐
- [`ADR-0013_Notion書き出し.md`](ADR-0013_Notion書き出し.md) — 内部トークン BYOK + page 親で議事録作成（認証は 0019 で OAuth 化）
- [`ADR-0014_Slack送信.md`](ADR-0014_Slack送信.md) — Incoming Webhook で要約のみ投稿・mrkdwn 変換（認証は 0019 で OAuth 化）
- [`ADR-0015_PDF書き出し.md`](ADR-0015_PDF書き出し.md) — window.print + capability + @media print で選択可テキスト PDF
- [`ADR-0016_カレンダー取り込み.md`](ADR-0016_カレンダー取り込み.md) — 限定公開 iCal URL で次の予定取り込み（取得は 0019 で Google OAuth へ）
- [`ADR-0017_会議モードのシステム音声キャプチャ.md`](ADR-0017_会議モードのシステム音声キャプチャ.md) — ScreenCaptureKit でシステム音声キャプチャ（実機 26.5 で go）
- [`ADR-0018_話者ライブラリの声紋照合.md`](ADR-0018_話者ライブラリの声紋照合.md) — TitaNet 埋め込みでクロス会議照合（方向性 go・τ は実運用較正）
- [`ADR-0019_連携のOAuthワンクリック化.md`](ADR-0019_連携のOAuthワンクリック化.md) — loopback PKCE + Worker ブローカーで連携をワンクリック化
- [`ADR-0020_自動リリースパイプライン.md`](ADR-0020_自動リリースパイプライン.md) — `version` バンプ→CI が .app/.dmg をビルド→署名検証→公開 Releases へ自動リリース
- [`ADR-0021_FFI例外シールドと重処理直列化.md`](ADR-0021_FFI例外シールドと重処理直列化.md) — C++ 例外を境界で Err 化（クラッシュ根絶）＋重い ML ジョブを 1 本に直列化（メモリ枯渇予防）
- [`ADR-0022_AppleDeveloperID署名とnotarization.md`](ADR-0022_AppleDeveloperID署名とnotarization.md) — 配布に Developer ID 署名+notarization（CI env 駆動・quarantine 付きでも警告なし起動）
- [`ADR-0023_録音PCMのspool化.md`](ADR-0023_録音PCMのspool化.md) — キャプチャ中に WAV へ逐次書き出し（長時間会議のメモリを ~13MB/トラックで一定に）
- [`ADR-0024_バックグラウンドジョブ基盤.md`](ADR-0024_バックグラウンドジョブ基盤.md) — 永続ジョブキュー（v5）でキャプチャと重い処理を分離（後付け話者分離/録音のみ→後処理/並行録音・MCP は v5 core から再ビルド）
- [`ADR-0025_リモートMCPをOAuthゲートウェイとTunnelで公開.md`](ADR-0025_リモートMCPをOAuthゲートウェイとTunnelで公開.md) — claude.ai から議事録を引けるよう mojiroku-mcp に HTTP モードを足し、OAuth ゲートウェイ（Cloudflare Worker）+ Tunnel で $0 公開（読み取り専用・単一ユーザーのパスフレーズ同意）
- [`ADR-0026_会議開始の自動録音プロンプト.md`](ADR-0026_会議開始の自動録音プロンプト.md) — 会議開始時に録音を促す。増分1=カレンダー開始時刻トリガー＋メニューバー常駐＋通知→アプリ内確認（macOS はアクションボタン非対応）。検知は増分2。prompt-only（自動録音しない）
- [`ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md`](ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md) — オープンソース化。AGPL-3.0-or-later + 許諾型 CLA（ADR-0011 の独自 EULA を supersede）。v0.6.0 以降が AGPL・v0.5.1 以前は旧 EULA。source-available はデスクトップで空文化するため不採用
- [`ADR-0028_話者分離segmentationをpyannoteに差し替え.md`](ADR-0028_話者分離segmentationをpyannoteに差し替え.md) — segmentation の reverb-diarization-v1 が Rev Model Non-Production License（Outputs にも及ぶ）と判明 → pyannote-segmentation-3.0（MIT, CNRS）に差し替え。ADR-0009 の「被覆」指標は過剰検出を報奨していた（recall −3.4pt / precision +29pt）。出荷ゲートの purity 再スパイクは 2026-08-21 に実施し PASS（誤帰属は reverb の半分以下・製品粒度では同等以上。ただしフレーム単位では reverb が 5pt 上）。再現資材は `eval/diarization/`
- [`ADR-0029_プロンプトはモデル自身のchatテンプレートで組む.md`](ADR-0029_プロンプトはモデル自身のchatテンプレートで組む.md) — 要約 sidecar が Qwen2.5 の ChatML を直書きしていたのをやめ、GGUF に焼かれた chat テンプレートを使う。非 Qwen 系は `<|im_end|>` を特殊トークンとして持たないため文字列として出力し生成が止まらなかった（実測: 非 Qwen 5 本すべてで漏れ / Qwen 6 本は 0 件）。**Qwen2.5 では旧実装と出力が完全一致**（GGUF のテンプレートが手書き ChatML と同一）。BOS はテンプレート次第なので決め打ちしない
- [`ADR-0030_要約モデルを端末のメモリで段分けする.md`](ADR-0030_要約モデルを端末のメモリで段分けする.md) — 要約モデルを搭載メモリで段分けする骨組み。1 行 1 モデルのカタログ（DL 元・ハッシュ・サイズ・段・採用可否）に集約し、**採用済みが無い段は既定へ落ちる**ので配るモデルは現行のまま。見るのは搭載メモリ（空きは起動ごとに変わり判定が安定しない）、分からなければ小さい方へ倒す（重すぎ＝クラッシュしうる / 軽すぎ＝質が落ちるだけ）。手元にあるモデルは黙って置き換えない（キャッシュ優先をテストで固定）。境界の値と品質ゲートは未測定
