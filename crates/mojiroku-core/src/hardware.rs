//! 端末のスペック取得。いまは搭載メモリだけ（Issue #30）。
//!
//! **なぜ「搭載」で「空き」ではないか。**
//! 空きメモリは起動のたびに変わる。同じ端末が、ブラウザを開いているかどうかで違う段に落ちる。
//! 落とし直しが数 GB のダウンロードである以上、判定は端末ごとに安定していないと使えない。
//! 空きメモリを見るなら、それは「いま実行してよいか」の判断（ADR-0021 の直列化）であって、
//! 「何を落とすか」の判断ではない。
//!
//! **macOS 以外は未対応。** 配布対象が macOS だけなので（ADR-0011/0022）、他 OS では
//! `None` を返す。呼び出し側は「分からない」を段の既定へ落とす（`models::tier_for_memory`）。

/// 搭載物理メモリ（バイト）。取得できなければ `None`。
///
/// macOS の `sysctl hw.memsize` を読む。`sysinfo` クレートを足さないのは、必要なのが
/// この 1 つの数値だけで、`libc` が既に依存グラフに居るため。
#[cfg(target_os = "macos")]
pub fn total_memory_bytes() -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: name は NUL 終端の C 文字列リテラル。oldp は u64 一つ分の書き込み可能な領域を
    // 指し、oldlenp はその長さ。newp は null（読み取り専用の呼び出し）。
    let rc = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut value as *mut u64).cast::<libc::c_void>(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    // rc != 0 は失敗。0 バイトは「取れた」とみなさない（後段の割り算・比較が壊れる）。
    if rc != 0 || value == 0 {
        return None;
    }
    Some(value)
}

/// 搭載物理メモリ（バイト）。macOS 以外では未対応のため常に `None`。
#[cfg(not(target_os = "macos"))]
pub fn total_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機で値が取れること。**上限も下限も緩く見る** — 特定の機種に縛ると CI や
    /// 将来の端末で落ちる。ここで見たいのは「単位を間違えていないか」だけ。
    #[test]
    #[cfg(target_os = "macos")]
    fn reads_plausible_total_memory() {
        let mem = total_memory_bytes().expect("macOS では搭載メモリを取得できる");
        const GB: u64 = 1024 * 1024 * 1024;
        assert!(
            (2 * GB..=1024 * GB).contains(&mem),
            "搭載メモリが桁違い（単位の取り違えを疑う）: {mem} バイト"
        );
    }
}
