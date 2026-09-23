//! Who is holding secure keyboard entry, and what the tray says about it.
//!
//! Any app can switch on macOS's secure event input — a terminal does it at a
//! password prompt, a browser inside a password field. While it is on, the
//! system swallows every hotkey made of Option alone, ⌥V included, and tells
//! nobody: the app that registered it just looks dead. An app that forgets to
//! let go keeps it that way for hours (Ghostty, 2026-09-23). ⌃⌥V still gets
//! through (see `HOTKEYS` in lib.rs), and the tray names the holder, so the
//! user knows both the way around and whom to blame.
//!
//! The holder's pid is published in the login session's dictionary — the same
//! value `ioreg -l | grep SecureInput` shows — and is absent while nobody
//! holds it. Windows has no such mode.

use crate::debug_log;
use tauri::AppHandle;

/// How often the holder is looked up. A dictionary read, nothing more — cheap
/// enough to keep the tray a couple of seconds behind reality at worst.
const POLL_SECS: u64 = 2;

/// Tray tooltip while nothing gets in the way of ⌥V.
pub const TOOLTIP: &str = "Iago — clipboard history (⌥V)";

/// The tooltip and the top line of the tray menu while `holder` blocks ⌥V.
pub fn blocked_hint(holder: &str) -> String {
    format!("{} holds secure input: ⌥V is blocked, ⌃⌥V still works", holder)
}

/// Watches the holder for the life of the app and hands every change to the
/// tray. A change is also a log line: when ⌥V "stops working" the log says who
/// took it and when.
#[cfg(target_os = "macos")]
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            let now = holder();
            if now != last {
                match &now {
                    Some(name) => debug_log::log(&format!("secure input: held by {}", name)),
                    None => debug_log::log("secure input: released"),
                }
                crate::tray::show_secure_input(&app, now.as_deref());
                last = now;
            }
            std::thread::sleep(std::time::Duration::from_secs(POLL_SECS));
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn watch(_app: AppHandle) {}

/// The name of the app holding secure input, if anyone does.
#[cfg(target_os = "macos")]
fn holder() -> Option<String> {
    use cocoa::base::{id, nil};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }

    let pid = unsafe {
        let raw = CGSessionCopyCurrentDictionary();
        if raw.is_null() {
            return None;
        }
        let session: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(raw);
        let value = session.find(CFString::from_static_string("kCGSSessionSecureInputPID"))?;
        value.downcast::<CFNumber>()?.to_i32()?
    };
    let name = unsafe {
        let running: id = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if running == nil {
            String::new()
        } else {
            crate::source_app::nsstring_to_string(msg_send![running, localizedName])
        }
    };
    // A holder with no app behind it (a helper process) is still a holder.
    Some(if name.is_empty() { format!("process {}", pid) } else { name })
}


