//! モデル管理: 初回 DL / キャッシュ（`docs/03_design/spec.md` §10）。
//! HF `ggerganov/whisper.cpp` の ggml モデルを取得する。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// 既定の文字起こしモデル（large-v3-turbo q5_0, 約 547MiB）。
pub const DEFAULT_WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

/// 既定の要約モデル。実会議での品質ゲートを PASS 済み（docs/roadmap.md）。
///
/// **「既定」の意味は 2 つ。**小の段が配るモデルであり、かつどの段にも採用済みが
/// 無いときの落とし先（[`model_for_tier`]）。中・大の段は 2026-08-30 に
/// Qwen3.5-9B へ移った（ADR-0030）ので、**これはもう全員に配られるものではない**。
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

// ───────────────────────── 要約モデルの段（Issue #30） ─────────────────────────

/// 要約モデルの段。端末の搭載メモリで決まる。
///
/// 要約 sidecar はモデル全体をメモリに載せる。16GB 機ではメモリ枯渇でクラッシュした
/// 前例があり、重い ML ジョブをアプリ全体 1 本に直列化したのはその対策（ADR-0021）。
/// 段はその手前で「そもそも載せるものを小さくする」ための仕組み。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SummaryTier {
    /// 目安 8GB。いまは現行の 7B がここを受け持つ（軽い候補が未採用のため）。
    Small,
    /// 目安 16GB。
    Medium,
    /// 目安 32GB 以上。
    Large,
}

/// 要約モデル 1 件。DL 元・検証値・段をここに集約する。
///
/// **1 モデル 1 行にまとめている理由。** 以前は既定モデル名・base URL・期待ハッシュが
/// 別々の定数に散っていた。段ごとに違うモデルを持つと、その 3 つがモデル数だけ増えて
/// 対応が崩れる（URL は新しいのにハッシュは古い、など）。崩れても**ビルドは通る**ので、
/// 気づくのは利用者の DL が checksum_mismatch で落ちたときになる。
pub struct SummaryModel {
    /// ローカルのファイル名（`models_dir` 直下）。
    pub file: &'static str,
    /// HF の DL 元。**リビジョン固定**（`resolve/main` にしない。上流の差し替えで
    /// 下の `sha256` と食い違い、DL が恒久的に失敗するため）。
    base_url: &'static str,
    /// 期待 SHA-256。取得手順は HF の paths-info API（LFS の `oid`）。
    sha256: &'static str,
    /// ファイルサイズ（バイト）。設定 UI で「落とし直しに何 GB か」を見せるために持つ。
    pub size_bytes: u64,
    /// この段の候補であること。
    pub tier: SummaryTier,
    /// ライセンス。**利用者の商用利用を妨げないもの**だけを採用する（NOTICE と対応）。
    pub license: &'static str,
    /// 思考トレースを吐くモデルか（Qwen3 系など）。`true` なら sidecar に `--no-think` を渡す。
    ///
    /// **モデルの属性として持つ理由。** このフラグはプロンプトに `<think></think>` を足すので、
    /// 思考しないモデルに渡すと**出力が変わる**（Qwen2.5 で実測: 文言が変化した）。
    /// 「常に渡す」も「渡さない」も正しくない。渡すかどうかはモデルが決める。
    ///
    /// ⚠️ 渡し忘れると壊れ方が派手。アプリと同じ引数で Qwen3.5-9B を叩くと、
    /// **英語の `<think>` ブロックがそのまま stdout に出る**（2026-08-30 実測）。
    /// 利用者には議事録の代わりに思考トレースが見える。
    pub thinking: bool,
    /// 実際に配るか。**`false` の間は決して選ばれない。**
    ///
    /// 実会議での品質ゲートを取り直すまで、候補は候補のまま置く。
    /// 採用済みが無い段は [`DEFAULT_SUMMARY_MODEL`] へ落ちる（[`model_for_tier`]）。
    pub adopted: bool,
}

