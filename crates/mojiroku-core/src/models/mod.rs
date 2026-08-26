//! モデル管理: 初回 DL / キャッシュ（`docs/03_design/spec.md` §10）。
//! HF `ggerganov/whisper.cpp` の ggml モデルを取得する。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// 既定の文字起こしモデル（large-v3-turbo q5_0, 約 547MiB）。
pub const DEFAULT_WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

/// 既定の要約モデル（**候補**。品質ゲートで最終決定。Apache-2.0・単一ファイル GGUF）。
/// 小さく軽い候補は `Qwen2.5-1.5B-Instruct-Q4_K_M.gguf` 等。
pub const DEFAULT_SUMMARY_MODEL: &str = "Qwen2.5-7B-Instruct-Q4_K_M.gguf";

/// VAD モデル（Silero, ggml）。whisper の無音ハルシネーション対策（spec の VAD 段）。
pub const DEFAULT_VAD_MODEL: &str = "ggml-silero-v5.1.2.bin";

/// 話者分離 segmentation（pyannote segmentation-3.0, ONNX, 約 6MB, MIT）。展開後のローカル名。
/// ADR-0009 は reverb-diarization-v1 を採っていたが、ライセンス（Rev Model Non-Production）が
/// 非商用・非本番限定と判明したため 2026-08-21 に差し替えた（ADR-0028）。
/// ADR-0009 の「被覆」指標は過剰検出を報奨していた（reverb は無音も発話と塗る）。
/// Silero VAD 参照で recall/precision に分けると reverb 98.1%/63.8% に対し本モデルは
/// 94.7%/92.8%（会議A 実測）。取りこぼしは 3.4pt 差で precision は大幅に上（ADR-0028）。
pub const DEFAULT_DIAR_SEG_MODEL: &str = "sherpa-pyannote-segmentation-3-0.onnx";
/// 話者分離 embedding（NeMo TitaNet-large, ONNX, 101MB）。日本語話者を分離（ADR-0009）。
pub const DEFAULT_DIAR_EMB_MODEL: &str = "nemo_titanet_large.onnx";

// DL 元はコミット SHA でリビジョン固定する（`resolve/main` だと上流のファイル差し替えで
// 下の期待 SHA-256 と食い違い、DL が恒久的に失敗するため）。モデルを更新するときは
// リビジョンと expected_sha256 の両方を揃えて更新すること。
const WHISPER_BASE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/";
const SUMMARY_BASE: &str =
    "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/8911e8a47f92bac19d6f5c64a2e2095bd2f7d031/";
const VAD_BASE: &str =
    "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/";
/// k2-fsa の話者分離モデル release（segmentation は tar.bz2、embedding は単体 onnx）。
const DIAR_SEG_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const DIAR_EMB_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_large.onnx";

/// 話者分離 embedding（TitaNet）の DL URL（単体 onnx、`ensure_model` で取得可）。
pub fn diar_emb_url() -> &'static str {
    DIAR_EMB_URL
}

/// 文字起こしモデルの DL URL。
pub fn whisper_model_url(file: &str) -> String {
    format!("{WHISPER_BASE}{file}")
}

/// 要約モデルの DL URL。
pub fn summary_model_url(file: &str) -> String {
    format!("{SUMMARY_BASE}{file}")
}

/// VAD モデルの DL URL（whisper-vad リポジトリ）。
pub fn vad_model_url(file: &str) -> String {
    format!("{VAD_BASE}{file}")
}

/// 既定モデルの期待 SHA-256（DL 完了時に検証。既存キャッシュの再検証はしない —
/// 4.4GB の再ハッシュは起動を遅くしすぎる。脅威モデルは截断/改竄された DL の検出）。
/// 出所: HF は LFS メタデータ（paths-info API）、GitHub アセットは実 DL の実測値。
/// いずれも 2026-07-05 取得・ローカル既存ファイルとの突き合わせ済み。
fn expected_sha256(model_file: &str) -> Option<&'static str> {
    match model_file {
        DEFAULT_WHISPER_MODEL => {
            Some("394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2")
        }
        DEFAULT_SUMMARY_MODEL => {
            Some("65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423")
        }
        DEFAULT_VAD_MODEL => {
            Some("29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf")
        }
        DEFAULT_DIAR_EMB_MODEL => {
            Some("d51abcf31717ef28162f26acb9d44dd4127c3d44c9b8624f699f3425daca8e77")
        }
        _ => None,
    }
}

