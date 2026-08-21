## 概要
-

## 種別
- [ ] feat（機能追加） / fix（バグ修正） / refactor / docs / test / chore

## 変更内容
-

## 確認項目
- [ ] `cargo build --workspace` / `cargo test --workspace` が通る
- [ ] フロントに変更があれば `npm --prefix frontend run build` が通る
- [ ] ハードコード・デバッグ用の一時コードが残っていない
- [ ] README / `docs/` / `CLAUDE.md` が最新の挙動を反映している

## develop → main（リリース）の場合のみ
- [ ] `src-tauri/tauri.conf.json` の `version` を上げた（自動リリース CI のトリガ・ADR-0020）
- [ ] 実機（Apple Silicon / macOS）で文字起こし→要約の基本フローを確認した
- [ ] 未署名配布の起動手順（`xattr -dr com.apple.quarantine`）に影響する変更がないか確認した

## 関連リンク（Notion ToDo / ADR / Issue）
-