/// 要約モデルのカタログ。
///
/// 候補の質と速度は 11 モデル × 4 タスクの横断評価で測った（Issue #4 のコメント）。
/// **段の境界（何 GB で何を選ぶか）は未測定**で、下の定数は Issue #30 のたたき台のまま。
/// 測るべきは「載るか」ではなく「whisper・話者分離と同居して快適か」。
pub const SUMMARY_MODELS: &[SummaryModel] = &[
    // 小の段の候補。**未採用。**ピーク RSS 3.75GB は 7B より 2.5GB 軽く、速度も約 2 倍で、
    // 議事録の分量も 7B を上回る。それでも採らないのは、実会議のゲートで欠陥が 2 件残ったため
    // ——議事録の見出しを 1 つ落とし、タイトルに簡体字（报汇）が出た。どちらも利用者に見える。
    // 軽さは魅力だが、非力な端末に「軽くて雑」を配る理由にはならない。
    SummaryModel {
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        base_url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a09523/",
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        size_bytes: 2_740_937_888,
        tier: SummaryTier::Small,
        license: "apache-2.0",
        thinking: true,
        adopted: false,
    },
    // 既定であり、いまは**小の段の担当**。実会議での品質ゲート PASS 済み。
    // ⚠️ 小の段にふさわしく軽いからここに居るのではない。ピーク RSS は 6.26GB で、
    //    中の段の 9B（6.53GB）とほとんど変わらない。**軽い候補（4B・3.75GB）が
    //    まだ品質ゲートを通っていない**ので、非力な端末には従来どおりこれを配る
    //    ——「良くはならないが、悪くもならない」状態を保つための配置。
    SummaryModel {
        file: DEFAULT_SUMMARY_MODEL,
        base_url: SUMMARY_BASE,
        sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423",
        size_bytes: 4_683_074_240,
        tier: SummaryTier::Small,
        license: "apache-2.0",
        thinking: false,
        adopted: true,
    },
    // 中・大の段。**2026-08-30 に実会議で品質ゲートを取り直して採用した**（ADR-0030）。
    // 決め手は機械採点ではなく議事録の中身。現行は入力の長さに追従せず、27,947 字の会議でも
    // 410 字・8 行しか出さない。本モデルは同じ会議で 1,351 字・23 行を出す。
    // ピーク RSS は現行 +0.27GB（6.53 / 6.26）で、ファイルが 1.0GB 大きいわりに実行時は変わらない。
    // ⚠️ 思考モデルなので `--no-think` が要る（thinking: true）。渡さないと英語の
    //    `<think>` ブロックがそのまま出力に出る。
    SummaryModel {
        file: "Qwen3.5-9B-Q4_K_M.gguf",
        base_url: "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/3885219b6810b007914f3a7950a8d1b469d598a5/",
        sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
        size_bytes: 5_680_522_464,
        tier: SummaryTier::Medium,
        license: "apache-2.0",
        thinking: true,
        adopted: true,
    },
    // 大の段（12B 以上）は候補が無い。横断評価で 0/14 だったのは gemma-3-12b だけで、
    // ライセンスに使用制限がつく。Apache-2.0 の 12B 級は 2026-08-25 時点で見つからなかった。
];

const GIB: u64 = 1024 * 1024 * 1024;

/// 中の段の下限（**仮**。Issue #30 のたたき台で、実測で決め直す）。
pub const TIER_MEDIUM_MIN_BYTES: u64 = 16 * GIB;
/// 大の段の下限（**仮**。同上）。
pub const TIER_LARGE_MIN_BYTES: u64 = 32 * GIB;

// 境界の**値**は仮で、実測で動かす前提。順序だけは値が動いても成り立たなければならない
// （逆転すると `tier_for_memory` の match が上から評価される都合で Medium に到達しない）。
// 定数同士なのでコンパイル時に見る。
const _: () = assert!(TIER_MEDIUM_MIN_BYTES < TIER_LARGE_MIN_BYTES);

