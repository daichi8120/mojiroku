# コントリビューションガイド

mojiroku の開発フロー・ブランチ運用・コミット/タグ規約をまとめます。

**はじめてコントリビュートする方は、先に下の「[Issue と PR](#issue-と-pr)」と
「[ライセンスと CLA](#ライセンスと-cla)」を読んでください。**

## コア原則

- `main` は常に「他人に見せられる安定状態」だけを置く
- `develop` で日常開発を統合し、`main` への一括反映で履歴を綺麗に保つ
- 変更は必ずブランチを切り、PR で `develop` に入れる

## ブランチ構成

| ブランチ | 役割 | マージ先 | 寿命 |
|---|---|---|---|
| `main` | 公開・配布可能な安定状態のみ。**直接 push 不可（ruleset で PR 必須）** | — | 永続 |
| `develop` | 日常開発・修正の統合先。最新の動く状態 | `main` | 永続 |
| `feat/xxx` | 機能追加・大規模リファクタ | `develop` | マージ後削除 |
| `fix/xxx` | `develop` 上で見つかったバグ修正 | `develop` | マージ後削除 |
| `hotfix/xxx` | 配布中の緊急バグ修正。`main` と `develop` 両方に反映 | `main` & `develop` | マージ後削除 |

### ブランチを切る基準

- `frontend/src/`・`src-tauri/src/`・`crates/` 配下のコードを変更する場合は **必ずブランチを切る**
- `README.md` や設定ファイルの軽微な修正は `develop` 直接でも可

## マージ基準

### `develop` → `main`

- ✅ マイルストーン到達（Phase 完了・リリース・デモ完成 など）
- ✅ アプリ全体が壊れていない（`cargo test` / `npm test` が通る・手動確認済み）
- ✅ ハードコード・デバッグ用一時コードが残っていない
- ✅ README・docs・設定ファイルが最新の挙動を反映

### `feat/xxx` → `develop`

- ✅ 回帰確認が完了
- ✅ 関連ドキュメント（spec/ADR 含む）・設定を更新
- ✅ レビュー or セルフレビュー済み

## 命名規則（ブランチ）

| パターン | 例 |
|---|---|
| `feat/<短い説明>` | `feat/whisper-pipeline`, `feat/summary-templates` |
| `fix/<短い説明>` | `fix/segment-merge-offset` |
| `hotfix/<短い説明>` | `hotfix/model-load-crash` |

- 日本語禁止。kebab-case で短く具体的に。

## コミット規約（Conventional Commits）

`<type>(<scope>): <subject>` 形式。`type` は `feat` / `fix` / `docs` / `refactor` / `test` /
`chore` / `perf` / `build`。`scope` は変更層を示す：

- `frontend` / `tauri` / `core` / `docs` / `ci`
- 例: `feat(core): add whisper-rs STT wrapper` / `docs(adr): record sherpa-onnx diarization`

## タグ運用

| パターン | 用途 | 例 |
|---|---|---|
| `release/vX.Y` | バージョン | `release/v0.1`, `release/v1.0` |
| `milestone/<name>` | 意味のある区切り | `milestone/phase1-mvp`, `milestone/diarization` |

リリースは `src-tauri/tauri.conf.json` の `version` を上げて `main` にマージすると、
CI が自動でビルド・署名・公開まで行います（[ADR-0020](./decisions/ADR-0020_自動リリースパイプライン.md)）。

## ディレクトリと言語別ツール

| 層 | ツール |
|---|---|
| `frontend/` | TypeScript strict / ESLint / Prettier / Vitest（npm, Vite） |
| `src-tauri/`・`crates/` | rustfmt / clippy / `cargo test` |

dev 起動は `just dev`（`tauri dev` が Vite を起動）。

**`cargo build --workspace` の前に `bash scripts/build-sidecar.sh` を実行してください。**
要約用の sidecar バイナリ `src-tauri/binaries/mojiroku-llm-<triple>` はビルド成果物で
gitignore されているため、clone 直後は存在しません。無いまま先に `cargo build` すると
`resource path binaries/mojiroku-llm-aarch64-apple-darwin doesn't exist` で失敗します。
`just dev` / `just build` は build-sidecar.sh を自動で実行します。

## Issue と PR

**mojiroku は個人が開発しています。** 返信までに時間がかかることがあります（数週間空くことも
あります）。急ぎの対応は約束できませんが、Issue は必ず読みます。

### バグ報告

次を書いてもらえると再現が早くなります。

- mojiroku のバージョン（メニューバー →「mojiroku について」）
- macOS のバージョンと Mac の機種（Apple Silicon 前提です）
- 再現手順と、期待した挙動 / 実際の挙動
- 音声ファイルを扱う処理なら、その長さ・形式・話者の人数

**録音や文字起こしの中身は貼らないでください。** 会議の内容が入っている可能性があります。
再現に必要なら、こちらから最小の再現手順を相談します。

### プルリクエスト

- **大きめの変更は、先に Issue で相談してください。** 方針が合わないまま実装が進むと、
  お互いの時間が無駄になります。
- 向き先は `develop` です（`main` は ruleset で直接マージできません）。
- コミットは上記の Conventional Commits に従ってください。
- `cargo test --workspace` と `npm --prefix frontend run build` が通ることを確認してください。
- 挙動が変わる変更は、`docs/` の該当箇所も一緒に更新してください。
- 技術的な判断を伴う変更は、`docs/decisions/` に ADR を添えてください。

## Issue and Project synchronization

When an agent opens a PR, linking and status verification are part of that task.
This is an agent-run workflow using the existing `gh` login and browser session;
it is not an unattended GitHub Actions job. It does not need a new token or change
shared Project automations.

1. Identify the issues actually implemented by the PR. Use `Refs #N` for partial
   work; use `Closes #N` only when all acceptance criteria are satisfied. Do not
   treat every issue mentioned in a PR as an implementation target.
2. Read the issue's state and Project Status **before** linking. Keep explicit
   deferred work at Todo. Review issue scope before deciding completion.
3. After opening the PR, use the issue's **Development** selector in the browser.
   Search for the **full PR URL**, select only the intended PR, and dismiss the
   selector. Preserve existing links. Confirm the link appears in Development.
   A bare number can match unrelated PR bodies and hide the intended result.
4. Run the checker below. It verifies both the native link and the Project's
   **Linked pull requests** field. Fix any missing link in the browser and rerun.
5. Apply the intended status, then verify again after a merge or issue closure.
   Update the issue's progress/checklists from the actual result. Keep release,
   real-device verification, and other unmet criteria open.

```bash
# Read-only check, scoped to one issue and one or more same-repository PRs.
python3 scripts/sync_issue_project.py --issue 87 --pr 88

# After authorized linking: active open work -> In Progress; closed work -> Done.
python3 scripts/sync_issue_project.py --issue 87 --pr 88 \
  --open-status 'In Progress' --apply

# Historical link on deferred work: restore its previous Todo intent.
python3 scripts/sync_issue_project.py --issue 42 --pr 54 \
  --open-status Todo --apply
```

The default repository comes from `gh repo view`; use `--repo OWNER/REPO` outside
the checkout. Repeat `--pr` when several PRs implement the same issue. Omit `--pr`
only for a status-only check; its success output does not claim link verification. With multiple
Project memberships, select the intended existing Project using `--project-id`.
Archived items, inaccessible Projects, ambiguous fields, and truncated API lists
are reported as blockers rather than silently skipped.

| Issue state / intent | Result |
|---|---|
| Closed, including a historical link | Done |
| Open and explicitly active | In Progress |
| Open and explicitly deferred | Todo |
| Open, no explicit status requested | Preserve the current status; missing status becomes Todo |
| Open but already marked Done | Require an explicit Todo/In Progress choice |

The script is read-only unless `--apply` is passed, checks for concurrent changes,
and reads back the result. It never creates links or closes issues. Exit codes:
`0` = verified, `1` = missing link or unapplied status correction, `2` = API,
ambiguity, concurrency, or verification error. Do not report completion after a
nonzero exit. Creating a PR does not authorize unrelated issue closures.

**Why the browser step is needed:** GitHub interprets closing keywords only for
PRs targeting the default branch (`main` here), so they do not create native links
on feature PRs targeting `develop`. A closing keyword in a commit can close the
issue later without populating the PR link. See [GitHub's linking rules](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue).
Project link automations can also change Status when a merged PR is linked to an
already-closed issue, so run the status correction **after** linking. See
[built-in Project automations](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-built-in-automations).

Validate changes to the helper with:

```bash
python3 -B -m unittest discover -s scripts -p 'test_sync_issue_project.py'
```

## ライセンスと CLA

本プロジェクトは **[AGPL-3.0-or-later](../LICENSE)** です。判断の経緯は
[ADR-0027](./decisions/ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md)。

**コードを提供するには [CLA](../CLA.md) への同意が必要です。** 初回の PR で自動的に
案内が出るよう `cla-assistant` を導入する予定ですが、**まだ設定していません**。
それまでは、PR に「CLA.md に同意します」とコメントしてください。

CLA は**許諾型**です。**著作権はあなたに残ります。** プロジェクトに再ライセンス可能な
広い許諾を与えてもらう形で、これは将来 Mac App Store のような AGPL と両立しない配布経路を
選べるようにするためです（[ADR-0027 §4-5](./decisions/ADR-0027_ライセンスをAGPL-3.0とCLAに決定.md)）。

第三者のコードを含める場合は、そのライセンスを明示し、[`NOTICE`](../NOTICE) に追記できる形に
してください。**ライセンスが不明なコードは受け入れられません。**
