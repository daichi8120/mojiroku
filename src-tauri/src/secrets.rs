//! シークレット保管（BYOK API キー / OAuth トークン等）。
//!
//! 平文設定ファイルには置かない（docs/03_design/spec.md §8）。**リリース（署名済み .app）**では
//! keyring crate の apple-native バックエンド = login キーチェーンに保存する。サービス名は
//! バンドル識別子に固定し、account 名（"byok_api_key_*" 等）でエントリを分ける。
//!
//! ⚠️ **dev（debug ビルド = `tauri dev`）ではキーチェーンを使わず、アプリデータ
//! ディレクトリの平文 JSON `dev-secrets.json` に保存する**。理由: tauri dev の dev バイナリは
//! 再ビルド毎に署名同一性が変わり、キーチェーン項目の ACL が一致せず読み取りの度に許可
//! ダイアログ（パスワード入力）が頻発するため（ADR-0012）。dev の使い捨てトークンは平文で
//! 十分で、開発体験を優先する。実 keychain 経路はリリース／配布ゲートで検証する。
//! escape hatch: `MOJIROKU_USE_KEYCHAIN=1` を設定すると debug でも keychain を使う。
//!
//! セキュリティ方針: `get` はコマンドとして JS に公開しない（鍵を webview へ往復させない）。
//! 鍵を必要とする処理（要約のクラウド経路）が Rust 内で直接 `get` する。`has`/`set`/`delete`
//! のみをコマンド化し、UI には「保存済みか否か」だけを返す。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// キーチェーンのサービス名（バンドル識別子と一致させる）。dev の平文保存先ディレクトリ
/// （= app_data_dir）の末尾セグメントもこれと一致する。
const SERVICE: &str = "com.daichi0812.mojiroku";

/// dev の平文ストアの読み書きを直列化する（Google のトークン更新は access/refresh/expiry の
/// 3 キーを連続 set するため、read-modify-write の競合を避ける）。
static DEV_LOCK: Mutex<()> = Mutex::new(());

/// BYOK API キーの account 名（provider 別にスロットを分け、別 provider の鍵を
/// 取り違えて送らないようにする。フロントの byokKeyName と一致させる）。
pub fn byok_key_name(provider: &str) -> String {
    format!("byok_api_key_{provider}")
}

/// Notion 連携トークン（内部インテグレーション）のキーチェーン account 名。
/// フロントの NOTION_TOKEN_KEY と一致させる。鍵は JS へ出さず Rust 内のみで使う。
/// 書き手は OAuth 連携（`oauth::connect_notion`）、読み手は Notion エクスポート（`commands::export`）。
pub(crate) const NOTION_TOKEN_KEY: &str = "notion_token";

/// Slack Incoming Webhook URL のキーチェーン account 名。
/// フロントの SLACK_WEBHOOK_KEY と一致させる。webhook URL 自体が秘密で JS へは出さない。
/// 書き手は OAuth 連携（`oauth::connect_slack`）、読み手は Slack エクスポート（`commands::export`）。
pub(crate) const SLACK_WEBHOOK_KEY: &str = "slack_webhook_url";

/// 限定公開 iCal URL のキーチェーン account 名（フロントの CALENDAR_ICAL_KEY と一致）。
/// URL 自体が秘密（カレンダー読み取りのクレデンシャル）なので JS へは出さない（has/set/delete のみ公開）。
pub(crate) const CALENDAR_ICAL_KEY: &str = "calendar_ical_url";

/// dev（debug）かつ escape hatch 未指定なら平文ファイルストアを使う。
fn use_file_store() -> bool {
    cfg!(debug_assertions) && std::env::var_os("MOJIROKU_USE_KEYCHAIN").is_none()
}

// ───────────────────────── keychain（リリース経路） ─────────────────────────

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|e| format!("keychain entry: {e}"))
}

// ───────────────────────── 平文ファイル（dev 経路） ─────────────────────────

/// dev 平文ストアのパス（`~/Library/Application Support/com.daichi0812.mojiroku/dev-secrets.json`）。
/// Tauri の app_data_dir と同じ場所（モデルや settings.json と同居）。AppHandle 非依存にするため
/// macOS の規約パスを直接組み立てる（このアプリは macOS 専用）。
fn dev_store_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME 環境変数が無い")?;
    Ok(PathBuf::from(home)
        .join("Library/Application Support")
        .join(SERVICE)
        .join("dev-secrets.json"))
}

fn dev_load() -> Result<BTreeMap<String, String>, String> {
    let p = dev_store_path()?;
    match std::fs::read(&p) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("dev-secrets parse: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("dev-secrets read: {e}")),
    }
}

fn dev_save(map: &BTreeMap<String, String>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let p = dev_store_path()?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("dev-secrets mkdir: {e}"))?;
    }
    let json = serde_json::to_vec_pretty(map).map_err(|e| format!("dev-secrets serialize: {e}"))?;
    // tmp へ書いて rename で原子的に置換。パーミッションは所有者のみ（0600）。
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| format!("dev-secrets write: {e}"))?;
    let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    std::fs::rename(&tmp, &p).map_err(|e| format!("dev-secrets rename: {e}"))?;
    Ok(())
}

/// ロック取得（poison は無視して継続。dev 用途で堅牢性を優先）。
fn dev_lock() -> std::sync::MutexGuard<'static, ()> {
    DEV_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

// ───────────────────────────────── 公開 API ─────────────────────────────────

/// シークレットを保存（上書き）。
pub fn set(name: &str, value: &str) -> Result<(), String> {
    if use_file_store() {
        let _g = dev_lock();
        let mut m = dev_load()?;
        m.insert(name.to_string(), value.to_string());
        return dev_save(&m);
    }
    entry(name)?
        .set_password(value)
        .map_err(|e| format!("keychain set: {e}"))
}

/// シークレットを取得。未登録なら `Ok(None)`。鍵は JS へ出さず Rust 内のみで使う。
pub fn get(name: &str) -> Result<Option<String>, String> {
    if use_file_store() {
        let _g = dev_lock();
        return Ok(dev_load()?.get(name).cloned());
    }
    match entry(name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("keychain get: {e}")),
    }
}

/// シークレットを削除。未登録でも成功扱い（冪等）。
pub fn delete(name: &str) -> Result<(), String> {
    if use_file_store() {
        let _g = dev_lock();
        let mut m = dev_load()?;
        m.remove(name);
        return dev_save(&m);
    }
    match entry(name)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete: {e}")),
    }
}

/// 保存済みか（UI のバッジ表示用）。dev/release どちらも `get` の分岐に従う。
pub fn has(name: &str) -> Result<bool, String> {
    Ok(get(name)?.is_some())
}
