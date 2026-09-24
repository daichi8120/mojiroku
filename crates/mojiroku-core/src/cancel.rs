//! 実行中の重い処理の中断（Issue #114）。
//!
//! ジョブワーカーは処理を 1 本のスレッド（`spawn_blocking`）で最後まで回す。そのスレッドに
//! 中断フラグを結び付けておき（[`scope`]）、コアは次の 2 か所でフラグを見る。
//! - 段階の境目（デコード → 文字起こし → 話者分離 → マージ）: [`check`]
//! - whisper の推論中: whisper.cpp の abort コールバック（`stt` がフラグのポインタを渡す）
//!
//! 話者分離（sherpa-onnx）は途中で止める手段が無いので、次の境目で止まる。
//! フラグはスレッドローカルに置く。パイプラインは内部でスレッドを分けないので、
//! 呼び出しの引数を増やさずに全段へ届く。

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{CoreError, Result};

thread_local! {
    static CURRENT: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// このスレッドで動く処理に中断フラグを結び付ける。戻り値を落とすと外れる。
pub fn scope(flag: Arc<AtomicBool>) -> ScopeGuard {
    let prev = CURRENT.with(|c| c.replace(Some(flag)));
    ScopeGuard { prev }
}

pub struct ScopeGuard {
    prev: Option<Arc<AtomicBool>>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let prev = self.prev.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

/// このスレッドの中断フラグ（結び付いていなければ None）。
pub fn current() -> Option<Arc<AtomicBool>> {
    CURRENT.with(|c| c.borrow().clone())
}

/// 中断が求められていれば `CoreError::Cancelled`。
pub fn check() -> Result<()> {
    if current().is_some_and(|f| f.load(Ordering::Relaxed)) {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_follows_the_flag_only_inside_the_scope() {
        assert!(check().is_ok(), "no scope, never cancelled");
        let flag = Arc::new(AtomicBool::new(false));
        {
            let _g = scope(Arc::clone(&flag));
            assert!(check().is_ok());
            flag.store(true, Ordering::Relaxed);
            assert!(matches!(check(), Err(CoreError::Cancelled)));
        }
        assert!(check().is_ok(), "scope ended");
    }

    #[test]
    fn scope_is_per_thread() {
        let flag = Arc::new(AtomicBool::new(true));
        let _g = scope(flag);
        assert!(check().is_err());
        std::thread::spawn(|| assert!(check().is_ok())).join().unwrap();
    }
}
