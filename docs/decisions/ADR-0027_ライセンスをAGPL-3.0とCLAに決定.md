# 0027. ライセンスを AGPL-3.0 + CLA に決定（オープンソース化）

- ステータス: 採用（2026-08-21 決定。[ADR-0011](./ADR-0011_配布は未署名dmgでCloudflareとReleases.md) の「LICENSE は OSS にしない」を **supersede**）
- 日付: 2026-08-21
- 関連: [ADR-0011](./ADR-0011_配布は未署名dmgでCloudflareとReleases.md)（本 ADR が上書きする独自 EULA の決定元）/ [ADR-0020](./ADR-0020_自動リリースパイプライン.md)（public 化で macOS ランナーの課金前提が変わる）/ [ADR-0022](./ADR-0022_AppleDeveloperID署名とnotarization.md)（署名配布は継続）

## Context

mojiroku の中心的な主張は「**ローカル完結・送信なし**」である。しかしこの主張は、
ユーザーには検証できない。通信していないことを外から確かめる手段が無いためである。

Obsidian はクローズドソースのまま信頼を得ているが、それは「ノートが素の Markdown で
手元にある」という別の検証手段を持つからである。mojiroku の主張は**通信の有無**なので、
その代替は存在しない。**ソース公開が実質的に唯一の証明手段**になる。

一方 [ADR-0011](./ADR-0011_配布は未署名dmgでCloudflareとReleases.md) は、当時の前提（ソース非公開・
商用前提）から「LICENSE は OSS（MIT/Apache）にしない。意図しない許諾を避け独自 EULA と
する」と決めていた。実際 `mojiroku-releases` には再配布・改変・リバースエンジニアリングを
禁じる EULA が置かれ、v0.1.0〜v0.5.1 はその条件で配布されている。

本 ADR はこの決定を覆す。方向は**プロプライエタリ → コピーレフト**であり、ユーザーの
権利は拡大する側なので、既存ユーザーの不利益は生じない。

## Decision

**ライセンスは `AGPL-3.0-or-later`。コントリビュートには許諾型 CLA を要求する。**

### 1. copyleft を選ぶ — 構造が近いプロダクトの実態がそう示している

ローカルファースト・プライバシー重視のデスクトップアプリは、ほぼ全てが copyleft か
クローズドである。permissive はほとんどいない。

| プロダクト | ライセンス | CLA | 収益 |
|---|---|---|---|
| Joplin | AGPL-3.0（**2022-12 に MIT から移行**） | あり・許諾型 | 同期 |
| Logseq | AGPL-3.0 | あり | 同期 |
| Standard Notes | AGPL-3.0 | 不十分（後述） | 同期 |
| Signal | GPL-3.0 | あり・許諾型 | 寄付 |
| Cryptomator | GPL-3.0 ＋ 商用のデュアル | あり・許諾型 | アプリ内課金 |
| Bitwarden | AGPL/GPL ＋ `bitwarden_license/` | あり・譲渡型優先 | 同期 |
| KeePassXC | GPL-2/3 | **なし** | 寄付のみ |
| Obsidian | クローズド | — | 同期・Publish |

法的な保護というより、**「データを守る側にいる」という姿勢の表明**として読まれている。
信頼の獲得が動機である以上、このシグナルは無視できない。

### 2. permissive を採らない — ただし「保護される」とは考えない

同じ議事録・文字起こし領域は全員 MIT である（Meetily ★29.7k / Buzz ★21.1k /
Vibe ★7.2k / anarlog ★9.1k / whisper.cpp ★53.1k）。AGPL は明確な逸脱になる。

それでも copyleft を選ぶ理由は上記 1 だが、**過大評価はしない**。

- AGPL が防ぐのは**クローズドなコピー**だけである。資金力のある競合が fork し、改良し、
  ソースを公開したまま売ることは合法であり、止められない。
- 「これは自分たちが作った」と言われることは、**どのライセンスでも防げない**。
- 上流の whisper.cpp / sherpa-onnx / llama.cpp が全て permissive なので、本気の競合は
  mojiroku を参照せずに同等品を作れる。
- 名前を守る手段はライセンスではなく**商標**であり、地位を守る手段は**開発を続けること**である。

