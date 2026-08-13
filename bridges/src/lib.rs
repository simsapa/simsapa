pub mod ai_engine;
pub mod api;
pub mod sutta_bridge;
pub mod asset_manager;
pub mod audio_manager;
pub mod storage_manager;
pub mod prompt_manager;
pub mod clipboard_manager;
pub mod dictionary_manager;
pub mod global_hotkey_manager;

use core::pin::Pin;
use cxx_qt::{CxxQtThread, Threading};

use simsapa_backend::logger::error;

/// Queue a closure onto the GUI thread, logging instead of panicking or failing
/// silently when the target QObject is gone.
///
/// `CxxQtThread::queue()` returns `Err(ThreadingQueueError::ObjectDestroyed)`
/// once its target `QObject` has been destroyed. The bridge objects are
/// **per-engine** — every window owns its own `SuttaBridge` / `AssetManager` /
/// … instance — so a background operation a window started holds a
/// `CxxQtThread` that starts failing the moment that window is destroyed. Since
/// the secondary windows are now destroyed on close, that is a live path.
///
/// Two idioms preceded this helper, and both were defects once destroy-on-close
/// landed: `.unwrap()` panicked the worker thread, and `let _ =` dropped the
/// error with no line in `log.txt` at all. In both cases the completion signal
/// is lost and whatever the completion handler owned — notably the
/// keep-screen-on lock — is never released.
///
/// `is_destroyed()` is documented by cxx-qt as racy and is **not** the fix;
/// handling the `Err` is.
///
/// Returns `true` if the closure was queued. Call sites decide whether a
/// failure means "return" (nothing further can be reported) or "continue"
/// (there is local cleanup still to do); the return value is deliberately not
/// `#[must_use]`, because most sites have nothing left to do either way.
pub fn queue_or_log<T, F>(qt_thread: &CxxQtThread<T>, context: &str, f: F) -> bool
where
    T: Threading,
    F: FnOnce(Pin<&mut T>) + Send + 'static,
{
    match qt_thread.queue(f) {
        Ok(()) => true,
        Err(e) => {
            error(&format!(
                "qt_thread.queue() failed in {context} (window closed?): {e}"
            ));
            false
        }
    }
}
