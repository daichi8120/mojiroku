# 0012. 設定の永続化 + BYOK シークレットのキーチェーン保管 + 要約のローカル/クラウド分岐

- ステータス: 採用
- 日付: 2026-06-27

## Context

Phase 6（連携/書き出し）の土台。Notion・Slack・BYOK クラウド要約はいずれも「**設定の永続化**」と
「**シークレット（トークン/API キー）の安全な保管**」を共通の前提とする。これまで設定は UI の local state
のみで、再起動で初期化されていた（`SettingsView` 冒頭コメント参照）。`docs/spec.md` §8 は当初から
**`keyring`（OS キーチェーン）に BYOK キーを置く**方針を記していたが未実装だった。

論点は3つ:
1. 非機密設定（要約エンジン選択・プロバイダ・モデル・プライバシートグル）を**どこに永続化するか**。
2. 機密（BYOK API キー）を**どう保管し、誰がいつ読むか**。
3. 要約のローカル/クラウド分岐を**どの層で行うか**（ローカル sidecar 経路を壊さずに）。

確定済みの裏取り:
- `dialog:default` capability は `allow-save` を含む（ファイル保存は別 capability 不要）。設定 JSON も
  **Rust の `std::fs` で直接書けば** JS の fs プラグイン/capability は不要（プラグイン権限は JS 側 API のみを律する）。
- `keyring` v3 は macOS で `apple-native` バックエンド（security-framework 経由の login キーチェーン）。
  追加の plugin/capability は不要（crate 直依存）。
- `crates/mojiroku-core` の要約は `SummarizeProvider` トレイトを持ち、BYOK 実装（`byok.rs`）は
  `summarize(transcript, template)` を実装済み。コマンド層から provider を選んで呼ぶだけで配線できる。
- **Anthropic の `claude-3-5-sonnet` 系は 2025-10-28 に提供終了**（API は 404）。BYOK 既定モデルに使うと
  既定構成のクラウド要約が必ず失敗する。

## Decision

**設定は `settings.json`、シークレットはキーチェーン、要約分岐はコマンド層**で行う。

- **非機密設定 = `app_data_dir/settings.json`**（`serde_json`、temp→rename の原子的書き込み）。
  DB・モデルと同じ親ディレクトリ。`Settings { engine, provider, model, save_recordings, send_usage }`。
  per-field `serde(default)` で古い JSON でも安全に既定へ倒す。**capability 追加なし**（Rust 直書き）。
- **機密 = OS キーチェーン（`keyring`, apple-native）**。account は **provider 別**（`byok_api_key_<provider>`）。
  別 provider の鍵を取り違えて送らないため。
  - `get` は**コマンド化しない**（鍵を webview へ往復させない）。要約コマンドが Rust 内で直読する。
  - `set`/`delete`/`has` のみコマンド公開。いずれも `#[tauri::command(async)]` でスレッドプール実行し、
    キーチェーンの許可ダイアログ中にメインスレッド（イベントループ）を止めない。
  - **失敗時に平文フォールバックしない**（エラーで止める）。
- **要約のローカル/クラウド分岐 = コマンド層（`summarize`）**。`settings.engine` を読み、`cloud` なら
  キーチェーンの鍵で `byok.rs` の summarizer を `spawn_blocking` で実行。**ローカル sidecar 経路は不変
  （純加算ブランチ）**。鍵取得（ダイアログでブロックし得る）も `spawn_blocking` 内に置く。
- **既定モデルは編集可能**。Anthropic 既定は `claude-sonnet-4-6`（3-5-sonnet 系は提供終了）。`model` 空欄なら
  provider 既定へ解決し、**空文字を API に送らない**。
- **送信の透明性**: クラウド（BYOK）要約では文字起こし内容が外部プロバイダへ送信される。これを
  設定画面のプライバシーバナー（engine 連動）・BYOK 設定欄・**生成モーダル（`TemplateModal`）**で明示する。
  `TemplateModal` は実 `settings.engine` を読み取り専用で反映し、ローカル時のみ「送信なし」と表示する。

## Consequences / リスク

- ⚠️ **未署名×キーチェーン（load-bearing）**。アドホック/未署名 .app ではキーチェーンアクセス時に許可
  ダイアログが出る（**dev はビルド毎にバイナリ同一性が変わり再プロンプトされ得る**＝macOS の仕様であり不具合
  ではない）。**2026-06-27、`just dev`（未署名）実機で BYOK キー保存→Anthropic クラウド要約の生成成功を確認**
  （キーチェーン保存/読取・settings 永続化とも動作。`claude-sonnet-4-6` も当該アカウントで疎通）。ただし
  **配布版（.dmg の安定アドホック署名）での挙動は dev とは別物**なので**配布ゲートで再確認**する
  （[[verify-load-bearing-assumptions]]）。既定モデルの可用性はアカウント依存・UI で編集可能。
- BYOK 利用時はデータが端末外へ出る（プライバシーのトレードオフ）。既定は常に `local`。
- エラーの可読性のため、BYOK の HTTP エラーはレスポンスボディ（プロバイダの error.message）を表面化し、
  200 でも content 空なら成功扱いにせずエラーにする（静かな失敗の防止）。
- **申し送り（本 ADR の範囲外）**:
  - `save_recordings`/`send_usage` は**値の保存のみで挙動未配線**（UI で明示）。録音保存停止・テレメトリは近日。
  - Notion・Slack 連携は本基盤（設定 + キーチェーン）の上に別 ADR で実装。
  - 設定の自由入力（model）は毎キーストローク保存。デバウンスは未対応（実害は小）。

関連: [[ADR-0007_要約llamaを別バイナリsidecarに分離]]（要約 sidecar 分離）, `docs/spec.md` §8（BYOK/keyring）。