### 3. source-available を採らない — デスクトップでは条項が空文化する

BUSL 1.1 / FSL / Elastic License 2.0 / SSPL / n8n の Sustainable Use License は、いずれも
核となる禁止条項が「**ホスティング／マネージドサービスとしての提供**」である。AWS 対
Elastic の構図への対策として設計されたもので、ネットワーク越しに提供しないデスクトップ
アプリでは発動しない。

例外は PolyForm Shield / Perimeter で、これは「ライセンサーと競合する利用」を直接禁じる
ためデスクトップでも機能する。しかし **OSI 非承認であり「オープンソース」を名乗れない**ため、
信頼の獲得という動機と正面から衝突する。よって不採用。

### 4. CLA を要求する — 将来の選択肢を残す唯一の手段

**AGPL/GPL のコードは再ライセンスできない。** 貢献者が 1 人でもいれば、全員の同意なしに
ライセンスを変えられなくなる。

Standard Notes は 2023 年に AGPL から別ライセンスへ移行しようとして、貢献者の反発で
撤回した。CLA を持っていなかったためである。

一方 MIT / Apache-2.0 は sublicense を明示的に許すため、CLA が無くても後から締められる
（screenpipe が 2026-06-09 に MIT から独自商用ライセンスへ移行した実例がある）。
**copyleft を選ぶなら CLA はセットで必要**という非対称性がある。

型は**許諾型**とする。著作権は貢献者に残し、プロジェクトに再ライセンス可能な広い許諾を
与える。Joplin / Signal / Cryptomator が同型である（いずれも "reserve all right, title,
and interest" を明記）。Bitwarden の譲渡型優先までは求めない。

**CLA には摩擦がある。** GitLab は 2017 年に CLA を廃止して DCO へ移行した。AGPL + CLA の
組み合わせは「将来プロプライエタリ化する布石」と読まれることがある。それでも採るのは、
DCO ではライセンス変更の権利が得られず、Mac App Store（下記 5）への道も塞がるためである。

### 5. Mac App Store との両立は CLA で解決する

GPL/AGPL は App Store の利用規約と非互換とされる。しかし**縛られるのは他人であって
著作権者本人ではない**。全著作権を集約していれば、同じコードを別ライセンスで App Store
向けに配布できる。

**Cryptomator がこれを実運用している。** GPL-3.0 と商用ライセンスのデュアルにし、
App Store 版は商用ライセンス側で配布している。

この道が使えるのは、**依存 634 件に GPL-only が 1 件も無い**ことが実測で確認できている
ためである（MPL-2.0 が 15 件あるが、いずれも未改変利用なので帰属表示のみで済む。
`r-efi` 5.3.0 / 6.0.0 が `MIT OR Apache-2.0 OR LGPL-2.1-or-later` のデュアルだが、
MIT を選べるため影響しない）。他人の copyleft が混ざっていれば成立しなかった。

### 6. 過去リリースには遡及しない

**v0.6.0 以降が AGPL-3.0。v0.5.1 以前は旧 EULA のまま残す。**

`mojiroku-releases` の `LICENSE` は AGPL に差し替え、旧 EULA は
`LICENSE-legacy-eula.txt` として保全する。過去リリースのアセットには手を入れない。

### 7. 公開は新規 public リポジトリで行い、旧リポジトリは private で保全する

公開にあたり `agents/`（Notion 前提の運用規約。プライベート Notion ページ URL 3 本を含む）
を削除するが、**既存リポジトリを public に切り替えることはしない**。新規に public な
`daichi8120/mojiroku` を作り、**匿名化済み HEAD の 1 コミット**から始める。既存リポジトリは
`daichi8120/mojiroku-archive` にリネームし、**private のまま保全する**（削除しない）。

そう決めたのは、履歴に残るのが `agents/` の Notion URL だけではなかったためである。

- `docs/decisions/ADR-0018`（話者ライブラリ）に第三者 4 名の実名が 11 箇所あった。
  **声紋の cosine 類似度スコアと紐づいており**、本人の同意が無い生体情報にあたる
- `docs/error.md`（`06f2d02` で誤コミット、`6a0a39e` で削除）に macOS のクラッシュレポートが
  残っている

