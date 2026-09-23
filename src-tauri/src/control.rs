//! A way into the popup that does not depend on macOS handing out hotkeys.
//!
//! Hotkeys are dispatched by the window server, and it mutes the Option ones for
//! everybody while any process holds secure keyboard entry. On 2026-09-23 that
//! was Ghostty for an hour, then a system password agent that kept the hold
//! even after it was killed — no app can take ⌥V back from that. A remapper
//! working at the keyboard-driver level (Karabiner-Elements) sees the key
//! before the window server does, so it catches ⌥V and passes the press on
//! through this socket. README, "When ⌥V does nothing", has the rule.
//!
//! The socket sits in the app's own data folder with owner-only permissions:
//! only the user's own processes can reach it, and the one word it takes can
//! do nothing but toggle the popup.

use crate::debug_log;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;
use tauri::AppHandle;

const SOCKET: &str = "control.sock";

/// The only thing a client may say.
const TOGGLE: &str = "toggle";

/// Longer than any command we take; a client sending more is not ours.
const MAX_REQUEST: u64 = 64;

/// Binds the socket for the owner alone. A socket file left by the previous
/// run refuses a new bind, so it goes first.
fn bind(path: &Path) -> std::io::Result<UnixListener> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Listens for the life of the app. Failing to listen costs the driver-level
/// way in, not the app, so it is a log line rather than an error.
pub fn listen(app: AppHandle, data_dir: &Path) {
    let path = data_dir.join(SOCKET);
    let listener = match bind(&path) {
        Ok(l) => l,
        Err(e) => {
            debug_log::log(&format!("control: cannot listen on {}: {}", path.display(), e));
            return;
        }
    };
    debug_log::log(&format!("control: listening on {}", path.display()));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut request = String::new();
            if stream.take(MAX_REQUEST).read_to_string(&mut request).is_err() {
                continue;
            }
            if request.trim() != TOGGLE {
                debug_log::log(&format!("control: ignored {:?}", request));
                continue;
            }
            // The popup is AppKit: it has to be raised from the main thread.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || crate::toggle_popup(&handle, "karabiner"));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn the_socket_is_the_owners_alone_and_survives_a_stale_file() {
        let dir = std::env::temp_dir().join(format!("iago-control-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SOCKET);
        std::fs::write(&path, b"left over by a crashed run").unwrap();

        let _listener = bind(&path).expect("a stale file must not block the bind");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(UnixStream::connect(&path).is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