/// 搭載メモリから段を決める。
///
/// **分からないときは小さい方に倒す**（macOS 以外や取得失敗）。外した場合の損害が
/// 非対称だから — 大きすぎるモデルはメモリ枯渇でクラッシュしうる（ADR-0021）のに対し、
/// 小さすぎるモデルは要約の質が落ちるだけで、設定から上げ直せる。
pub fn tier_for_memory(total_memory_bytes: Option<u64>) -> SummaryTier {
    match total_memory_bytes {
        Some(b) if b >= TIER_LARGE_MIN_BYTES => SummaryTier::Large,
        Some(b) if b >= TIER_MEDIUM_MIN_BYTES => SummaryTier::Medium,
        _ => SummaryTier::Small,
    }
}

/// ファイル名からカタログ項目を引く。載っていなければ `None`。
pub fn summary_model(file: &str) -> Option<&'static SummaryModel> {
    SUMMARY_MODELS.iter().find(|m| m.file == file)
}

/// このモデルに `--no-think` を渡すべきか。
///
/// **カタログに無いファイルは `false`**（＝従来どおり渡さない）。手で置いたモデルに
/// 対して勝手にプロンプトを書き換えないため。壊れ方は「思考が出る」で、
/// 出力が黙って変わるより気づきやすい。
pub fn needs_no_think(file: &str) -> bool {
    summary_model(file).is_some_and(|m| m.thinking)
}

/// 既定モデルのカタログ項目。`SUMMARY_MODELS` に必ず 1 件ある（テストで保証）。
fn default_summary_model() -> &'static SummaryModel {
    SUMMARY_MODELS
        .iter()
        .find(|m| m.file == DEFAULT_SUMMARY_MODEL)
        .expect("DEFAULT_SUMMARY_MODEL は SUMMARY_MODELS に載せる")
}

/// 段に対して配るモデル。
///
/// **その段以下で、採用済みのうち一番上のものを選ぶ。**「ちょうどその段」だけを探すと、
/// 上の段に候補が無いときに既定へ落ちてしまい、**余裕のある端末ほど貧しいモデルを掴む**
/// という逆転が起きる（32GB 機が 16GB 機より悪いものを引く）。段は「これ以上は載せない」
/// という上限であって、ちょうど一致させる対象ではない。
///
/// どの段にも採用済みが無ければ [`DEFAULT_SUMMARY_MODEL`] へ落ちる。品質ゲートを
/// 取り直していないモデルを、段が決まったというだけで配らないため。
pub fn model_for_tier(tier: SummaryTier) -> &'static SummaryModel {
    SUMMARY_MODELS
        .iter()
        .filter(|m| m.adopted && m.tier <= tier)
        .max_by_key(|m| m.tier)
        .unwrap_or_else(|| default_summary_model())
}

/// 端末に合わせて要約モデルを選ぶ。**手元にあるものを優先する。**
///
/// 段の判定より先にキャッシュを見るのは、`models_dir` に既にあるモデルを黙って
/// 別のものに置き換えないため（Issue #30 の終了条件）。数 GB の再ダウンロードは、
/// 利用者にとって「勝手に始まった」以外の何物でもない。乗り換えは設定から明示的に行う。
pub fn select_summary_model(
    total_memory_bytes: Option<u64>,
    models_dir: &Path,
) -> &'static SummaryModel {
    let want = model_for_tier(tier_for_memory(total_memory_bytes));
    if cached(&models_dir.join(want.file)) {
        return want;
    }
    // 段の候補は無いが、別の登録モデルが手元にある場合。**それを使う。**
    // 複数あるなら小さい方（決定的に選ぶ、かつ載せて安全な方）。
    SUMMARY_MODELS
        .iter()
        .filter(|m| cached(&models_dir.join(m.file)))
        .min_by_key(|m| m.size_bytes)
        .unwrap_or(want)
}

/// 文字起こしモデルの DL URL。
pub fn whisper_model_url(file: &str) -> String {
    format!("{WHISPER_BASE}{file}")
}

