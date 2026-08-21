// FFI 例外シールド（ffi_guard.rs と対）。
//
// 静的リンクした C++ 推論系（whisper.cpp / sherpa-onnx / onnxruntime）が投げる
// C++ 例外（std::bad_alloc / Ort::Exception 等）は Rust を素通りして
// tokio の catch_unwind に達し、"Rust cannot catch foreign exceptions" で
// プロセスごと abort する（docs/error.md の実クラッシュ 3 件の根本原因）。
// この関数は Rust から渡されたコールバックを try/catch で実行し、
// C++ 例外をエラーコード＋メッセージに変換して Rust に返す。
//
// ⚠️ catch (...) を書かないこと: Rust panic（外来例外）を C++ 側で握り潰すと
// "Rust panics must be rethrown" で abort する。std::exception 派生のみ捕捉し、
// Rust panic はこのフレームを素通りさせて Rust の通常の unwind に任せる
// （whisper/sherpa/ORT の例外は全て std::exception 派生）。

#include <cstddef>
#include <cstring>
#include <exception>
#include <stdexcept>

extern "C" {

typedef void (*mojiroku_guarded_fn)(void *);

int mojiroku_cpp_guard(mojiroku_guarded_fn f, void *ctx, char *err,
                       size_t err_len) {
  try {
    f(ctx);
    return 0;
  } catch (const std::exception &e) {
    if (err != nullptr && err_len > 0) {
      std::strncpy(err, e.what(), err_len - 1);
      err[err_len - 1] = '\0';
    }
    return 1;
  }
}

// テスト用フック: std::exception 派生（runtime_error）を投げる。
// ffi_guard.rs の単体テストが「C++ 例外 → Err 変換」を実証するために使う。
void mojiroku_cpp_guard_test_throw(void *) {
  throw std::runtime_error("test exception from C++");
}

} // extern "C"
