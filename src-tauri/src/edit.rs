//! Editing a picture from the history in the Mac's own markup tools.
//!
//! The clip's own file under `img/` is never the one that opens: the history
//! owns it, and an editor saving over it would rewrite a card that is still on
//! screen. A copy goes out instead, the editor opens the copy, and every save
//! there comes back as a *new* clip — so the original stays as the card below
//! the edited one, which is the whole point of editing from a clipboard history.
//!
//! Where the copy goes is not a matter of taste. Preview is sandboxed, and the
//! only folder it may write into without the user picking the file in a save
//! panel is Downloads (`com.apple.security.files.downloads.read-write`). Handing
//! it a file anywhere else — Application Support, where this used to live — opens
//! and draws fine and then swallows the save: no error, no prompt on close, and
//! the file on disk byte-for-byte what we wrote (2026-08-06). So the copies live
//! in a folder of ours inside Downloads, and are swept from there.

use crate::history::History;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Our folder inside Downloads. A folder rather than loose files in Downloads
/// itself: the watcher would otherwise wake on every download the user makes.
const EDIT_DIR: &str = "Iago";

/// Prefix on every copy we write. The folder is the user's, in the open, and
/// anything they drop in it is theirs — the sweep only ever touches ours.
const EDIT_PREFIX: &str = "iago-";

/// macOS ships the markup tools — arrows, shapes, text, redaction, crop — inside
/// Preview, so that is what a picture is handed to. Not "whatever opens PNGs":
/// that can be a viewer with nothing to draw with.
#[cfg(target_os = "macos")]
const EDITOR_APP: &str = "Preview";

/// A copy is useless once the editor is done with it — but not before, and there
/// is no way to know when that is. So they go by age instead: a day is long
/// enough that a second save always finds the file it is saving into.
const KEEP_SECS: u64 = 24 * 60 * 60;

/// The pictures already accounted for, oldest first.
///
/// Two things would otherwise come back as clips that are not edits. The copy we
/// hand the editor is byte-for-byte a clip the history already holds, and picking
/// it up would bump that card to the top and relabel it as an edit that never
/// happened. And one save arrives as a burst of filesystem events, each of which
/// would re-read and re-decode the same PNG.
///
/// Keyed by content rather than by path on purpose: the watcher reports the
/// folder's resolved path, which is not always the one we wrote to (`/var` is a
/// symlink to `/private/var`), and it is the bytes that decide anyway. A few
/// entries are enough — once a file has moved on, its older versions can never
/// arrive again.
pub type Known = Mutex<VecDeque<u64>>;

/// How many recent pictures stay remembered. Covers the copy handed out plus a
/// run of saves; beyond that there is nothing left to recognise.
const KNOWN_MAX: usize = 16;

/// `None` when the OS will not tell us where Downloads is — editing is off
/// rather than quietly writing somewhere the editor cannot save.
pub fn dir() -> Option<PathBuf> {
    dirs::download_dir().map(|d| d.join(EDIT_DIR))
}

/// Hand a copy of a clip to the editor. The id names the file, so editing the
/// same clip twice reuses the document the editor already has open instead of
/// littering the folder with near-identical copies.
pub fn open(id: u64, png: &[u8], known: &Known) -> Result<(), String> {
    let dir = dir().ok_or("no Downloads folder to put the picture in")?;
    crate::private::create_dir(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    let path = dir.join(format!("{}{}.png", EDIT_PREFIX, id));
    // Remembered before it is written, not after: the watcher is already running,
    // and the only thing keeping it from filing our own copy as an edit would be
    // the fraction of a second it spends waiting for the write to settle. Saying
    // so first does not depend on that.
    remember(known, png);
    // The copy goes in locked, like everything else Iago writes. An editor saving
    // over it replaces the file rather than writing into it, so the saved version
    // comes back with the editor's own permissions — the 0700 folder above is
    // what keeps the picture private either way.
    crate::private::write(&path, png).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    crate::debug_log::log(&format!("edit: {} handed to the editor", path.display()));
    launch(&path)
}

#[cfg(target_os = "macos")]
fn launch(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("open")
        .args(["-a", EDITOR_APP])
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} would not open the picture", EDITOR_APP))
    }
}

/// Windows has no equivalent of Preview's markup, so the picture goes to
/// whatever the user has set for PNGs — Paint, by default, which draws.
#[cfg(not(target_os = "macos"))]
fn launch(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("no editor would open the picture".into())
    }
}

/// Watches the edit folder. Blocks; call on its own thread.
pub fn watch<F: Fn()>(
    dir: PathBuf,
    history: Arc<Mutex<History>>,
    skip_next: Arc<Mutex<bool>>,
    known: Arc<Known>,
    on_change: F,
) {
    use notify::{Event, EventKind, RecursiveMode, Watcher as _};

    if let Err(e) = crate::private::create_dir(&dir) {
        crate::debug_log::log(&format!("edit: cannot create {}: {}", dir.display(), e));
        return;
    }
    crate::debug_log::log(&format!("edit: watching {}", dir.display()));

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            crate::debug_log::log(&format!("edit: watcher init failed: {}", e));
            return;
        }
    };
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        crate::debug_log::log(&format!("edit: cannot watch {}: {}", dir.display(), e));
        return;
    }

    for event in rx {
        let event = match event {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            continue;
        }
        for path in event.paths {
            if !crate::intake::is_png(&path) || !ours(&path) {
                continue;
            }
            let bytes = match crate::intake::read_when_complete(&path) {
                Some(b) => b,
                None => continue,
            };
            // Unlike a screenshot, the same path legitimately comes back again
            // and again: every save is another version the user wants. What must
            // not come back is the version we already have.
            if is_known(&known, &bytes) {
                continue;
            }
            remember(&known, &bytes);
            let added = crate::intake::ingest(
                &path,
                bytes,
                crate::source_app::edited_source(),
                &history,
                &skip_next,
            );
            if added {
                on_change();
            }
        }
    }
}

