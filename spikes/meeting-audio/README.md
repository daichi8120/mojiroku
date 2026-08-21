# Phase 7 会議モード スパイク — システム音声キャプチャ × 未署名×TCC（macOS 26.5）

会議モード（Zoom/Meet/Teams の**相手側の音声**をローカル取得 → 文字起こし）の実現性を、
**実装前に実機で裏取り**するための throwaway スパイク。北極星の "$0・送信なし・ローカル完結" を壊さないかを確認する。

> 方針: ロードマップの「スパイク → ADR → 実装」ゲート。`docs/decisions/ADR-0011_配布は未署名dmgでCloudflareとReleases`（未署名配布）に関わる
> **$99 署名判断を前倒しすべきか**をこの計測で決める。結果は ADR-0017 に記録する。

## 何を測るのか（このスパイクが出すべき結論）

「音を録れるか」ではなく **「未署名(ad-hoc) .app での TCC ストーリーが許容できるか」**。具体的に:

1. **API 動作**: `screencapturekit`(Rust v8) が macOS 26.5 でビルド・動作するか → ✅ **ビルド/リンク確認済み**（コンパイル時点）
2. **TCC 発火**: ad-hoc `.app` で許可ダイアログが出て Allow でき、Privacy 一覧に載るか
3. **キャプチャ成立**: 許可後にコールバックが発火し **非無音 PCM** が流れるか（相手の声が録れるか）
4. **再ビルド→TCC失効（最重要 = 署名判断の分かれ目）**: 更新（cdhash 変化）後に
   **クリーン再プロンプト**（許容可）か **サイレント拒否**（トグル ON のまま無音録音 = 会議アプリで致命的）か

> **権限ペインについて（誤解しないために）**: このハーネスは SCK の既定パターンに合わせ Screen ハンドラ＋2x2 映像を
> 引いている。つまり **構造上 "Screen Recording" 確定** で、軽い "System Audio" 単独ペインには載らない。
> リサーチ上 SCK は音声専用でも Screen Recording を要求する（「画面は録らないのに画面収録許可が要る」）ので、
> ここで問うべきは「**会議メモアプリに Screen Recording 許可は許容できるか**」だけ。許容できないなら
> **軽いペイン（`kTCCServiceAudioCapture`）は Plan B = Core Audio process tap でしか得られない**。SCK でそこは追わない。

## 構成

- `src/main.rs` — SCK でシステム音声を 25 秒キャプチャ。コールバック数・実フォーマット・RMS（非無音判定）を
  `~/Desktop/mojiroku-spike-log.txt` に記録し、mono WAV を `~/Desktop/mojiroku-spike-capture.wav` に書く。
- `Info.plist` — bundle id = `com.daichi0812.mojiroku-spike`（実アプリと TCC を混ぜない）、`NSScreenCaptureUsageDescription`。
- `package.sh` — release ビルド → `.app` 組み立て → **clean deep ad-hoc 署名** → 実行手順表示。
  毎回 `build_tag` を更新し、**バイナリ(cdhash)が必ず変わる**（再ビルドサイクル計測のため）。
- 本体ワークスペースからは切り離し済み（`Cargo.toml` の空 `[workspace]`）。`target/` `dist/` は gitignore。

## 計測手順（実機・要ユーザー操作）

TCC ダイアログは人手が要る。`open`（LaunchServices）で起動すると **bundle ID で TCC 帰属**するので、
ターミナル直起動ではなく必ず `open` を使う。

### ラウンド1 — 発火 / ペイン / キャプチャ成立

```bash
cd spikes/meeting-audio
bash package.sh
open './dist/MojirokuSpike.app' --args 25     # 直後に音楽 or 通話で音を鳴らす
```

- **初回は get() で許可ダイアログが出て、その回は失敗で終わるのが正常**（TCC の仕様）。
  System Settings > Privacy & Security > **Screen & System Audio Recording** で **MojirokuSpike を ON** にして、**もう一度 `open ...` を実行**。
- **ダイアログが出ず一覧にも MojirokuSpike が無い場合**（ad-hoc アプリで起こり得る・それ自体が計測データ）:
  同ペインの **「＋」で `spikes/meeting-audio/dist/MojirokuSpike.app` を手動追加** → ON → 再 `open`。手動追加が要ったかを報告。
- 確認: `cat ~/Desktop/mojiroku-spike-log.txt` で `audio callbacks > 0` と `peak RMS` 非無音、
  `afplay ~/Desktop/mojiroku-spike-capture.wav` で相手音が録れているか。

**報告してほしいこと:** ①ダイアログの正確な文言と、名指しされたアプリ名（MojirokuSpike か Terminal か）
②どのペイン名で出て一覧に載ったか（"Screen Recording" / "Screen & System Audio Recording" 等）— 会議メモに許容できそうか所感
③ログの callbacks 数・peak RMS・frames/elapsed ④WAV で相手音が聞こえたか（ピッチが速い/遅いなら別レートのサイン）

### ラウンド2 — 再ビルド→TCC失効サイクル（署名判断の核心）

ラウンド1で許可済みのまま:

```bash
bash package.sh                               # build_tag 更新 = 新 cdhash
open './dist/MojirokuSpike.app' --args 25     # 音を鳴らす
cat ~/Desktop/mojiroku-spike-log.txt
```

**報告してほしいこと:** 次のどれか —
(a) 何もせず録れた（許可が残った） / (b) 新しい許可ダイアログが出た（クリーン再プロンプト） /
(c) トグルは ON のままなのに `peak RMS ≈ 0`（**サイレント拒否** = 致命的）。
(c) の場合の復旧: `tccutil reset ScreenCapture com.daichi0812.mojiroku-spike`。

### ラウンド3（任意）— 実会議互換 & dev-loop $0 緩和

- Zoom / Google Meet / Microsoft Teams の実通話で `open ... --args 30` し、相手の声が WAV に入るか（Teams が要注意）。
- dev-loop 緩和: 無料 Apple Development（Personal Team）ID で署名すると再ビルドで許可が残るか
  （`codesign --force --deep --sign "Apple Development: <you>" dist/MojirokuSpike.app` 後にラウンド2）。

## 結果の解釈 → 次アクション

| ラウンド2の結果 | 意味 | 次 |
|---|---|---|
| (a)/(b) クリーン | 未署名のままベータ可（更新ごと再許可は xattr 同様の許容コスト） | 実装へ（$99 は ADR-0011 のまま据え置き） |
| (c) サイレント拒否 | 未署名は会議アプリに不適 | **$99 Developer ID 署名を前倒し**（ADR-0017 で決定） |

実装時は本体 `.app` 内インプロセス Rust（`src-tauri`、`mic.rs` の隣）で。bare sidecar は不可
（Tahoe で Privacy 一覧に出ず、責任プロセス帰属が崩れる）。詳細は ADR-0017。
