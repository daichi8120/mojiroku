// screencapturekit クレートは内部の Swift ブリッジが libswift_Concurrency.dylib に依存し、
// build.rs で Swift ランタイムへの -rpath を出している。だが cargo の `rustc-link-arg` は
// **依存先から最終バイナリへ伝播しない**（link-lib/link-search のみ伝播）。よって消費側で
// 同じ rpath を再発行しないと、実行時に `dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
// ... no LC_RPATH's found` で落ちる。
//
// ⚠️ 配布知見（ADR-0017）: 下の Xcode ツールチェーン path はユーザー機に無い可能性が高い。
// 実装時はこの dylib を .app に同梱（@executable_path/../Frameworks）するか、OS 提供の
// /usr/lib/swift（dyld 共有キャッシュ）で解決できるかを検証する必要がある。
fn main() {
    // OS 提供の Swift ランタイム（dyld 共有キャッシュ）。
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    // Xcode ツールチェーンの Swift Concurrency ランタイム（開発機向け）。
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
