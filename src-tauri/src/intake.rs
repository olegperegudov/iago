//! Turning a PNG file on disk into a clip.
//!
//! Two folders feed the history this way — the one macOS drops screenshots into
//! and the one an edited picture comes back from — and everything between the
//! file and the card is the same for both: wait for the write to finish, read
//! the size out of the image, put it on the clipboard, file it. Only the
//! watching differs, so only the watching lives in the two callers.

use crate::history::{now_secs, History, Payload, SourceApp};
use std::path::Path;
use std::sync::Mutex;

/// A file worth reading: a PNG that is not the hidden temp file macOS writes a
/// capture into before renaming it into place — that one is half a picture.
pub fn is_png(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name.starts_with('.') {
        return false;
    }
    name.to_lowercase().ends_with(".png")
}

/// A filesystem event fires while the file is still being written. Wait until
/// its size stops growing before reading, or we hand the history a truncated PNG.
pub fn read_when_complete(path: &Path) -> Option<Vec<u8>> {
    let mut last = 0u64;
    for _ in 0..40 {
        let size = std::fs::metadata(path).ok()?.len();
        if size > 0 && size == last {
            return std::fs::read(path).ok();
        }
        last = size;
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    crate::debug_log::log(&format!("intake: {} never settled", path.display()));
    None
}

/// Clipboard first, so a plain Cmd+V straight after the capture or the save
/// pastes the picture instead of whatever was there before; then the history.
/// Our own clipboard write must not bounce back through the clipboard watcher
/// as a second copy of the same image, hence `skip_next`.
///
/// Returns whether the history actually changed — the caller redraws and saves
/// on that, and re-reading the same bytes must cost neither.
pub fn ingest(
    path: &Path,
    bytes: Vec<u8>,
    app: SourceApp,
    history: &Mutex<History>,
    skip_next: &Mutex<bool>,
) -> bool {
    let (width, height) = match image::load_from_memory(&bytes) {
        Ok(img) => (img.width(), img.height()),
        Err(e) => {
            crate::debug_log::log(&format!("intake: {} is not an image: {}", path.display(), e));
            return false;
        }
    };
    let payload = Payload::Image { png: bytes, width, height };

    if let Ok(mut skip) = skip_next.lock() {
        *skip = true;
    }
    if let Err(e) = crate::clipboard::write_clipboard(&payload) {
        crate::debug_log::log(&format!("intake: clipboard write failed: {}", e));
        if let Ok(mut skip) = skip_next.lock() {
            *skip = false;
        }
    }

    let added = match history.lock() {
        Ok(mut h) => h.add(payload, app, now_secs()),
        Err(_) => false,
    };
    crate::debug_log::log(&format!(
        "intake: {} ({}x{}) added={}",
        path.display(),
        width,
        height,
        added
    ));
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_half_written_temp_file_macos_renames_into_place_is_not_a_picture_yet() {
        assert!(is_png(&PathBuf::from("/tmp/Screenshot 2026-08-06.png")));
        assert!(is_png(&PathBuf::from("/tmp/SHOT.PNG")));
        assert!(!is_png(&PathBuf::from("/tmp/.screencapture-abc123.png")));
        assert!(!is_png(&PathBuf::from("/tmp/notes.txt")));
    }
}
