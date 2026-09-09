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
    let loc = read_pref("location");
    let expanded = expand(loc.clone(), &home);
    let resolved = resolve_dir(loc, &home, |p| p.is_dir());
    if resolved != expanded {
        crate::debug_log::log(&format!(
            "screenshot: configured location {} does not exist, falling back to {}",
            expanded.display(),
            resolved.display()
        ));
    }
    resolved
}

fn expand(loc: Option<String>, home: &std::path::Path) -> PathBuf {
    match loc {
        Some(loc) if !loc.is_empty() => match loc.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => PathBuf::from(loc),
        },
        _ => home.join(DEFAULT_DIR),
    }
}

/// macOS itself falls back to the Desktop when the configured location has
/// gone missing (moved, deleted, an external drive that isn't mounted) — it
/// never fails to save a screenshot over a stale pref. We mirror that: a
/// configured folder that doesn't exist on disk is as good as unset.
fn resolve_dir(loc: Option<String>, home: &std::path::Path, is_dir: impl Fn(&std::path::Path) -> bool) -> PathBuf {
    let expanded = expand(loc, home);
    if is_dir(&expanded) {
        expanded
    } else {
        home.join(DEFAULT_DIR)
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

#[cfg(test)]
mod tests {
    use super::resolve_dir;
    use std::path::PathBuf;

    /// A stale `com.apple.screencapture location` pointing at a folder that no
    /// longer exists must not leave the watcher pointed at nothing — macOS
    /// itself saves to the Desktop in that case, and so must we.
    #[test]
    fn missing_configured_dir_falls_back_to_desktop() {
        let home = PathBuf::from("/Users/oleg");
        let resolved = resolve_dir(Some("/Users/oleg/Documents/screenshots".into()), &home, |_| false);
        assert_eq!(resolved, home.join("Desktop"));
    }

    #[test]
    fn existing_configured_dir_is_used_as_is() {
        let home = PathBuf::from("/Users/oleg");
        let configured = home.join("Pictures/Screenshots");
        let resolved = resolve_dir(Some("~/Pictures/Screenshots".into()), &home, |p| p == configured);
        assert_eq!(resolved, configured);
    }

    #[test]
    fn unset_pref_defaults_to_desktop() {
        let home = PathBuf::from("/Users/oleg");
        let resolved = resolve_dir(None, &home, |_| true);
        assert_eq!(resolved, home.join("Desktop"));
    }
}
