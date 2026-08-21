fn main() {
    // ggml-metal（whisper-rs-sys）の `@available`（residency sets 等の新しい macOS 機能）が要求する
    // `__isPlatformVersionAtLeast`（compiler-rt の os_version_check）を解決する。
    // Rust の最終リンクは `-nodefaultlibs` で clang_rt を含めないため、最終バイナリ側で
    // libclang_rt.osx.a を明示リンクする。ディレクトリは clang から動的取得（ハードコードしない）。
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

        // 会議モード（ADR-0017）: screencapturekit の Swift ブリッジが libswift_Concurrency.dylib
        // に依存する。cargo の `rustc-link-arg` は依存先（クレートの build.rs）から最終バイナリへ
        // **伝播しない**ため、消費側（この src-tauri）で Swift ランタイムへの -rpath を再発行する。
        // 無いと実行時に `dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
        // ... no LC_RPATH's found` で落ちる。
        //
        // ⚠️ 配布知見: 下の Xcode ツールチェーン path はユーザー機に無い。配布 .app では
        // この dylib を同梱（@executable_path/../Frameworks）するか OS 提供の /usr/lib/swift で
        // 解決できるかを別途検証する（Phase 7 の配布フェーズ）。
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        if let Ok(out) = std::process::Command::new("xcode-select").arg("-p").output() {
            if out.status.success() {
                let xcode = String::from_utf8_lossy(&out.stdout).trim().to_string();
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{xcode}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx"
                );
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{xcode}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"
                );
            }
        }
    }

    tauri_build::build()
}
