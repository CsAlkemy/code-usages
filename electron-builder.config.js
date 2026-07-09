'use strict';
// Central build config (package.json "build" moved here so signing can be
// env-driven). Two modes, decided by the environment:
//
//   No CSC_LINK (local dev)  → unsigned build, ad-hoc signed by after-pack.js.
//   CSC_LINK set (CI/release) → Developer ID signing + hardened runtime, and
//   notarization when APPLE_TEAM_ID / APPLE_ID / APPLE_APP_SPECIFIC_PASSWORD
//   are present. See .github/workflows/release.yml for the secrets.
//
// GITHUB_REPOSITORY_OWNER is set automatically on GitHub Actions, so the
// publish target resolves to whichever account hosts the repo. The fallback
// only matters for local `--publish` runs, which we don't do.
const hasCert = !!process.env.CSC_LINK;
const owner = process.env.GITHUB_REPOSITORY_OWNER || 'CsAlkemy';

module.exports = {
  appId: 'com.tuliptech.claude-usage-bar',
  productName: 'Claude Usage Bar',
  afterPack: './build/after-pack.js',
  files: ['src/**/*', 'assets/**/*', 'package.json'],
  // No spaces in artifact names — keeps Homebrew/curl URLs sane.
  artifactName: '${name}-${version}-${arch}.${ext}',
  publish: { provider: 'github', owner, repo: 'code-usages' },
  mac: {
    // zip alongside dmg: electron-updater consumes the zip for auto-updates.
    target: [{ target: 'dmg' }, { target: 'zip' }],
    category: 'public.app-category.utilities',
    icon: 'assets/icon.icns',
    extendInfo: { LSUIElement: true },
    ...(hasCert
      ? {
          hardenedRuntime: true,
          gatekeeperAssess: false,
          entitlements: 'build/entitlements.mac.plist',
          entitlementsInherit: 'build/entitlements.mac.plist',
          notarize: process.env.APPLE_TEAM_ID ? { teamId: process.env.APPLE_TEAM_ID } : false,
        }
      : { identity: null }),
  },
  dmg: { title: 'Claude Usage Bar' },
};
