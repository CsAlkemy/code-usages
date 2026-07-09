# Code Usages

A tiny macOS menu-bar app that shows your **claude.ai plan usage** as a live
circular progress ring — tracking your **current session** limit, with the
weekly limits one click away in a native glass popover.

Built with **Tauri** (Rust + the system WebView), so the whole download is
**~3 MB**. It works by keeping a hidden, logged-in claude.ai webview in the
background and reading your own usage from your own session. No separate
account, no API key, and nothing ever leaves your machine.

> **Unofficial.** Not affiliated with Anthropic. It reads an undocumented
> endpoint, so a claude.ai change can break it until the app is updated.

## Install

**Recommended — one line in Terminal** (no security dialogs; terminal
downloads carry no quarantine flag):

```bash
curl -fsSL https://raw.githubusercontent.com/csalkemy/code-usages/main/install.sh | sh
```

**Or download manually** from [Releases](../../releases/latest) (`-arm64` for
Apple Silicon, `-x64` for Intel) and drag the app to Applications. Because
releases aren't notarized (needs an Apple Developer ID), browser downloads
hit *"Apple could not verify … is free of malware"* on first open. One-time
fix: **System Settings → Privacy & Security → Open Anyway**, or
`xattr -cr "/Applications/Code Usages.app"`.

**First launch:** a claude.ai window opens — sign in as normal. Seconds after
login the ring appears and the window hides itself. Your session persists
across restarts, so this is a one-time thing.

## Using it

- **Click** the ring → detail popover: current session + weekly limits, reset
  countdowns, plan badge.
- **Right-click** → Refresh, open the Claude window, Quit.
- The menu-bar number and ring track your **current session** (the limit that
  actually interrupts you). If no session limit is present, it falls back to
  whichever limit is closest.
- Not signed in? The app says so — it never shows made-up numbers.

## How it works

1. A hidden WebView holds your logged-in claude.ai session (cookies persist
   in the app's own data store).
2. Every ~4 minutes the Rust core reads that session's cookies and asks
   `/api/organizations/{your-org}/usage` directly, normalizing the `limits`
   array into session/weekly rows (`src-tauri/src/usage.rs`).
3. The tray ring is rasterized in Rust (`src-tauri/src/ring.rs`); the popover
   is plain HTML/CSS/JS (`ui/`) over Tauri IPC.

Polling is deliberately gentle — please keep it that way (`POLL_SECS` in
`src-tauri/src/lib.rs`).

## Privacy

Your credentials and usage data stay in the app's local container. There is
no telemetry, no server, no analytics. The only network traffic is your own
session talking to claude.ai.

## Develop

Requires Rust (`rustup`) and Node (for the Tauri CLI only).

```bash
npm install
npm run dev      # run with hot console output
npm run build    # release DMG into src-tauri/target/release/bundle/dmg/
```

The previous Electron implementation (v0.3.0 and earlier) was removed after
the Tauri port was verified; it lives on in git history and the v0.3.0 tag.

## Release

Releases are automated (`.github/workflows/release.yml`): bump the version in
`package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` (keep
them in sync), then:

```bash
git tag v0.4.0
git push origin main v0.4.0   # CI builds arm64 + x64 DMGs → GitHub Release
```

With no repo secrets configured, CI publishes unsigned builds. Add the Apple
secrets listed in the workflow file to ship signed + notarized builds.

## License

[MIT](LICENSE)
