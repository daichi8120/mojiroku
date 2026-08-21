# 0013. Notion 書き出し（内部インテグレーション トークン = BYOK・ページ親・コア exporter）

- ステータス: 採用（**認証方式は [[ADR-0019_連携のOAuthワンクリック化]] で OAuth に置換**。exporter／ページ親／ブロック構築は引き続き有効）
- 日付: 2026-06-27

## Context

Phase 6（連携/書き出し）の最初の外部送信スライス。[[ADR-0012_設定永続化とBYOKキーチェーンと要約分岐]] の
設定永続化 + キーチェーン基盤の上に、議事録を **Notion へページとして書き出す**。論点:

1. **認証方式**: Notion 連携には (a) 内部インテグレーション トークン（ユーザーがトークンを発行し
   対象ページに共有）と (b) 公開 OAuth インテグレーション（ホスト型 redirect が必要）がある。
   北極星は **$0 維持費**。OAuth はホスト型 callback を要し $0 と相反する。
2. **HTTP をどの層に置くか**: BYOK 要約（`byok.rs`）が既にコアで ureq blocking を使う。
3. **送信先の指定**: Notion ページ作成は親（page か database）を要する。database 親は
   タイトルプロパティ名の特定が要り、かつ**新しい Notion-Version では `database_id` ではなく
   `data_source_id` を要求**する（仕様変更）。page 親は title プロパティが常に `"title"` で安定。
4. **正直性**: Notion 送信は要約 + 文字起こしを第三者サーバへ出す。**要約エンジンが local でも送信される**。

裏取り済み（WebFetch で Notion 公式リファレンス確認）:
- Create: `POST /v1/pages`、`parent {type:"page_id", page_id}`、`properties.title`、children 最大 100。
- Append: `PATCH /v1/blocks/{block_id}/children`、children 最大 100。応答に作成ブロックが返る。
- ヘッダ: `Authorization: Bearer <token>`、`Notion-Version`。最新は `2026-03-11` だが
  page 親 + paragraph/heading/bulleted_list_item + 応答 `url` は 2021 年から不変のコア。
- rich_text 1 要素 2000 文字上限（超で 400 validation_error）。rich_text 配列にも要素数上限。

## Decision

**内部インテグレーション トークン（BYOK）＋ page 親 ＋ コアの exporter** で実装する。

- **認証 = 内部インテグレーション トークン（$0・OAuth 不要）**。ユーザーが
  notion.so/my-integrations でトークン発行 → 対象ページをインテグレーションに「共有」→
  トークンと親ページ URL/ID を mojiroku に貼る。トークンは**キーチェーン**（account 名 `notion_token`、
  既存 `set_secret`/`has_secret`/`delete_secret` を流用。`get` は JS 非公開で Rust 内のみ）。
  親ページ ID は `settings.json`（`notion_parent_id`、`serde(default)` で後方互換）。
- **HTTP = コア `crates/mojiroku-core/src/export/notion.rs`**（`byok.rs` と同じ ureq blocking ＋
  `notion_err` で 401/403/404/400 を区別しレスポンスボディを表面化）。Markdown→Notion ブロック変換・
  ページ ID 正規化・話者マージは**単体テスト可能**（純関数）。
- **送信先 = page 親に子ページ作成**（`parent.page_id`、title プロパティは確実に `"title"`）。
  ⚠️ **database 親は採らない**: 新 Notion-Version の `data_source_id` 要求でバージョンピンが崩れるため。
  これにより安定版 **`2022-06-28`** をピンできる。
- **バージョンピン = `2022-06-28`**（page 親 + 基本ブロックはこのバージョンで不変。最新追従は不要）。
- **ブロック構築**: 各要約セクション（議事録/要約/アクションアイテム）+ 区切り + 文字起こし。
  - 100 ブロック制限 → create に先頭 100 ＋残りを 100 ずつ append（**無言の切り捨てをしない**）。
  - 連続同一話者セグメントをマージしブロック数を抑える。**マージ判定は `speaker_id`**（表示名ではない。
    別話者を同名へ改名しても別ターンに保つ）。表示名は描画時に解決。
  - 1 段落を **1800 文字でチャンク分割**（話者分離なしの長尺が 1 段落へ collapse して壁テキスト/
    巨大 rich_text 配列になるのを防ぐ）。空テキストのターンは段落を作らない。
  - インライン Markdown 装飾（`**太字**` 等）は**除去してプレーン化**（MVP 制限。UI で開示）。
  - 応答ストール対策に ureq Agent へ全体タイムアウト（60s）を設定。
- **送信の透明性（要約エンジン非依存）**: Notion 送信は要約 + 文字起こしを Notion サーバへ送る。
  これを **設定の「連携」セクション（常時表示）・SharePopover のボタン直下・フッター・
  プライバシーパネル**で明示。プライバシーパネルは「外部送信は (1) BYOK 要約 (2) Notion 書き出し」の
  2 経路として記述し、`engine` チェックの裏に Notion を隠さない。
- **送信は明示同意のみ**: ユーザーが「Notion に送る」を押したときだけ実行（自動同期はしない）。
  `doNotion` は in-flight ガード（`useRef`）で二重ページ作成を防ぐ。

## Consequences / リスク

- ⚠️ **ラウンドトリップは実 Notion 連携でしか検証できない（load-bearing）**。ビルド/単体テストは
  コンパイルとブロック構築の正しさを示すが、トークン発行→ページ共有→送信の疎通は別。
  **配布前・ベータで実トークンによる送信を確認**する（401=トークン/404=未共有/400=ID 形式の
  区別エラーを実装済みなのでユーザー自己診断可能）[[verify-load-bearing-assumptions]]。
- データが端末外（Notion）へ出る。**ローカル要約でも送信される**ため、開示をエンジン非依存にした。
- **多次元アドバーサリアル レビュー（5 次元）で 6 件確定 → 全件修正**: プライバシーパネルの
  Notion 開示漏れ、二重送信ガード、HTTP タイムアウト、長尺 collapse 分割、同名話者マージ、空ターン。
- **申し送り（本 ADR の範囲外）**:
  - database 親（会議ノート DB への行追加）は未対応。要望が出たら `data_source_id` 対応と
    タイトルプロパティ検出を別途設計（バージョン更新を伴う）。
  - PDF 書き出し・Slack 送信・カレンダー連携は本基盤の上に別途。
  - 装飾除去は素朴な記号 strip（`**`/`__`/`` ` ``）。インライン記法の厳密変換は未対応。

関連: [[ADR-0012_設定永続化とBYOKキーチェーンと要約分岐]]（設定 + キーチェーン基盤）, `docs/spec.md` §8。