/// 話者分離 segmentation アーカイブ（tar.bz2）自体の期待 SHA-256（展開前に検証）。
const DIAR_SEG_ARCHIVE_SHA256: &str =
    "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488";

/// 進捗コールバック: `(downloaded_bytes, total_bytes_opt)`。
pub type ProgressFn<'a> = dyn Fn(u64, Option<u64>) + 'a;

/// キャッシュ判定: `dest` が存在し空でなければ true（DL 済みとみなす）。
fn cached(dest: &Path) -> bool {
    dest.exists() && fs::metadata(dest).map(|m| m.len() > 0).unwrap_or(false)
}

/// `url` をストリーミングで `tmp` に書き出す。request 失敗は user-facing の安定キー
/// `CoreError::Model("error.model.download: {req_err_label}: {e}")`（オフライン初回起動で普通に
/// 起きるため。表示文言はフロントの i18n 辞書が持つ）、IO は `CoreError::Io`。
/// 呼び出し側は tmp の rename/展開を担う（本関数は返る前に file を close する）。
/// ensure_model / ensure_diar_seg_model で逐語重複していた DL ループを集約。
/// ダウンロード失敗のエラーキーを選ぶ。
///
/// 証明書の検証失敗を「ネットワーク接続を確認してください」と出すと、利用者は回線を疑う。
/// 実際には TLS を検査する中間装置（社内・学内プロキシ / VPN / セキュリティソフト）が原因で、
/// 回線は正常なことが多い。**原因の方向を指す文言に振り分ける**（Issue #31 の報告がこれ）。
fn download_error_key(req_err_label: &str, err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    let key = if lower.contains("certificate")
        || lower.contains("unknownissuer")
        || lower.contains("tls connection init failed")
    {
        "error.model.download_tls"
    } else {
        "error.model.download"
    };
    format!("{key}: {req_err_label}: {err}")
}

fn download_to_file(
    url: &str,
    tmp: &Path,
    req_err_label: &str,
    expected_sha256: Option<&str>,
    on_progress: Option<&ProgressFn<'_>>,
) -> Result<()> {
    // ureq は既定でリダイレクトを追従（HF → CDN）。
    let resp = ureq::get(url)
        .call()
        .map_err(|e| CoreError::Model(download_error_key(req_err_label, &e.to_string())))?;
    let total: Option<u64> = resp.header("Content-Length").and_then(|s| s.parse().ok());

    let mut file = fs::File::create(tmp).map_err(|e| CoreError::Io(e.to_string()))?;
    let mut reader = resp.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| CoreError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| CoreError::Io(e.to_string()))?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if let Some(cb) = on_progress {
            cb(downloaded, total);
        }
    }
    file.flush().ok();
    drop(file);
    verify_download(
        tmp,
        downloaded,
        total,
        &format!("{:x}", hasher.finalize()),
        expected_sha256,
        req_err_label,
    )
}

/// DL 結果の完全性検証。失敗時は tmp を削除して Err を返す（截断ファイルを rename させない）。
/// - サイズ: コネクション切断が正常な EOF として現れると截断ファイルがキャッシュ化され、
///   以後モデルロード失敗が固定化する（ユーザーは手動削除でしか回復できない）ため必須。
/// - SHA-256: 上流の差し替え・改竄・経路破損の検出（URL はリビジョン固定済み）。
fn verify_download(
    tmp: &Path,
    downloaded: u64,
    total: Option<u64>,
    actual_sha256: &str,
    expected_sha256: Option<&str>,
    req_err_label: &str,
) -> Result<()> {
    if let Some(total) = total {
        if downloaded != total {
            let _ = fs::remove_file(tmp);
            return Err(CoreError::Model(format!(
                "error.model.download_incomplete: {req_err_label}: {downloaded}/{total} bytes"
            )));
        }
    }
    if let Some(expected) = expected_sha256 {
        if !actual_sha256.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(tmp);
            return Err(CoreError::Model(format!(
                "error.model.checksum_mismatch: {req_err_label}: got {actual_sha256}"
            )));
        }
    }
    Ok(())
}

