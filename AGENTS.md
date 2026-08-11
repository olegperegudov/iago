# Agent notes

Iago is a clipboard-history app living in the menu bar. Tauri 2: Rust owns the
history and the pasting, the webview only draws cards. No frontend framework,
no bundler — `src/` is served as-is.

Start here: **[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)** — stack, window
behaviour, storage, signing, CI.

Quick facts:

- Run it: `npm install && npm run tauri dev`
- Tests: `npm test` (vitest: search, filtering, highlighting, escaping) and
  `cargo test --lib` in `src-tauri/` (history, store, paste)
- Clips are stored locally with `0600` permissions and expire on a schedule the
  user sets; clips marked concealed by a password manager are never recorded.
- Versions are bumped by CI on push to `main` — do not edit the version in
  `src-tauri/tauri.conf.json` or `Cargo.toml` by hand.
- User-visible changes go in `CHANGELOG.md` under `## Unreleased`, one plain
  bullet per change; CI cuts that section into the release notes.