/// 要約モデルの DL URL。カタログ（[`SUMMARY_MODELS`]）に載っていればその固定リビジョンを、
/// 載っていなければ既定モデルの repo を基準に組み立てる（従来どおり）。
pub fn summary_model_url(file: &str) -> String {
    match SUMMARY_MODELS.iter().find(|m| m.file == file) {
        Some(m) => format!("{}{}", m.base_url, m.file),
        None => format!("{SUMMARY_BASE}{file}"),
    }
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
        DEFAULT_VAD_MODEL => {
            Some("29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf")
        }
        DEFAULT_DIAR_EMB_MODEL => {
            Some("d51abcf31717ef28162f26acb9d44dd4127c3d44c9b8624f699f3425daca8e77")
        }
        // 要約モデルはカタログが持つ（段ごとに増えるため、ここに散らさない）。
        f => SUMMARY_MODELS
            .iter()
            .find(|m| m.file == f)
            .map(|m| m.sha256),
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

    // ───────── 要約モデルの段（Issue #30） ─────────

    /// **挙動が変わっていないことの証明。** カタログ化は純粋な移動で、既定モデルの
    /// DL URL と期待ハッシュは 1 バイトも変えていない。ここは refactor 前の値を
    /// 直接書いて固定する（カタログから引いて比べると、両方ずれても通ってしまう）。
    #[test]
    fn summary_registry_preserves_the_current_download() {
        assert_eq!(
            summary_model_url(DEFAULT_SUMMARY_MODEL),
            "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/\
             8911e8a47f92bac19d6f5c64a2e2095bd2f7d031/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
        );
        assert_eq!(
            expected_sha256(DEFAULT_SUMMARY_MODEL),
            Some("65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423")
        );
        // カタログに無いファイルは従来どおり既定 repo を基準に組み立てる。
        assert_eq!(
            summary_model_url("whatever.gguf"),
            format!("{SUMMARY_BASE}whatever.gguf")
        );
    }

    /// カタログの各行が揃っていること。URL とハッシュの対応が崩れてもビルドは通るので、
    /// ここで落とす（崩れたまま出荷すると、利用者の DL が checksum_mismatch で失敗する）。
    #[test]
    fn summary_registry_rows_are_wellformed() {
        assert!(!SUMMARY_MODELS.is_empty());
        for m in SUMMARY_MODELS {
            assert_eq!(m.sha256.len(), 64, "{}: SHA-256 hex は 64 桁", m.file);
            assert!(
                m.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: SHA-256 が 16 進でない",
                m.file
            );
            assert!(
                !m.base_url.contains("/resolve/main/"),
                "{}: DL 元はリビジョン固定にする（main だと上流差し替えで恒久的に失敗する）",
                m.file
            );
            assert!(
                m.base_url.ends_with('/'),
                "{}: base_url は / で終える",
                m.file
            );
            assert!(
                m.file.ends_with(".gguf"),
                "{}: 単一ファイル GGUF のみ",
                m.file
            );
            assert!(m.size_bytes > 0, "{}: サイズは設定 UI が使う", m.file);
            assert!(
                matches!(m.license, "apache-2.0" | "mit"),
                "{}: 利用者の商用利用を妨げないライセンスだけを載せる（{} は条文を読んでから）",
                m.file,
                m.license
            );
        }
        // 既定モデルは必ずカタログに居て、採用済みであること（落とし先だから）。
        let d = default_summary_model();
        assert!(d.adopted, "既定モデルが adopted=false だと落とし先が消える");
    }

    /// 段の境界。値は**仮**だが、境界そのものの振る舞い（以上/未満）は固定する。
    #[test]
    fn tier_boundaries_are_inclusive_lower_bounds() {
        let cases = [
            (Some(8 * GIB), SummaryTier::Small),
            (Some(TIER_MEDIUM_MIN_BYTES - 1), SummaryTier::Small),
            (Some(TIER_MEDIUM_MIN_BYTES), SummaryTier::Medium),
            (Some(TIER_LARGE_MIN_BYTES - 1), SummaryTier::Medium),
            (Some(TIER_LARGE_MIN_BYTES), SummaryTier::Large),
            (Some(128 * GIB), SummaryTier::Large),
        ];
        for (mem, want) in cases {
            assert_eq!(tier_for_memory(mem), want, "メモリ {mem:?} の段が違う");
        }
    }

    /// 取得できないときは小さい方へ倒す。外したときの損害が非対称だから
    /// （重すぎ = クラッシュしうる / 軽すぎ = 質が落ちるだけ）。
    #[test]
    fn unknown_memory_falls_to_the_small_tier() {
        assert_eq!(tier_for_memory(None), SummaryTier::Small);
        assert_eq!(tier_for_memory(Some(0)), SummaryTier::Small);
    }

    /// 段ごとに、いま実際に配られるモデル。**利用者に届くものが変わったら落ちる。**
    /// 差し替えは意図してこのテストを直すときだけ起きる。
    #[test]
    fn each_tier_resolves_to_the_intended_model() {
        assert_eq!(
            model_for_tier(SummaryTier::Small).file,
            DEFAULT_SUMMARY_MODEL,
            "小の段に採用済みが無いので既定へ落ちるはず"
        );
        for tier in [SummaryTier::Medium, SummaryTier::Large] {
            assert_eq!(
                model_for_tier(tier).file,
                "Qwen3.5-9B-Q4_K_M.gguf",
                "{tier:?} の段は 9B を配る"
            );
        }
    }

    /// **余裕のある端末が、貧しい端末より悪いモデルを掴まないこと。**
    /// 「ちょうどその段」だけを探す実装だと、上の段に候補が無いときに既定へ落ちて
    /// 逆転が起きる（32GB 機が 16GB 機より下のものを引く）。
    #[test]
    fn a_bigger_tier_never_gets_a_worse_model() {
        let tiers = [SummaryTier::Small, SummaryTier::Medium, SummaryTier::Large];
        for pair in tiers.windows(2) {
            let (lo, hi) = (model_for_tier(pair[0]), model_for_tier(pair[1]));
            assert!(
                hi.size_bytes >= lo.size_bytes,
                "{:?}({}) が {:?}({}) より小さいモデルを配っている",
                pair[1],
                hi.file,
                pair[0],
                lo.file
            );
        }
    }

    /// 1 つの段に採用済みが 2 つあると `model_for_tier` の選択が曖昧になる。
    #[test]
    fn at_most_one_adopted_model_per_tier() {
        for tier in [SummaryTier::Small, SummaryTier::Medium, SummaryTier::Large] {
            let n = SUMMARY_MODELS
                .iter()
                .filter(|m| m.adopted && m.tier == tier)
                .count();
            assert!(n <= 1, "{tier:?} の段に採用済みが {n} 件ある");
        }
    }

    /// **いまの既定に `--no-think` は渡さない。** 渡すとプロンプトが変わり、
    /// 出荷中のモデルの出力が変わる（Qwen2.5 で実測: 文言が変化した）。
    #[test]
    fn the_current_default_does_not_get_no_think() {
        assert!(!needs_no_think(DEFAULT_SUMMARY_MODEL));
        assert!(!default_summary_model().thinking);
    }

    /// **Qwen3 系には必ず渡す。** 渡さないと英語の `<think>` ブロックが
    /// そのまま出力に出る（2026-08-30 に sidecar で実測）。
    #[test]
    fn qwen3_family_models_get_no_think() {
        let q3: Vec<_> = SUMMARY_MODELS
            .iter()
            .filter(|m| m.file.starts_with("Qwen3"))
            .collect();
        assert!(!q3.is_empty(), "Qwen3 系がカタログに 1 件も無い");
        for m in q3 {
            assert!(m.thinking, "{}: Qwen3 系なのに thinking=false", m.file);
            assert!(needs_no_think(m.file), "{}: --no-think が渡らない", m.file);
        }
    }

    /// カタログに無いファイルは従来どおり（渡さない）。手で置いたモデルの
    /// プロンプトを勝手に書き換えない。
    #[test]
    fn unknown_models_keep_the_previous_behaviour() {
        assert!(summary_model("not-in-catalog.gguf").is_none());
        assert!(!needs_no_think("not-in-catalog.gguf"));
    }

    fn models_dir_with(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mojiroku-tier-{}-{:?}",
            files.len(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in files {
            fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    /// **手元にあるものを黙って置き換えない**（Issue #30 の終了条件）。
    /// 段が別のモデルを指していても、既に落としてあるものがあればそれを返す。
    ///
    /// ⚠️ このテストは名前に反して `select_summary_model` の**キャッシュ優先の分岐を
    /// 踏んでいない**。手元が 1 件だけだと、その分岐を消しても後段の「手元の最小」が
    /// 同じ答えを返すため。実際に変異を入れて素通りすることを確かめた。
    /// 分岐を固定しているのは [`multiple_cached_models_resolve_deterministically`]。
    #[test]
    fn cached_model_wins_over_the_tier_choice() {
        let candidate = SUMMARY_MODELS
            .iter()
            .find(|m| !m.adopted)
            .expect("候補が 1 件も無い");
        let dir = models_dir_with(&[candidate.file]);

        // どの搭載メモリでも、手元の候補が選ばれる（数 GB の再 DL を起こさない）。
        for mem in [None, Some(8 * GIB), Some(16 * GIB), Some(64 * GIB)] {
            assert_eq!(
                select_summary_model(mem, &dir).file,
                candidate.file,
                "メモリ {mem:?}: 手元のモデルを無視して別のものを選んだ"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    /// 手元に何も無ければ段の選択どおり。**新規インストールがここを通る。**
    #[test]
    fn empty_models_dir_falls_back_to_the_tier_choice() {
        let dir = models_dir_with(&[]);
        let cases = [
            // 搭載メモリが取れない・非力な端末は従来どおり既定。
            (None, DEFAULT_SUMMARY_MODEL),
            (Some(8 * GIB), DEFAULT_SUMMARY_MODEL),
            // 16GB 以上は 9B。ここが「既定を良くする」の実体。
            (Some(16 * GIB), "Qwen3.5-9B-Q4_K_M.gguf"),
            (Some(64 * GIB), "Qwen3.5-9B-Q4_K_M.gguf"),
        ];
        for (mem, want) in cases {
            assert_eq!(select_summary_model(mem, &dir).file, want, "メモリ {mem:?}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    /// **既存利用者に数 GB の再ダウンロードを起こさない。**
    /// 手元に 7B があるなら、段が 9B を指していてもそのまま使う（Issue #30 の終了条件）。
    #[test]
    fn an_existing_install_is_not_upgraded_behind_the_users_back() {
        let dir = models_dir_with(&[DEFAULT_SUMMARY_MODEL]);
        for mem in [Some(16 * GIB), Some(64 * GIB), Some(128 * GIB)] {
            assert_eq!(
                select_summary_model(mem, &dir).file,
                DEFAULT_SUMMARY_MODEL,
                "メモリ {mem:?}: 手元の既定を無視して 9B を落としにいった"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    /// 複数あるときは決定的に選ぶ（小さい方＝載せて安全な方）。
    #[test]
    fn multiple_cached_models_resolve_deterministically() {
        let files: Vec<&str> = SUMMARY_MODELS.iter().map(|m| m.file).collect();
        let dir = models_dir_with(&files);
        // 既定は手元にあるので、段の選択（＝既定）がそのまま通る。
        assert_eq!(select_summary_model(None, &dir).file, DEFAULT_SUMMARY_MODEL);

        // 既定だけ消すと、残りのうち最小が選ばれる。
        fs::remove_file(dir.join(DEFAULT_SUMMARY_MODEL)).unwrap();
        let smallest = SUMMARY_MODELS
            .iter()
            .filter(|m| m.file != DEFAULT_SUMMARY_MODEL)
            .min_by_key(|m| m.size_bytes)
            .unwrap();
        assert_eq!(select_summary_model(None, &dir).file, smallest.file);
        let _ = fs::remove_dir_all(&dir);
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
