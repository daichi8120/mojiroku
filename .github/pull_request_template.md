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

## develop → main の場合のみ
- [ ] **リリースなら** `src-tauri/tauri.conf.json` の `version` を上げた（自動リリース CI のトリガ・ADR-0020）
- [ ] **リリースでないなら** `version` を据え置いた（gate が `HEAD^` と比較し、macOS ジョブをスキップする）
- [ ] 実機（Apple Silicon / macOS）で文字起こし→要約の基本フローを確認した
- [ ] 署名・公証（ADR-0022）に影響する変更がないか確認した
      （`src-tauri/entitlements.plist` / `tauri.conf.json` の `bundle` / `release.yml` の署名まわり）

## 外部からの貢献の場合
- [ ] [CLA.md](../blob/main/CLA.md) を読み、内容に同意している
- [ ] [docs/CONTRIBUTING.md](../blob/main/docs/CONTRIBUTING.md) に目を通した

## 関連リンク（ADR / Issue）
-
