# 0022. 配布に Apple Developer ID 署名 + notarization を導入

- ステータス: 採用
- 日付: 2026-07-03
- 関連: [[ADR-0011_配布は未署名dmgでCloudflareとReleases]]（本 ADR が「未署名で配布」の節を supersede。ホスティング構成は存続）/
  [[ADR-0017_会議モードのシステム音声キャプチャ]]（署名の twofer 論拠）/ [[ADR-0020_自動リリースパイプライン]]（本 ADR が署名層を追加する CI）

## Context

Apple Developer Program の加入が承認された（2026-07。$99/年）。これまでは ADR-0011 の判断で
未署名 .dmg を配布しており、次の摩擦を抱えていた:

- **初回起動の「damaged（壊れているため開けません）」**: macOS 26 は右クリック「開く」バイパスが
  廃止され、`xattr -dr com.apple.quarantine` のターミナル操作が主手順。非技術ユーザーには高い壁。
- **TCC 許可が更新のたびに失効**: ad-hoc 署名は cdhash でキーされるため、アプリ更新（=新 cdhash）で
  マイク・画面収録の許可が無効化され、GUI での remove/re-add が必要（ADR-0017 が「会議モードの存在は
  署名の根拠を一段強める」と申し送り済み）。

ADR-0017 の言う twofer — ①notarization が quarantine の「damaged」を解消 ②Developer ID の安定した
designated requirement (DR) により TCC 許可が更新を跨いで永続 — を実現する。

裏取り済みの技術事実（Tauri v2 公式 docs + tauri-bundler ソース確認、2026-07-03）:

- `APPLE_CERTIFICATE`(base64 .p12) + `APPLE_CERTIFICATE_PASSWORD` env があれば **Tauri が一時
  keychain を自動作成・削除**する（手動 `security` ステップ不要）。`APPLE_SIGNING_IDENTITY` を併設すると
  import した証明書との不一致で fail-closed になる。env が無ければ署名は丸ごとスキップ（警告のみ）。
- notarization は `APPLE_API_ISSUER` + `APPLE_API_KEY` + `APPLE_API_KEY_PATH`（App Store Connect API
  キー方式、Team ID 不要）で **.app を自動 notarize + staple**。失敗はビルドエラー（fail-closed）。
  ただし env 不足時は警告のみでスキップされる → CI 冒頭の secrets 存在チェックが必須。
- **.dmg は Tauri が codesign はするが notarize/staple はしない** → CI で手動 `notarytool submit` +
  `stapler staple` が必要。
- updater 用 `.app.tar.gz` は notarize+staple 済み .app から後段生成される（順序問題なし）。
  sidecar `mojiroku-llm`（externalBin）は bundler が同一 identity + hardened runtime で自動署名。
- hardened runtime（notarization 必須要件）下では **`com.apple.security.device.audio-input`
  entitlement が無いとマイクの TCC 要求自体が拒否される**。画面収録（ScreenCaptureKit）に対応する
  entitlement は存在しない（TCC のみ）。disable-library-validation / allow-jit /
  allow-unsigned-executable-memory は本アプリ構成では不要（ggml/Metal は実行時 CPU コード生成なし、
  WKWebView の JIT は Apple 署名の別プロセス、/usr/lib/swift は Apple 署名 dylib）。

## Decision

**リリース CI（release.yml）で Developer ID Application 署名 + hardened runtime + notarization を行い、
署名済み .dmg / .app.tar.gz を配布する。**

1. **署名は CI の env 駆動のみ**。`tauri.conf.json` に `signingIdentity` は書かない
   （ローカル `just build` は従来どおり無署名で通る）。conf には `hardenedRuntime: true` と
   `entitlements: "entitlements.plist"` のみ宣言。
2. **entitlements は `com.apple.security.device.audio-input` のみ**（`src-tauri/entitlements.plist`）。
   最小権限を維持し、必要になった時だけ追加する。
3. **notarization は App Store Connect API キー方式**（CI 向き。2FA/セッション切れの影響を受けない）。
   .app は Tauri が自動 notarize+staple、**.dmg は CI が手動 notarytool+staple**。
4. **CI は fail-closed を二層で検証**: 既存の minisign（updater）検証に加え、Apple 署名/公証の検証
   （codesign --verify / stapler validate / spctl、対象は .app・.app.tar.gz の中身・.dmg の3実体）を
   公開前に実行。secrets 不足はビルド前にアサートで落とす。
5. **minisign updater チェーンは Apple 署名と完全独立で不変**（ADR-0020 の構成・鍵・latest.json に変更なし）。
6. **開発時署名は Apple Development 証明書に切替可能に**: `scripts/dev-sign-run.sh` が
   `.mojiroku-dev-sign.env`（gitignore）または env の identity を優先し、無ければ従来の自己署名
   `mojiroku-dev` にフォールバック。dev には hardened runtime / entitlements は付けない。

## Consequences

- 初回起動の摩擦がゼロに（quarantine 付き DL をダブルクリック起動）。landing / install-macos.md の
  xattr 案内は「旧バージョン向け」に降格。非技術ユーザーへの配布が可能になる。
- 署名版どうしの更新では TCC 許可が永続（安定 DR）。**0.3.x（ad-hoc）→ 0.4.0（Developer ID）の
  移行時だけ、マイク・画面収録の再許可が一度求められる**（updater の動作自体は不変）。
- コスト: $99/年。CI 所要時間 +5〜15分（notarytool。遅延日対策に timeout-minutes: 150）。
- 管理物が増える: Developer ID 証明書（5年）/ .p12 とパスワード / App Store Connect API キー（.p8）/
  GitHub secrets 6 個（APPLE_CERTIFICATE / APPLE_CERTIFICATE_PASSWORD / APPLE_SIGNING_IDENTITY /
  APPLE_API_ISSUER / APPLE_API_KEY / APPLE_API_KEY_CONTENT）。開発用 Apple Development 証明書は
  1年失効（失効時は再作成 + TCC 一回再許可）。
- 実機検証（2026-07-03, macOS 26.5, CI dry-run ビルドで実施済み）:
  - ✅ hardened runtime 下の ScreenCaptureKit（会議モード・デュアルトラック文字起こし。/usr/lib/swift rpath 解決も成立）
  - ✅ マイク録音（audio-input entitlement 経由の TCC）/ ✅ ローカル要約 sidecar（署名+hardened runtime で Metal 動作）
  - ✅ Gatekeeper: quarantine 付き .dmg → spctl「Notarized Developer ID」accepted・警告なしダブルクリック起動
  - 画面収録 TCC は**許可後にアプリ再起動するまで効かない**（macOS 仕様。初回テストで空のライブ文字起こしに見える罠）
  - Sequoia+ の「private window picker バイパス同意」は**バイナリ（cdhash）が変わった直後の起動でのみ再提示**され、
    同一バイナリの再起動では出ない（3回目起動で不出を確認）。将来頻度が問題になれば SCContentSharingPicker を検討
  - ✅ アプリ内更新: 署名版 0.3.0 → 0.4.0 を実機 E2E（DL→minisign 検証→置換→再起動。更新後も
    Developer ID 署名・staple 有効を確認）
  - ⚠️ **検証時の罠（App Translocation）**: quarantine 付き .app を Finder を使わず CLI（ditto 等）で
    /Applications に置いて起動すると、translocation（読み取り専用ランダムパス実行）が発動し
    **updater が自己置換できず失敗する**。実ユーザーは Finder ドラッグ（translocation 不発動）か
    updater 経由（quarantine なし）なので影響なし。検証では `xattr -dr com.apple.quarantine` で解除する。
