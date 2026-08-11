# Development

Everything a user does not need to know. The [README](../README.md) is the front door.

## Stack

[Tauri 2](https://tauri.app/) — Rust on the outside, plain HTML/CSS/JS on the inside, no bundler, one codebase for macOS and Windows. The same rails as [Ribbit](https://github.com/olegperegudov/ribbit) and [Quill](https://github.com/olegperegudov/quill): a shared build, signing and update pipeline across all three.

The popup is a non-activating `NSPanel` — it takes the keyboard but never takes focus from the app underneath, so a paste lands where you were working. The frontmost pid is remembered when the popup opens and the app is raised again before the keystroke goes out.

## Run it locally

```bash
npm install
npm run tauri dev
```

## Tests

```bash
npm test                           # frontend: search, filtering, highlighting, escaping
cd src-tauri && cargo test --lib   # backend: history, store, paste
```

Both run in CI before anything is built.

## Release

Every push to `main` is a release. CI bumps the patch version itself, tags it, builds Windows and both macOS architectures, and publishes the GitHub release plus the `latest.json` the in-app updater reads. Never bump by hand.

The release is published as a **prerelease** and reaches nobody until it is promoted. Between the two, CI installs the build on real runners and demands it start: on Windows the NSIS installer runs silently, the installed `ProductVersion` must equal the tag and the process must still be alive 10s after launch; on macOS `codesign --verify --deep --strict`, the `Info.plist` version and the same 10-second launch (the Intel launch is skipped with a notice when the runner has no Rosetta). A tray app has no window to assert on — a living process is the smoke signal. Then `latest.json` is verified (all three platforms, version matches the tag, no universal macOS bundle) and only then does the `promote` job mark the release latest, which is what the updater follows.

Any red gate leaves the build a prerelease and stable users keep the previous version. To move the channel by hand — after a bad release, or to ship a build whose canary flaked — use **Actions → Release control**: `promote` or `rollback` with the tag.

Each release also carries version-less copies of the installers (`Iago_macOS_AppleSilicon.dmg`, `Iago_macOS_Intel.dmg`, `Iago_Windows_Setup.exe`) so the README buttons can link straight at a file that survives the next bump.

## Signing

macOS builds are signed with a stable self-signed certificate ("Iago Code Signing"), not ad-hoc. macOS binds the Accessibility grant to the *signature*, so the user grants it once, at install, and updates never re-ask — an ad-hoc signature changes with every build and would.

Not notarized (that needs a paid Apple account), so the first open still needs `xattr -cr`.

## Synthetic keystrokes

⌘V is posted as a raw `CGEvent` on the **physical** V key (`kVK_ANSI_V` = 9) with the Command flag set on the event. Never address the key by its letter: a lookup through the active layout finds no "v" on a Cyrillic layout, falls through to keycode 0 — which is the A key — and the paste silently goes out as ⌘A. That shipped once; see the 0.1.15 entry in the [changelog](../CHANGELOG.md).

## Debugging

DevTools are off in release builds — `console.log` is invisible. The frontend logs through the `js_log` command instead; it lands in the session log next to the Rust side's own lines, so a UI event and the backend's reaction sit in one timeline.

```js
invoke("js_log", { message: `paste: ${id}` });
```

Remove the probes in the same change that fixes the bug.

## Where things live

| | |
|---|---|
| History, images, settings | the OS app-data dir (`app_data_dir()`) — `index.json`, `img/<id>.png`, `settings.json` |
| Session log | `~/Library/Application Support/iago/debug.log` |
