# 0014. Slack 送信（Incoming Webhook = BYOK・要約のみ・コア exporter）

- ステータス: 採用（**認証方式は [[ADR-0019_連携のOAuthワンクリック化]] で OAuth に置換**。Webhook URL は OAuth で取得するようになったが、exporter／mrkdwn 変換／`slack.rs` は不変で引き続き有効）
- 日付: 2026-06-27

## Context

Phase 6（連携/書き出し）の 2 つ目の外部送信スライス。[[ADR-0013_Notion書き出し]] と同じ
キーチェーン + コア exporter 基盤の上に、要約を **Slack チャンネルへ投稿**する。論点:

1. **認証方式**: Slack 投稿には (a) Incoming Webhook（チャンネル固定・URL 自体が秘密・OAuth 不要）と
   (b) Bot トークン + `chat.postMessage`（チャンネル選択可・OAuth/スコープ・トークン管理）がある。
   北極星は **$0 維持費・最小構成**。Webhook はホスト型 callback 不要で Notion の内部トークンと同型。
2. **送信内容**: Notion は要約 + 文字起こしを送った。Slack は**チャット/ダイジェスト**媒体であり、
   全文文字起こしの貼り付けは騒音。**要約のみ**を送る（Notion と送信内容が異なる）。
3. **整形**: Slack は独自の **mrkdwn**（太字は `*単一*`、箇条書き `• `、見出し記法なし、`---` 水平線なし）。
   LLM の Markdown を mrkdwn へ**変換**する必要がある（Notion は装飾を除去したが Slack は変換）。
4. **正直性**: Slack 送信は要約を第三者サーバへ出す。**要約エンジンが local でも送信される**。

裏取り済み（WebFetch で Slack 公式 docs 確認）:
- POST `https://hooks.slack.com/services/T.../B.../XXX`、body `{text, blocks}`、成功は 200 + 本文 `"ok"`。
- 失敗は `no_service` / `channel_not_found` / `invalid_payload`（HTTP エラーコード + 本文に文字列）。
- **チャンネルは webhook 作成時に固定**（リクエストで変えられない）。**webhook URL 自体が秘密**（別トークン無し）。
- Block Kit: 1 メッセージ **50 ブロック**、section(mrkdwn) **≤3000 字**、header(plain_text) **≤150 字**（plain_text のみ）。

## Decision

**Incoming Webhook（BYOK）＋ 要約のみ ＋ コアの exporter** で実装する。

- **認証 = Incoming Webhook URL（$0・OAuth 不要）**。ユーザーが api.slack.com/apps で App 作成 →
  Incoming Webhooks を On → チャンネルを選んで URL 発行 → mojiroku に貼る。URL は**キーチェーン**
  （account 名 `slack_webhook_url`、既存 `set_secret`/`has_secret`/`delete_secret` を流用。`get` は JS
  非公開で Rust 内のみ）。**チャンネルが URL に内包されるため settings.json に別フィールドを持たない**
  （Notion の `notion_parent_id` 相当が不要）。
- **HTTP = コア `crates/mojiroku-core/src/export/slack.rs`**（`notion.rs` と同じ ureq blocking ＋
  `slack_err` で `no_service`/`channel_not_found`/`invalid_payload` を区別）。mrkdwn 変換・URL 検証・
  ブロック構築は**単体テスト可能**（純関数）。
- **送信内容 = 要約のみ**（文字起こしは送らない）。要約が無ければ**空投稿せず誘導エラー**を返す。
- **URL 検証**: `https://hooks.slack.com/services/` プレフィックスを強制（誤 URL/タイプミスで要約を
  **任意ホストへ流さない**ためのガード。host authority が `/services/` で確定し userinfo/サブドメイン偽装不可）。
- **mrkdwn 変換**: 見出し `#`/`##` → `*太字*`（LLM の二重マーク `## # 議題` も先頭ハッシュ群を剥がす）、
  箇条書き `-`/`*`/`+` → `• `、`**bold**`/`__bold__` → `*bold*`、`---`/`***`/`___` 水平線は除去。
  **Slack 制御文字 `&`/`<`/`>` をエンティティへエスケープ**（`&` を先に）。未エスケープだと `Vec<String>` 等の
  角括弧表記がリンク扱いで消える（開発者の議事録でジェネリクスが頻出）。
- **ブロック構築**: header(タイトル ≤150 字) + 各要約の `*ラベル*` + 本文 section(≤3000 字で分割)。
  50 ブロック超は**複数メッセージへ分割**（無言の切り捨てをしない。通常は 1 通）。
- **送信の透明性（要約エンジン非依存）**: Slack 送信は**要約のみ**を Slack サーバへ送る。設定の「連携」
  セクション・SharePopover のボタン直下/フッター・プライバシーパネルで明示。プライバシーパネルは
  宛先別の送信内容を保つ（**Notion へ＝要約 + 文字起こし / Slack へ＝要約のみ**）。装飾は Slack 記法へ
  **変換**（Notion の「除去」と区別して開示）。
- **送信は明示同意のみ**: 「Slack に送る」を押したときだけ実行。`doSlack` は in-flight ガード（`useRef`）で二重投稿を防ぐ。

## Consequences / リスク

- ⚠️ **ラウンドトリップは実 webhook でしか検証できない（load-bearing）**。**ベータ/実機で実 webhook 投稿を
  確認済み**（チャンネルに着弾 → mrkdwn レンダリング目視。`no_service`/`channel_not_found` の区別エラーで
  ユーザー自己診断可能）[[verify-load-bearing-assumptions]]。
- 要約が端末外（Slack）へ出る。**ローカル要約でも送信される**ため開示をエンジン非依存にした。
- **4 次元アドバーサリアル レビュー（mrkdwn/開示/上限/検証）で 4 件確定 → 対応**:
  - (high) 通信エラー時に ureq の `Display` が**秘密 webhook URL** をトースト/JS へ漏らす →
    Transport ブランチを `kind()`（URL 非含有の分類）に変更。Notion と同型コードだが Notion は固定の
    非秘密 URL なので無害 →「Notion と同型だが Slack 固有の漏れ」。
  - (medium) Slack 制御文字 `&`/`<`/`>` 未エスケープ → セクション経路でエスケープ。
  - (medium) 要約再生成で `summaries` 表が累積（`save_summary` は素 INSERT）→ エクスポートに旧要約混入。
    **`get_recording_detail` で `template_id` ごと最新に畳む**（フロント DetailView と同じセマンティクス。
    Notion/MCP も一貫して正される）。
  - (low) 3000 字境界で `*bold*` が割れる → 既知制限としてドキュメント化（表示のみ・データ欠落なし）。
- **申し送り（本 ADR の範囲外）**:
  - チャンネルは webhook 固定。複数チャンネル投稿/動的選択は Bot トークン化が必要（未対応）。
  - mrkdwn 変換は素朴な行ベース。Markdown リンク `[x](y)`・テーブル等の厳密変換は未対応。
  - PDF 書き出し・カレンダー連携は本基盤の上に別途。

関連: [[ADR-0013_Notion書き出し]]（同型の BYOK exporter 基盤）, [[ADR-0012_設定永続化とBYOKキーチェーンと要約分岐]], `docs/spec.md` §8。
