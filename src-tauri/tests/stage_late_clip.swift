// Copies the way WhatsApp does: empties the clipboard, then fills it a moment
// later. macOS moves the change counter on the emptying only, so a watcher that
// reads during the gap sees nothing and — unless it re-reads — never sees the
// text either. Used to check by hand that the clip still reaches the history.
//
//   swift src-tauri/tests/stage_late_clip.swift
//
// then look for "iago-late-clip-…" in the popup and in debug.log.

import AppKit

let pb = NSPasteboard.general
pb.clearContents()
Thread.sleep(forTimeInterval: 0.7)
let text = "iago-late-clip-\(Int(Date().timeIntervalSince1970))"
pb.setString(text, forType: .string)
print("staged: \(text)")