/// Drops our copies older than a day. Called at startup: the folder only grows
/// while the app runs, and the app runs for weeks.
///
/// The folder sits in the user's Downloads, so anything in it that we did not
/// write is theirs and is never touched — hence the name check before the age
/// check, not after.
pub fn sweep(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // No folder yet is the normal first launch, not a failure.
        Err(_) => return,
    };
    let mut swept = 0;
    for entry in entries.flatten() {
        if !ours(&entry.path()) {
            continue;
        }
        // A file whose age we cannot read is a file we leave alone: deleting on
        // a guess is not something to do in a folder the user can see.
        let Ok(written) = entry.metadata().and_then(|m| m.modified()) else { continue };
        let Ok(age) = written.elapsed() else { continue };
        if age.as_secs() <= KEEP_SECS {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            swept += 1;
        }
    }
    if swept > 0 {
        crate::debug_log::log(&format!("edit: {} stale copies swept", swept));
    }
}

/// A file this app put in the folder, rather than something the user dropped
/// there — the folder is in plain sight, so both happen.
fn ours(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(EDIT_PREFIX) && n.to_lowercase().ends_with(".png"))
}

fn remember(known: &Known, bytes: &[u8]) {
    if let Ok(mut seen) = known.lock() {
        let h = hash(bytes);
        if seen.contains(&h) {
            return;
        }
        seen.push_back(h);
        while seen.len() > KNOWN_MAX {
            seen.pop_front();
        }
    }
}

fn is_known(known: &Known, bytes: &[u8]) -> bool {
    known.lock().map(|seen| seen.contains(&hash(bytes))).unwrap_or(false)
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copy_we_hand_the_editor_does_not_come_back_as_an_edit() {
        let known: Known = Mutex::new(VecDeque::new());
        let original = b"the clip as it already is in the history";

        remember(&known, original);
        assert!(is_known(&known, original), "our own copy must be ignored");

        let saved = b"the clip with an arrow drawn on it";
        assert!(!is_known(&known, saved), "a real save must get through");

        // And once it is in, the burst of events that follows one save must not
        // file it a second time.
        remember(&known, saved);
        assert!(is_known(&known, saved));
    }

    #[test]
    fn only_the_recent_pictures_stay_remembered() {
        let known: Known = Mutex::new(VecDeque::new());
        let first = b"the oldest picture".to_vec();
        remember(&known, &first);
        for i in 0..KNOWN_MAX {
            remember(&known, format!("save {}", i).as_bytes());
        }
        assert!(!is_known(&known, &first), "the list must not grow for as long as the app runs");
        assert_eq!(known.lock().unwrap().len(), KNOWN_MAX);
    }

    /// The whole loop with a real filesystem watcher behind it, minus the editor:
    /// the copy going out, a save coming back, and the second save that follows
    /// the first. Ignored by default because filing a clip puts it on the real
    /// clipboard, which is not something a test run should do to whoever ran it.
    ///
    ///     cargo test -- --ignored edited
    #[test]
    #[ignore = "writes to the real clipboard"]
    fn a_saved_picture_comes_back_as_its_own_clip_and_only_once() {
        let dir = std::env::temp_dir().join(format!("iago-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        crate::private::create_dir(&dir).unwrap();

        let history = Arc::new(Mutex::new(History::new()));
        let skip_next = Arc::new(Mutex::new(false));
        let known: Arc<Known> = Arc::new(Mutex::new(VecDeque::new()));

        let watched = dir.clone();
        let (h, s, k) = (Arc::clone(&history), Arc::clone(&skip_next), Arc::clone(&known));
        std::thread::spawn(move || watch(watched, h, s, k, || {}));
        std::thread::sleep(std::time::Duration::from_millis(300));

        // A picture the user happened to put in the folder is not an edit: the
        // folder lives in their Downloads, in plain sight.
        let theirs = dir.join("holiday.png");
        crate::private::write(&theirs, &png(4, 4, [10, 10, 10])).unwrap();

        let path = dir.join(format!("{}42.png", EDIT_PREFIX));
        let original = png(4, 4, [200, 0, 0]);
        remember(&known, &original);
        crate::private::write(&path, &original).unwrap();
        // Proving a clip does *not* appear means giving it every chance to: a
        // wait that returns the moment the count matches would pass before the
        // watcher had even read the file.
        std::thread::sleep(std::time::Duration::from_millis(700));
        assert_eq!(
            count(&history), 0,
            "neither the copy we handed the editor nor the user's own picture is an edit"
        );

        crate::private::write(&path, &png(4, 4, [0, 200, 0])).unwrap();
        assert_eq!(wait_for(&history, 1), 1, "a save has to come back as a clip");
        assert_eq!(
            history.lock().unwrap().items()[0].app.name,
            "Edited",
            "and it has to say where it came from"
        );

        crate::private::write(&path, &png(4, 4, [0, 0, 200])).unwrap();
        assert_eq!(wait_for(&history, 2), 2, "the next save is the next clip, not a replacement");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(test)]
    fn count(history: &Mutex<History>) -> usize {
        history.lock().unwrap().items().len()
    }

    /// Waits up to two seconds for the history to hold exactly `want` clips — the
    /// watcher runs on its own thread, and a fixed sleep is a flaky test.
    #[cfg(test)]
    fn wait_for(history: &Mutex<History>, want: usize) -> usize {
        for _ in 0..80 {
            if count(history) == want {
                return want;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        count(history)
    }

    #[cfg(test)]
    fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
}
