//! コア共通のエラー型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    /// I/O 系
    #[error("io error: {0}")]
    Io(String),

    /// 音声デコード / リサンプル
    #[error("audio error: {0}")]
    Audio(String),

    /// モデル関連（DL / ロード / 推論失敗など）
    #[error("model error: {0}")]
    Model(String),

    /// 永続化（SQLite）
    #[error("db error: {0}")]
    Db(String),

    /// カレンダー連携（iCal フィード取得 / パース）。
    /// ⚠️ 文字列に**秘密の iCal URL を含めない**（URL 自体がカレンダー読み取りのクレデンシャル）。
    #[error("calendar error: {0}")]
    Calendar(String),

    /// ネイティブ（C++）例外を FFI 例外シールドで捕捉したもの（`ffi_guard`）。
    /// 典型はメモリ枯渇時の std::bad_alloc / onnxruntime の Ort::Exception。
    /// ユーザ向けに「メモリ不足の可能性」を含める（16GB 機で重処理が重なると発生しうる）。
    #[error("{label} がネイティブ例外で失敗しました: {what}（メモリ不足の可能性。他のアプリや処理を閉じて再試行してください）")]
    Native { label: String, what: String },
}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Db(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
