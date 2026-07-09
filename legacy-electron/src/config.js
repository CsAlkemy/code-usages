'use strict';

// ---------------------------------------------------------------------------
// Everything you might want to tweak lives here.
// ---------------------------------------------------------------------------

module.exports = {
  // The site whose session we piggyback on. The app opens this in a hidden
  // window that holds your normal logged-in cookies.
  CLAUDE_ORIGIN: 'https://claude.ai',

  // Present a normal Chrome UA instead of Electron's default. Cloudflare's
  // "Verify you are human" (Turnstile) challenge loops forever when it sees an
  // "Electron/…" user-agent; a matching Chrome UA lets it pass. Keep the Chrome
  // major version aligned with the bundled Chromium (Electron 31 → Chromium 126).
  USER_AGENT: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36',

  // How often to refresh usage, in milliseconds. Be a good citizen — this hits
  // an undocumented endpoint, so don't hammer it. 3–5 minutes is plenty.
  POLL_MS: 4 * 60 * 1000,

  // ---- Data source -----------------------------------------------------------
  // By default the app auto-discovers your org from /api/organizations and polls
  // /api/organizations/{uuid}/usage directly (main.js → fetchUsage). No manual
  // step: as soon as the hidden window is logged in, it just works. The plan
  // label comes from the org's `raven_type` (e.g. "team" → "Team").
  //
  // Override only if the API path changes: pin the full usage URL here to skip
  // discovery. Leave '' for auto-discovery. As a further fallback, inject-session.js
  // still learns the endpoint by watching the network.
  USAGE_URL: '',

  // Optional legacy trigger: a page to navigate the hidden window to so the
  // network-watch fallback can learn the endpoint. Not needed with auto-discovery.
  PRIME_PATH: '',

  // Colour thresholds for the ring, by "closest to limit" percentage.
  THRESHOLDS: {
    healthy: { max: 60, light: '#0F9D6C', dark: '#33C89A' }, // green/teal
    warning: { max: 85, light: '#C9820F', dark: '#E6A93B' }, // amber
    danger: { max: 101, light: '#D64545', dark: '#F06B6B' }, // red
  },

  POPOVER: { width: 340, height: 384 },

  // Show the hidden claude.ai window on first launch so you can sign in.
  // It auto-hides as soon as usage data is captured.
  SHOW_LOGIN_ON_START: true,
};
