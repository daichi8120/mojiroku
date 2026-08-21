# 0011. 配布: 未署名 .dmg + Cloudflare ランディング + 公開 Releases repo

- ステータス: 採用（**「未署名で配布」の節のみ [ADR-0022](./ADR-0022_AppleDeveloperID署名とnotarization.md) が supersede**。
  Cloudflare ランディング / 公開 `mojiroku-releases` Releases / 安定名・302 リダイレクトの構成は存続）
- 日付: 2026-06-27（2026-07-03 一部 supersede）

## Context

北極星は「`mojiroku.com`（取得済み）から配布し、友人・研究室で実用検証→反復」。Phase 4 はその入口＝
ランディング + ダウンロード導線を、維持費 **$0** で用意する。論点は「**どこにホストし、どう配るか**」と
「**未署名アプリをどう開いてもらうか**」。

確定済みの裏取り:
- **macOS 26 Tahoe は右クリック「開く」での Gatekeeper バイパスを廃止**。未署名・未 notarize アプリは
  ダウンロード（隔離属性付き）だと開けないことがある。正規手順は System Settings > Privacy & Security >
  「このまま開く」、または `xattr -dr com.apple.quarantine`。摩擦ゼロには Developer ID 署名+notarization
  （Apple Developer $99/年）が実質必須。
- **アプリ本体 repo（`daichi8120/mojiroku`）は private**。→ GitHub Pages は無料では公開不可（要 Pro/Team）、
  **private repo の GitHub Releases 資産は公開ダウンロード不可**（認証必須）。
- **Cloudflare の無料枠は商用利用OK・帯域無制限・独自ドメイン可**、private repo とも連携可（Pages / Workers 静的アセット）。

## Decision

**未署名 .dmg** を当面の配布形式とし、**ランディングは Astro → Cloudflare**（独自ドメイン `mojiroku.com`）、
**バイナリは公開 `mojiroku-releases` repo の GitHub Releases** に置く。

- **未署名で配布（$0）**: tauri.conf に署名/notarize は入れない。`docs/install-macos.md`・ランディングに開き方を明記。
  **配布ゲート**（クリーンな macOS 26 Mac で隔離属性付き .dmg を開けるか）を実機検証し、壁が高ければ Phase 5 で
  $99 署名へ再判断。
- **ランディング = Astro/Cloudflare**: 静的・ゼロJS。GitHub Pages は private 不可＋商用 ToS の制約があるため採らない。
  実装では Cloudflare の新既定に従い **Pages でなく Workers 静的アセット**を使用（`landing/wrangler.jsonc` の
  `assets.directory: ./dist`、Worker スクリプトなし＝`main` 不要）。商用OK・帯域無制限で private 連携可。
  `landing/` は本体 repo（private）に同居し、Cloudflare の Git 連携で push（production branch = `main`）時に
  `cd landing && npm install && npm run build` → `npx wrangler deploy` で自動デプロイ。
- **バイナリ = 公開 `mojiroku-releases` repo の Releases**: 本体ソースは private のまま、公開 repo には
  **ビルド済み .dmg + プロプライエタリ EULA + リリースノートのみ**を置く。
  - **LICENSE は OSS（MIT/Apache）にしない**。ソース非公開・商用前提のため、意図しない許諾を避け独自 EULA とする。
    **⚠️ 本項は 2026-08-21 に [[ADR-0027_ライセンスをAGPL-3.0とCLAに決定]] が supersede した。**
    信頼性の獲得を目的にソースを公開し、ライセンスは **AGPL-3.0-or-later** に変更。
    v0.6.0 以降が AGPL、v0.5.1 以前は旧 EULA（`LICENSE-legacy-eula.txt`）のまま。
  - **R2 は2手目**: いまの .dmg は約12MB（モデルは起動時DL）でベータ規模、egress 無料の旨味がまだ無い。
    ダウンロードが伸びたら `dl.mojiroku.com` を R2 に向ける。
- **「ファイルの置き場所」と「公開URL」を分離**: ランディングの Download は `mojiroku.com/download` を指し、
  そこから Releases へ **302 リダイレクト**（Cloudflare の `_redirects`。Workers/Pages 静的アセット共通で外部URL 302 をサポート）。
  実体を後から R2 へ移しても行先1箇所の変更で済み、ブックマーク・将来の Tauri updater も壊れない。
  - **`releases/latest/download/...` は draft でも prerelease でもない公開リリースにしか解決しない**。初回は
    フル公開リリースで出す（beta 表記なら `/download` を固定タグへ）。アセットは**版非依存の安定名**
    `mojiroku-macos-aarch64.dmg` で添付し、版上げでリンクを壊さない。
  - **公開リリース先行**: `/download` の最終到達先は公開リリースが1本ある時に初めて 200 になる。
    リリースを先に出してから LP/独自ドメインを公開する。

## Consequences

- 利用者は `mojiroku.com` から DL → 初回のみ未署名アプリの開き方を踏む。摩擦は技術ユーザー（友人・研究室）には許容範囲。
- ビルドは当面ローカル（`just build`）。Metal/whisper.cpp の CI ビルドは不安定なので CI 化は将来。
- 署名・notarization・自動更新・R2・Windows は Phase 5 以降。
- 検証は **`curl -IL`** でリダイレクトを最後まで追い、最終応答が **.dmg の HTTP 200** であることを確認する
  （302 だけ見て GitHub 側 404 を見逃さない）。

## 検証

- **実施済み（2026-06-27, すべて緑）**:
  - Astro ビルド緑・`dist/_redirects` 反映、desktop/mobile で表示目視確認。未署名 .dmg（実測12MB）生成、`xattr` 除去で起動確認。
  - 公開リリース `v0.1.0`（非 prerelease・安定名 `mojiroku-macos-aarch64.dmg`）→
    `curl -IL .../releases/latest/download/mojiroku-macos-aarch64.dmg` が最終 .dmg 200。
  - `mojiroku.daichi8120.workers.dev` → 独自ドメイン `mojiroku.com`（NS を お名前.com→Cloudflare に委任, Universal SSL 発行）で公開。
    `curl -IL https://mojiroku.com/download` が最終 .dmg 200（`_redirects` の外部302 が Workers 静的アセットでも有効）。
- **配布ゲート実機（クリーン macOS 26）**: 未署名 .dmg は DL（隔離属性）の初回起動で「damaged（壊れているため開けません）」表示。
  `xattr -dr com.apple.quarantine /Applications/mojiroku.app` で起動可（アプリ自体は正常）。右クリック「開く」は macOS 26 で廃止のため
  **`xattr` が主手順**（System Settings「このまま開く」は別パターン）。→ 技術ユーザー向けベータは未署名で可、広め配布で $99 署名へ。
