//! FFI 例外シールド。静的リンクした C++ 推論系（whisper.cpp / sherpa-onnx / onnxruntime）が
//! 投げる C++ 例外（メモリ枯渇時の std::bad_alloc、onnxruntime の Ort::Exception 等）を
//! `CoreError` に変換し、プロセス abort を防ぐ。
//!
//! 背景: C++ 例外は Rust のフレームを unwind で素通りし、tokio（spawn_blocking 等）の
//! catch_unwind に達した時点で "fatal runtime error: Rust cannot catch foreign exceptions"
//! として **プロセスごと abort** する。v0.3.0 実機のクラッシュ 3 件（docs/error.md）は
//! 全てこの経路だった。ここでは C++ 側（ffi_guard.cc）の try/catch で例外を境界内に
//! 封じ、エラーコード＋メッセージとして Rust に返す。
//!
//! 設計上の要点:
//! - トランポリンは `extern "C-unwind"`。Rust panic（例: 添字 panic）はシールドを
//!   **素通り**して通常の Rust unwind として上へ抜ける（C++ 側は catch (...) を
//!   持たないので Rust panic を握り潰さない）。挙動はシールド導入前と同一。
//! - 捕捉するのは `std::exception` 派生のみ（whisper/sherpa/ORT の例外は全て派生）。
//! - ggml の `GGML_ABORT`（abort() 直呼び）はこの仕組みでは防げない。それは例外ではなく
//!   即死シグナルであり、防ぐには要 プロセス分離（mojiroku-llm と同じ构図）。

use std::ffi::c_void;
use std::os::raw::c_char;

use crate::error::CoreError;

extern "C-unwind" {
    /// ffi_guard.cc の実体。`f(ctx)` を C++ の try/catch 内で実行する。
    /// 戻り値 0=正常、1=std::exception を捕捉（err にメッセージ）。
    fn mojiroku_cpp_guard(
        f: extern "C-unwind" fn(*mut c_void),
        ctx: *mut c_void,
        err: *mut c_char,
        err_len: usize,
    ) -> i32;

    /// テスト用: std::runtime_error を投げる（単体テストが変換経路を実証する用）。
    #[cfg(test)]
    fn mojiroku_cpp_guard_test_throw(ctx: *mut c_void);
}

/// トランポリンとクロージャの受け渡し。`out` は正常完了時のみ Some。
struct CallCtx<F, T> {
    f: Option<F>,
    out: Option<T>,
}

/// C++ シールドから呼び戻されるトランポリン。
/// `extern "C-unwind"`: C++ 例外（外向き）も Rust panic（内向き）も unwind を合法にする。
/// ここで catch_unwind してはいけない（外来例外が catch_unwind に触れた時点で abort するため）。
extern "C-unwind" fn trampoline<F: FnOnce() -> T, T>(ctx: *mut c_void) {
    let ctx = unsafe { &mut *ctx.cast::<CallCtx<F, T>>() };
    let f = ctx.f.take().expect("ffi_guard: closure invoked twice");
    ctx.out = Some(f());
}

/// `f` を C++ 例外シールド内で実行する。C++ 例外は `Err(CoreError::Native)` に変換され、
/// Rust panic はそのまま伝播する（従来挙動）。whisper / sherpa-onnx を呼ぶ経路は
/// 必ずこれを経由すること。
pub fn guard<T, F: FnOnce() -> T>(label: &str, f: F) -> Result<T, CoreError> {
    let mut err = [0u8; 512];
    let mut ctx = CallCtx::<F, T> { f: Some(f), out: None };
    let rc = unsafe {
        mojiroku_cpp_guard(
            trampoline::<F, T>,
            (&mut ctx as *mut CallCtx<F, T>).cast(),
            err.as_mut_ptr().cast(),
            err.len(),
        )
    };
    match rc {
        0 => Ok(ctx
            .out
            .expect("ffi_guard: guard returned 0 without a result")),
        _ => {
            let end = err.iter().position(|&b| b == 0).unwrap_or(err.len());
            let what = String::from_utf8_lossy(&err[..end]).into_owned();
            Err(CoreError::Native { label: label.to_string(), what })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_return_value() {
        let v = guard("test", || 42).unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn converts_cpp_exception_to_err() {
        // C++ 側テストフックで std::runtime_error を投げ、Err に変換されることを実証。
        let r = guard("stt", || unsafe {
            mojiroku_cpp_guard_test_throw(std::ptr::null_mut())
        });
        let err = r.unwrap_err().to_string();
        assert!(
            err.contains("test exception from C++"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn rust_panic_passes_through_unchanged() {
        // Rust panic はシールドを素通りし、通常の panic として catch_unwind で受けられる。
        let r = std::panic::catch_unwind(|| {
            let _ = guard("test", || panic!("rust panic passthrough"));
        });
        assert!(r.is_err());
    }
}
