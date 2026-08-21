# mojiroku — dev orchestration
# 使い方: `just <recipe>`（要 `brew install just`） / npm スクリプトでも代替可（下記コメント参照）
# Tauri CLI はルート package.json に同梱。`npm install` 済みが前提。

# デフォルト: レシピ一覧
default:
    @just --list

# 開発起動（sidecar をビルド → tauri dev → frontend の Vite を beforeDevCommand で起動）
# dev バイナリの署名は .cargo/config.toml の runner（scripts/dev-sign-run.sh）が自動で行い、
# 再ビルド毎のキーチェーンパスワード要求を解消する。初回のみ scripts/dev-codesign-setup.sh。
dev:
    bash scripts/build-sidecar.sh
    npm run tauri dev

# 配布バンドル。sidecar を同梱（ローカルは無署名。配布用の Apple 署名+公証はリリース CI が行う。
# ローカルで署名ビルドを試す場合は APPLE_SIGNING_IDENTITY 等の env を設定＝ADR-0022）
build:
    bash scripts/build-sidecar.sh
    npm run tauri build

# フロントのみ起動
dev-frontend:
    npm --prefix frontend run dev

# 型チェック + clippy
lint:
    npm --prefix frontend exec tsc -- --noEmit
    cargo clippy --workspace --all-targets

# Format（Rust）
fmt:
    cargo fmt --all

# Test（Rust ワークスペース）
test:
    cargo test --workspace

# 構成ツリー表示（ドキュメントのディレクトリ規約と突き合わせ用）
tree:
    tree -a -I 'node_modules|target|dist|.git' -L 4