/// モデルが無ければ HF から DL し、ローカルパスを返す。既存なら即返す。
pub fn ensure_model(
    model_file: &str,
    url: &str,
    models_dir: &Path,
    on_progress: Option<&ProgressFn<'_>>,
) -> Result<PathBuf> {
    fs::create_dir_all(models_dir).map_err(|e| CoreError::Io(e.to_string()))?;
    let dest = models_dir.join(model_file);
    if cached(&dest) {
        return Ok(dest);
    }

    let tmp = dest.with_extension("part");
    download_to_file(
        url,
        &tmp,
        "download request",
        expected_sha256(model_file),
        on_progress,
    )?;
    fs::rename(&tmp, &dest).map_err(|e| CoreError::Io(e.to_string()))?;
    Ok(dest)
}

/// 話者分離 segmentation モデルを確保する。k2-fsa は tar.bz2 で配布するため、
/// アーカイブを DL → 内部の `model.onnx`（int8 ではなく f32）を `DEFAULT_DIAR_SEG_MODEL`
/// として展開する。既存なら即返す。embedding（TitaNet 単体 onnx）は `ensure_model` でよい。
pub fn ensure_diar_seg_model(
    models_dir: &Path,
    on_progress: Option<&ProgressFn<'_>>,
) -> Result<PathBuf> {
    fs::create_dir_all(models_dir).map_err(|e| CoreError::Io(e.to_string()))?;
    let dest = models_dir.join(DEFAULT_DIAR_SEG_MODEL);
    if cached(&dest) {
        return Ok(dest);
    }

    // 1) アーカイブを一時ファイルへ DL（展開前にアーカイブ自体を検証）
    let archive = dest.with_extension("tar.bz2.part");
    download_to_file(
        DIAR_SEG_ARCHIVE_URL,
        &archive,
        "diar seg download",
        Some(DIAR_SEG_ARCHIVE_SHA256),
        on_progress,
    )?;

    // 2) bzip2 → tar 展開し、末尾が `/model.onnx` のエントリを取り出す（int8 は除外）。
    let f = fs::File::open(&archive).map_err(|e| CoreError::Io(e.to_string()))?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(f));
    let entries = tar
        .entries()
        .map_err(|e| CoreError::Model(format!("diar seg tar: {e}")))?;
    let tmp = dest.with_extension("onnx.part");
    let mut found = false;
    for entry in entries {
        let mut entry = entry.map_err(|e| CoreError::Model(format!("diar seg entry: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| CoreError::Model(format!("diar seg path: {e}")))?
            .into_owned();
        if path.file_name().and_then(|s| s.to_str()) == Some("model.onnx") {
            let mut w = fs::File::create(&tmp).map_err(|e| CoreError::Io(e.to_string()))?;
            std::io::copy(&mut entry, &mut w).map_err(|e| CoreError::Io(e.to_string()))?;
            w.flush().ok();
            found = true;
            break;
        }
    }
    let _ = fs::remove_file(&archive);
    if !found {
        let _ = fs::remove_file(&tmp);
        return Err(CoreError::Model(
            "diar seg: model.onnx not found in archive".into(),
        ));
    }
    fs::rename(&tmp, &dest).map_err(|e| CoreError::Io(e.to_string()))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "abc" の SHA-256（既知ベクタ）。
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    /// 証明書の失敗を接続の失敗と区別する。**Issue #31 で実際に報告された文字列**を固定する。
    /// 分けないと「ネットワーク接続を確認してください」と出て、利用者は正常な回線を疑う。
    #[test]
    fn certificate_failures_get_their_own_error_key() {
        let reported = "Connection Failed: tls connection init failed: \
                        invalid peer certificate: UnknownIssuer";
        assert!(
            download_error_key("download request", reported).starts_with("error.model.download_tls:"),
            "報告された証明書エラーが download_tls に振り分けられていない"
        );

        // 大文字小文字や表現が違っても拾う。
        for s in [
            "invalid peer certificate: UnknownIssuer",
            "TLS connection init failed",
            "certificate verify failed",
        ] {
            assert!(
                download_error_key("l", s).starts_with("error.model.download_tls:"),
                "{s} が download_tls に振り分けられていない"
            );
        }

        // 証明書と無関係な失敗は従来のキーのまま（過剰に振り分けない）。
        for s in ["Connection Failed: dns error", "io: timed out", "status code 503"] {
            assert!(
                download_error_key("l", s).starts_with("error.model.download:"),
                "{s} が誤って download_tls に振り分けられた"
            );
        }

        // 元のメッセージは失われない（原因の特定に要る）。
        assert!(download_error_key("download request", reported).contains("UnknownIssuer"));
    }

    fn tmp_file(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(name);
        fs::write(&p, b"abc").unwrap();
        p
    }

    #[test]
    fn verify_download_accepts_matching_size_and_hash() {
        let p = tmp_file("mojiroku-verify-ok.part");
        assert!(verify_download(&p, 3, Some(3), ABC_SHA256, Some(ABC_SHA256), "t").is_ok());
        // 大文字の期待値も許容（eq_ignore_ascii_case）。
        let upper = ABC_SHA256.to_uppercase();
        assert!(verify_download(&p, 3, Some(3), ABC_SHA256, Some(&upper), "t").is_ok());
        // 成功時はファイルを消さない（呼び出し側が rename する）。
        assert!(p.exists());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn verify_download_rejects_truncated_and_removes_tmp() {
        let p = tmp_file("mojiroku-verify-trunc.part");
        let err = verify_download(&p, 3, Some(10), ABC_SHA256, None, "t").unwrap_err();
        assert!(err.to_string().contains("error.model.download_incomplete"));
        assert!(!p.exists(), "截断検出時は tmp を削除する");
    }

    #[test]
    fn verify_download_rejects_hash_mismatch_and_removes_tmp() {
        let p = tmp_file("mojiroku-verify-hash.part");
        let err =
            verify_download(&p, 3, Some(3), ABC_SHA256, Some("deadbeef"), "t").unwrap_err();
        assert!(err.to_string().contains("error.model.checksum_mismatch"));
        assert!(!p.exists(), "ハッシュ不一致時は tmp を削除する");
    }

    #[test]
    fn verify_download_skips_checks_when_unknown() {
        // Content-Length 無し + 期待ハッシュ無し（未知モデル）は素通し（従来挙動）。
        let p = tmp_file("mojiroku-verify-skip.part");
        assert!(verify_download(&p, 3, None, ABC_SHA256, None, "t").is_ok());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn default_models_have_expected_hashes() {
        for f in [
            DEFAULT_WHISPER_MODEL,
            DEFAULT_SUMMARY_MODEL,
            DEFAULT_VAD_MODEL,
            DEFAULT_DIAR_EMB_MODEL,
        ] {
            let h = expected_sha256(f).expect("既定モデルには期待ハッシュを定義する");
            assert_eq!(h.len(), 64, "{f}: SHA-256 hex は 64 桁");
        }
        assert_eq!(DIAR_SEG_ARCHIVE_SHA256.len(), 64);
        // URL はリビジョン固定（main を含まない）であること。
        for base in [WHISPER_BASE, SUMMARY_BASE, VAD_BASE] {
            assert!(!base.contains("/resolve/main/"), "{base} はリビジョン固定にする");
        }
    }

    #[test]
    #[ignore = "network: 約7MB tar.bz2 DL + 展開"]
    fn download_and_extract_diar_seg_model() {
        let dir = std::env::temp_dir().join("mojiroku-diarseg-test");
        let _ = fs::remove_file(dir.join(DEFAULT_DIAR_SEG_MODEL));
        let path = ensure_diar_seg_model(&dir, None).unwrap();
        assert!(path.exists());
        let len = fs::metadata(&path).unwrap().len();
        // pyannote seg-3.0 の f32 model.onnx は約 6.0MB（int8 の 1.5MB ではないこと）。
        assert!(len > 5_000_000, "expected f32 model.onnx ~6.0MB, got {len}");
        // 一時アーカイブが残っていないこと。
        assert!(!dir
            .join("sherpa-pyannote-segmentation-3-0.tar.bz2.part")
            .exists());
    }

    #[test]
    #[ignore = "network + 547MB download"]
    fn download_default_model() {
        let dir = std::env::temp_dir().join("mojiroku-models-test");
        let path = ensure_model(
            DEFAULT_WHISPER_MODEL,
            &whisper_model_url(DEFAULT_WHISPER_MODEL),
            &dir,
            None,
        )
        .unwrap();
        assert!(path.exists());
        assert!(fs::metadata(&path).unwrap().len() > 100_000_000);
    }
}
