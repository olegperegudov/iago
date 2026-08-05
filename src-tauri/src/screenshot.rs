//! Makes a fresh screenshot land in the history immediately.
//!
//! Shift-Cmd-4 does not copy anything: it writes a PNG into the screenshot
//! folder. So we watch that folder with filesystem events (not a poll — a 1s
//! poll adds up to a second of "where is my screenshot"), and the moment a new
//! capture appears we put it both in the history and on the clipboard, so plain
//! Cmd+V pastes it too.
//!
//! macOS-only. On Windows PrintScreen already puts the bitmap on the clipboard,
//! where the regular watcher picks it up.

use crate::history::History;
use crate::{intake, source_app};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// System-wide default when the user never moved the screenshot folder.
const DEFAULT_DIR: &str = "Desktop";

/// Reads the folder macOS saves screenshots into (Cmd+Shift+5 → Options).
pub fn screenshot_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    match read_pref("location") {
        Some(loc) if !loc.is_empty() => {
            let expanded = if let Some(rest) = loc.strip_prefix("~/") {
                home.join(rest)
            } else {
                PathBuf::from(loc)
            };
            expanded
        }
        _ => home.join(DEFAULT_DIR),
    }
}

fn read_pref(key: &str) -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// True when macOS still shows the floating thumbnail after a capture. That
/// thumbnail is why a screenshot takes ~5 s to reach the clipboard: the file is
/// only written to disk once it fades. Off = the capture lands instantly.
pub fn instant_enabled() -> bool {
    // The key is absent by default, and absent means the thumbnail is ON.
    matches!(read_pref("show-thumbnail").as_deref(), Some("0"))
}

pub fn set_instant(on: bool) -> Result<(), String> {
    let value = if on { "false" } else { "true" };
    let status = std::process::Command::new("defaults")
        .args(["write", "com.apple.screencapture", "show-thumbnail", "-bool", value])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("defaults write failed".into());
    }
    // The screenshot UI reads the pref at launch, so it has to be restarted for
    // the change to take. It respawns on its own within a second.
    let _ = std::process::Command::new("killall").arg("screencaptureui").status();
    crate::debug_log::log(&format!("screenshot: instant mode = {}", on));
    Ok(())
}

/// Watches the screenshot folder. Blocks; call on its own thread.
pub fn watch<F: Fn()>(history: Arc<Mutex<History>>, skip_next: Arc<Mutex<bool>>, on_change: F) {
    use notify::{Event, EventKind, RecursiveMode, Watcher as _};

    let dir = screenshot_dir();
    crate::debug_log::log(&format!("screenshot: watching {}", dir.display()));

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            crate::debug_log::log(&format!("screenshot: watcher init failed: {}", e));
            return;
        }
    };
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        crate::debug_log::log(&format!("screenshot: cannot watch {}: {}", dir.display(), e));
        return;
    }

    // One capture arrives as a burst of events (create, then several modifies as
    // macOS writes and renames the file). Without this, each one re-reads and
    // re-encodes the same PNG; the history dedup swallows the copies, so the
    // waste was invisible in the UI and visible only in the log.
    let mut handled: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for event in rx {
        let event = match event {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            continue;
        }
        for path in event.paths {
            if !intake::is_png(&path) {
                continue;
            }
            if !handled.insert(path.clone()) {
                continue;
            }
            let bytes = match intake::read_when_complete(&path) {
                Some(b) => b,
                None => continue,
            };
            let added = intake::ingest(
                &path,
                bytes,
                source_app::screenshot_source(),
                &history,
                &skip_next,
            );
            if added {
                on_change();
            }
        }
    }
}