そして**そのいずれもが 29 本すべての PR ref から到達可能**だった（2026-08-21 に
`git ls-remote origin 'refs/pull/*/head'` で 29 本を実測）。`refs/pull/*` は GitHub が管理する
ref でクライアントからは削除できず、`git filter-repo` で書き換えても `blob/<旧SHA>/...` は
読める状態が残る。**履歴の書き換えでは解決しない。**

失うものは小さくない。正直に書く。

- **318 コミットの開発履歴**（`git rev-list --count --all` の実測値）
- **29 本の PR とそこでのレビューの議論**
- **27 本の ADR の変遷**（確定した本文は引き継がれるが、そこに至る差分は消える）

ADR と `docs/` の現在形は新リポジトリに引き継がれるので、判断の記録そのものは失われない。
失うのは「いつ・どの差分でそこへ至ったか」である。旧リポジトリを削除せず private で残すのは、
作者の手元でこれを参照できるようにするためである。

## 検証

- `cargo metadata --format-version 1 --no-deps` で 4 クレート全てが `AGPL-3.0-or-later`
  を返すこと。**ワークスペースの `license` は自動継承されない**ため、各クレートに
  `license.workspace = true` を明示している（2026-08-21 実測で確認済み）
- `src-tauri/tauri.conf.json` の `bundle.resources` により、v0.6.0 以降の `.app` に
  `LICENSE` と `NOTICE` が同梱される。**v0.5.1 までは同梱されていなかった**
  （`bundle.licenseFile` は使わない。理由は下記「影響・制約」）
- `gitleaks` 8.30.1 の `gitleaks detect --log-opts="--all"` で全履歴をスキャンし、
  **検出 3 件・すべて誤検知**（2026-08-21 実測）
  - `landing/src/layouts/Layout.astro` の Cloudflare Web Analytics beacon トークン。
    HTML に載る公開 ID なので秘匿の必要がない（同ファイルのコメントにもそう書いてある）
  - 履歴にのみ存在する `docs/error.md`（`06f2d02` で誤コミット、`6a0a39e` で削除）の
    macOS クラッシュレポート内 `Crash Reporter Key` が 2 件。端末に紐づく UUID であって
    資格情報ではなく、パスは Apple が `/Users/USER` に匿名化済み

## 影響・制約

- **AGPL は組織内での採用を一部止める。** Google は社内で AGPL の使用を全面禁止している。
  ただし mojiroku のターゲット（議事録を外部に送れない日本の組織）が同等の厳格さを持つかは
  別問題で、社内利用に限れば開示義務は原則生じないという整理もある（グレーゾーン）
- **デスクトップ配布に限れば AGPL と GPL-3.0 は実質同じ**。AGPL の追加条項が効くのは
  ネットワーク越しの提供時なので、将来 E2E 同期サーバーを作ったときに初めて本領を発揮する
- **`cla-assistant` の設定は public 化の後**（GitHub App が private repo を見られない）。
  最初の PR を受け入れる前に済ませる必要がある
- public 化により GitHub Actions の macOS ランナーが無料枠になる。[ADR-0020](./ADR-0020_自動リリースパイプライン.md)
  の「private repo は分課金 10 倍」という前提が変わる
- **`bundle.licenseFile` は使わない。** tauri-bundler がこの値を `create-dmg` の `--eula` に
  渡すため、.dmg をマウントすると AGPL 全文の同意ダイアログが出て、Agree を押すまで中を
  開けない。「非エンジニアがダウンロードしてそのまま開ける」ことは署名+公証
  （[ADR-0022](./ADR-0022_AppleDeveloperID署名とnotarization.md)）で確保した強みなので、
  この UX 劣化は受け入れない
- **`LICENSE` と `NOTICE` は `bundle.resources` のマップ形式で `.app` に同梱する**
  （`{"../LICENSE": "LICENSE", "../NOTICE": "NOTICE"}`）。配列形式だと `tauri-utils` の
  `resource_relpath` が `..` を `_up_` に置換して `_up_/LICENSE` になるが、マップ形式は
  指定した宛先をそのまま使う（`tauri-utils` 2.9.3 のソースで確認）。ソース公開により AGPL の
  要求は満たされるが、同梱 whisper.cpp の MIT 条件はバイナリ配布にも及ぶため、これで対応する
