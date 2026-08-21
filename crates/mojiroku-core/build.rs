// FFI 例外シールド（src/ffi_guard.cc）のビルド。
// C++ 例外を捕捉するため、例外を有効にしたまま（-fno-exceptions を付けない）コンパイルする。
fn main() {
    println!("cargo:rerun-if-changed=src/ffi_guard.cc");
    cc::Build::new()
        .cpp(true)
        .file("src/ffi_guard.cc")
        .flag_if_supported("-std=c++17")
        .compile("mojiroku_ffi_guard");

    // ggml-metal（whisper-rs-sys）の `@available` が要求する `__isPlatformVersionAtLeast`
    // （compiler-rt）を解決する。src-tauri/build.rs と同じ対処だが、`rustc-link-lib` は
    // 依存クレートの build.rs から**このクレートを含む最終バイナリ**へしか効かないため、
    // 本クレートの examples（transcribe_cli 等の release ビルド）用にここでも発行する。
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("apple") {
        if let Ok(out) = std::process::Command::new("clang")
            .arg("-print-runtime-dir")
            .output()
        {
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !dir.is_empty() {
                println!("cargo:rustc-link-search=native={dir}");
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        }
    }
}
