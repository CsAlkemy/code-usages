# Code Usages

A tiny macOS menu-bar app (Windows/Linux tray supported too) that shows your
**claude.ai plan usage** as a live circular progress ring — tracking your
**current session** limit, with the weekly limits one click away in a native
glass popover.

It works by keeping a hidden, logged-in claude.ai window in the background and
reading your own usage from your own session. No separate account, no API key,
and nothing ever leaves your machine.

> **Unofficial.** Not affiliated with Anthropic. It reads an undocumented
> endpoint, so a claude.ai change can break it until the app is updated.

## Install

**Download** the latest DMG from [Releases](../../releases/latest), open it,
and drag the app to Applications.

Until releases are notarized with an Apple Developer ID, macOS will warn on
first open: **right-click the app → Open → Open** (once). If it says the app
"is damaged", clear the quarantine flag instead:

```bash
xattr -cr "/Applications/Code Usages.app"
```

Or via Homebrew (once the tap is set up — see `packaging/homebrew/`):

```bash
brew install CsAlkemy/tap/code-usages
```

**First launch:** a claude.ai window opens — sign in as normal. Seconds after
login the ring appears and the window hides itself. Your session persists
across restarts, so this is a one-time thing.

## Using it

- **Click** the ring → detail popover: current session + weekly limits, reset
  countdowns, plan badge.
- **Right-click** → Refresh, open the Claude window, appearance, "Open at
  login", Quit.
- The menu-bar number and ring track your **current session** (the limit that
  actually interrupts you). If no session limit is present, it falls back to
  whichever limit is closest.
- Not signed in? The app says so — it never shows made-up numbers.

## How it works

1. A hidden `BrowserWindow` holds your logged-in claude.ai session
   (cookies persist in a `persist:claude` partition).
2. Every ~4 minutes the app asks that session for
   `/api/organizations/{your-org}/usage` and normalizes the `limits` array
   into session/weekly rows (`src/usage.js`).
3. A network-sniffing fallback (`src/inject-session.js`) watches the page's
   own requests, so if the endpoint path moves, the app re-learns it.

Polling is deliberately gentle — please keep it that way (`POLL_MS` in
`src/config.js`).

## Privacy

Your credentials and usage data stay in the app's local profile
(`~/Library/Application Support/Code Usages`). There is no telemetry, no
server, no analytics. The only network traffic is your own browser session
talking to claude.ai.

## Develop

```bash
npm install
npm start          # run from source
npm run dist       # build an unsigned DMG into dist/
```

Everything tweakable lives in `src/config.js` (poll interval, thresholds,
popover size). UI is `src/popover/`; data normalization is `src/usage.js`.

## Release

Releases are automated (`.github/workflows/release.yml`):

```bash
npm version minor          # bumps package.json, creates the git tag
git push --follow-tags     # CI builds universal DMG+zip → GitHub Release
```

With no repo secrets configured, CI publishes an unsigned build. Add
`CSC_LINK`, `CSC_KEY_PASSWORD`, `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`,
and `APPLE_TEAM_ID` (see the workflow file) to ship signed + notarized builds;
auto-update via `electron-updater` then works out of the box.

## License

[MIT](LICENSE)
