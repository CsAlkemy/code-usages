'use strict';

const { app, Tray, Menu, BrowserWindow, nativeImage, nativeTheme, screen, ipcMain, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const config = require('./config');
const usage = require('./usage');
const settings = require('./settings');

const INJECT = fs.readFileSync(path.join(__dirname, 'inject-session.js'), 'utf8');

let tray = null;
let ringWin = null;      // hidden: draws the tray ring PNG
let sessionWin = null;   // hidden: holds the logged-in claude.ai session
let popoverWin = null;   // the detail panel
let ringReady = null;    // promise
let lastModel = usage.SIGNED_OUT;

// ---------------------------------------------------------------------------
// Ring rendering (offscreen)
// ---------------------------------------------------------------------------
function createRingWindow() {
  ringWin = new BrowserWindow({ show: false, width: 64, height: 64, webPreferences: {} });
  ringReady = new Promise((resolve) => ringWin.webContents.once('did-finish-load', resolve));
  ringWin.loadFile(path.join(__dirname, 'renderer', 'ring.html'));
}

function ringColor(pct) {
  const dark = nativeTheme.shouldUseDarkColors;
  const t = config.THRESHOLDS;
  const band = pct < t.healthy.max ? t.healthy : pct < t.warning.max ? t.warning : t.danger;
  return dark ? band.dark : band.light;
}

async function renderTrayImage(pct) {
  await ringReady;
  const color = ringColor(pct ?? 0);
  const track = 'rgba(140,140,140,0.5)';
  const dataUrl = await ringWin.webContents.executeJavaScript(
    `drawRing(${pct == null ? 0 : pct}, 22, ${JSON.stringify(color)}, ${JSON.stringify(track)})`,
    true
  );
  // Render big for anti-aliasing, then hand the tray a menu-bar-sized icon
  // (~16pt of glyph inside a 22pt box) so it matches the other status items.
  return nativeImage.createFromDataURL(dataUrl).resize({ width: 22, height: 22 });
}

async function updateTray(model) {
  if (!tray) return;
  const top = usage.primary(model);
  try {
    tray.setImage(await renderTrayImage(top));
  } catch (e) { /* keep previous icon */ }

  if (process.platform === 'darwin') {
    const showNum = settings.load().showNumber;
    tray.setTitle(showNum && top != null ? ` ${top}%` : '');
  }
  const parts = [];
  if (model.session) parts.push(`Session ${model.session.pct}%`);
  (model.weekly || []).forEach((w) => parts.push(`${w.label} ${w.pct}%`));
  tray.setToolTip(parts.length ? `Claude usage — ${parts.join(' · ')}` : 'Claude usage — not signed in');
}

// ---------------------------------------------------------------------------
// claude.ai session window (auth + data source)
// ---------------------------------------------------------------------------
function createSessionWindow() {
  sessionWin = new BrowserWindow({
    show: false,
    width: 1040,
    height: 820,
    title: 'Sign in to Claude',
    webPreferences: {
      partition: 'persist:claude',
      // Hook fetch/XHR at document-start via the preload. That needs the
      // preload to share the page world, hence contextIsolation:false +
      // sandbox:false. nodeIntegration stays off so claude.ai gets no Node.
      preload: path.join(__dirname, 'inject-preload.js'),
      contextIsolation: false,
      sandbox: false,
      nodeIntegration: false,
    },
  });

  const inject = () => sessionWin.webContents.executeJavaScript(INJECT, true).catch(() => {});
  sessionWin.webContents.on('did-finish-load', inject);
  sessionWin.webContents.on('did-navigate', inject);
  // While there's no real data yet, every navigation in this window is likely a
  // step of the login flow — poll right after each one so usage appears seconds
  // after sign-in instead of waiting for the next scheduled tick.
  const pollSoon = () => { if (lastModel.signedOut) setTimeout(() => poll(), 1200); };
  sessionWin.webContents.on('did-finish-load', pollSoon);
  sessionWin.webContents.on('did-navigate', pollSoon);
  sessionWin.webContents.on('did-navigate-in-page', pollSoon);
  // Keep it as a background window: hide instead of closing.
  sessionWin.on('close', (e) => { e.preventDefault(); sessionWin.hide(); });

  if (config.USER_AGENT) sessionWin.webContents.setUserAgent(config.USER_AGENT);
  const url = config.CLAUDE_ORIGIN + (config.PRIME_PATH || '');
  sessionWin.loadURL(url, config.USER_AGENT ? { userAgent: config.USER_AGENT } : undefined);
}

function showSession() {
  if (!sessionWin || sessionWin.isDestroyed()) createSessionWindow();
  sessionWin.show();
  sessionWin.focus();
}

let learnedUsageUrl = ''; // discovered once (…/organizations/{uuid}/usage), then reused
let learnedPlan = '';     // raven_type, e.g. "team"

async function fetchUsage() {
  if (!sessionWin || sessionWin.isDestroyed()) return null;
  try {
    // Discover the org and read /usage directly from the logged-in session.
    // Cached across polls; discovery (/api/organizations) only runs until known.
    const knownUrl = config.USAGE_URL || learnedUsageUrl || '';
    const result = await sessionWin.webContents.executeJavaScript(`(async () => {
      const j = async (u) => { try { const r = await fetch(u, { credentials:'include', headers:{ accept:'application/json' } }); if (!r.ok) return null; return await r.json(); } catch (e) { return null; } };
      let usageUrl = ${JSON.stringify(knownUrl)};
      let plan = ${JSON.stringify(learnedPlan)};
      if (!usageUrl) {
        const orgs = await j('/api/organizations');
        const org = Array.isArray(orgs)
          ? (orgs.find((o) => o && Array.isArray(o.capabilities) && o.capabilities.includes('chat')) || orgs[0])
          : null;
        if (org && org.uuid) { usageUrl = '/api/organizations/' + org.uuid + '/usage'; plan = org.raven_type || plan; }
      }
      if (!usageUrl) return null;
      const raw = await j(usageUrl);
      if (!raw) return null;
      if (plan) raw.__plan = plan;
      return { url: usageUrl, plan: plan, raw: raw };
    })()`, true).catch(() => null);

    let raw = result && result.raw ? result.raw : null;
    let url = (result && result.url) || knownUrl || null;
    if (result && result.url) { learnedUsageUrl = result.url; if (result.plan) learnedPlan = result.plan; }

    // Fallback: the inject-hook capture (in case the API shape/paths change).
    if (!raw || !usage.normalize(raw)) {
      const cap = await sessionWin.webContents
        .executeJavaScript('window.__CLAUDE_USAGE_CAPTURE__ || null', true).catch(() => null);
      if (cap && cap.raw && usage.normalize(cap.raw)) { raw = cap.raw; url = cap.url; }
    }

    if (raw) {
      console.log('[usage] payload snapshot:', JSON.stringify(raw).slice(0, 2000));
      saveSnapshot(raw, url);
    }
    return raw;
  } catch (e) {
    return null;
  }
}

function snapshotPath() { return path.join(app.getPath('userData'), 'last-usage.json'); }
function saveSnapshot(raw, url) {
  try {
    fs.writeFileSync(snapshotPath(),
      JSON.stringify({ url: url || null, capturedAt: new Date().toISOString(), raw }, null, 2));
  } catch { /* best effort */ }
}

async function poll() {
  const raw = await fetchUsage();
  let model = raw ? usage.normalize(raw) : null;
  if (!model) {
    model = usage.SIGNED_OUT;
  } else if (lastModel.signedOut && sessionWin && !sessionWin.isDestroyed() && sessionWin.isVisible()) {
    sessionWin.hide(); // sign-in just completed — tuck the window away
  }
  lastModel = model;
  await updateTray(model);
  if (popoverWin && !popoverWin.isDestroyed()) popoverWin.webContents.send('usage', model);
}

// ---------------------------------------------------------------------------
// Popover (detail panel)
// ---------------------------------------------------------------------------
function createPopover() {
  const mac = process.platform === 'darwin';
  popoverWin = new BrowserWindow({
    width: config.POPOVER.width,
    height: config.POPOVER.height,
    show: false,
    frame: false,
    // macOS: native glass (vibrancy) + system rounded corners, like a real
    // menu-bar popover. Elsewhere: transparent window, card drawn in CSS.
    transparent: !mac,
    ...(mac ? {
      vibrancy: 'popover',
      visualEffectState: 'active',
      backgroundColor: '#00000000',
      roundedCorners: true,
    } : {}),
    resizable: false,
    movable: false,
    skipTaskbar: true,
    alwaysOnTop: true,
    fullscreenable: false,
    webPreferences: {
      preload: path.join(__dirname, 'popover', 'popover-preload.js'),
      contextIsolation: true,
    },
  });
  popoverWin.loadFile(path.join(__dirname, 'popover', 'index.html'));
  popoverWin.webContents.on('did-finish-load', () => {
    popoverWin.webContents.send('settings', settings.getAll());
    popoverWin.webContents.send('usage', lastModel);
  });
  popoverWin.on('blur', () => popoverWin.hide());
}

function sendSettings() {
  if (popoverWin && !popoverWin.isDestroyed()) {
    popoverWin.webContents.send('settings', settings.getAll());
  }
}

function positionPopover() {
  const b = tray.getBounds();
  const cur = popoverWin.getBounds();
  const width = cur.width || config.POPOVER.width;
  const height = cur.height || config.POPOVER.height;
  const disp = screen.getDisplayMatching(b);
  const wa = disp.workArea;
  let x, y;

  if (process.platform === 'darwin') {
    x = Math.round(b.x + b.width / 2 - width / 2);
    y = Math.round(b.y + b.height + 2);
  } else {
    // Windows / Linux tray sits bottom-right; anchor there.
    x = wa.x + wa.width - width - 8;
    y = wa.y + wa.height - height - 8;
    if (b.y < wa.y + wa.height / 2) y = b.y + b.height + 4; // tray on top
  }
  x = Math.max(wa.x + 4, Math.min(x, wa.x + wa.width - width - 4));
  y = Math.max(wa.y + 4, Math.min(y, wa.y + wa.height - height - 4));
  popoverWin.setBounds({ x, y, width, height });
}

function togglePopover() {
  if (!popoverWin || popoverWin.isDestroyed()) createPopover();
  if (popoverWin.isVisible()) {
    popoverWin.hide();
    return;
  }
  positionPopover();
  popoverWin.webContents.send('settings', settings.getAll());
  popoverWin.webContents.send('usage', lastModel);
  popoverWin.webContents.send('reset-view'); // always reopen on the main view
  popoverWin.show();
  popoverWin.focus();
  poll(); // refresh in the background while it's open
}

// ---------------------------------------------------------------------------
// Tray + menu
// ---------------------------------------------------------------------------
function setTheme(theme) {
  settings.save({ theme });
  nativeTheme.themeSource = theme; // 'system' | 'light' | 'dark'
  updateTray(lastModel);
  sendSettings();
}

function buildMenu() {
  const s = settings.load();
  return Menu.buildFromTemplate([
    { label: 'Refresh now', click: () => poll() },
    { label: 'Open Claude (capture usage)…', click: () => showSession() },
    { label: 'Reveal captured usage (JSON)…', click: () => { try { shell.showItemInFolder(snapshotPath()); } catch { /* none yet */ } } },
    { type: 'separator' },
    {
      label: 'Show percentage',
      type: 'checkbox',
      checked: s.showNumber,
      click: (item) => { settings.save({ showNumber: item.checked }); updateTray(lastModel); sendSettings(); },
    },
    {
      label: 'Appearance',
      submenu: [
        { label: 'System', type: 'radio', checked: (s.theme || 'system') === 'system', click: () => setTheme('system') },
        { label: 'Light', type: 'radio', checked: s.theme === 'light', click: () => setTheme('light') },
        { label: 'Dark', type: 'radio', checked: s.theme === 'dark', click: () => setTheme('dark') },
      ],
    },
    {
      label: 'Open at login',
      type: 'checkbox',
      checked: app.getLoginItemSettings().openAtLogin,
      click: (item) => app.setLoginItemSettings({ openAtLogin: item.checked }),
    },
    { type: 'separator' },
    {
      label: 'Quit',
      click: () => {
        // Make sure the claude.ai cookies hit disk so the login survives.
        try { sessionWin.webContents.session.flushStorageData(); } catch { /* best effort */ }
        app.exit(0);
      },
    },
  ]);
}

function createTray() {
  const placeholder = nativeImage.createFromPath(
    path.join(__dirname, '..', 'assets', 'tray-placeholder.png')
  );
  tray = new Tray(placeholder);

  if (process.platform === 'linux') {
    // Left-click events are unreliable on many Linux trays; use the menu.
    tray.setContextMenu(buildMenu());
  } else {
    tray.on('click', togglePopover);
    // Rebuild each time so the checkboxes/radios reflect the current settings.
    tray.on('right-click', () => tray.popUpContextMenu(buildMenu()));
  }
}

// ---------------------------------------------------------------------------
// IPC from popover
// ---------------------------------------------------------------------------
ipcMain.handle('refresh', async () => { await poll(); return true; });
ipcMain.handle('open-claude', () => { showSession(); return true; });
ipcMain.on('resize-popover', (_e, h) => {
  if (!popoverWin || popoverWin.isDestroyed()) return;
  const width = config.POPOVER.width;
  const height = Math.max(160, Math.min(600, Math.round(h || config.POPOVER.height)));
  popoverWin.setBounds({ ...popoverWin.getBounds(), width, height });
  if (popoverWin.isVisible()) positionPopover(); // keep it anchored to the tray
});
ipcMain.handle('get-settings', () => settings.getAll());
ipcMain.handle('set-settings', (_e, patch) => {
  patch = patch || {};
  if (typeof patch.openAtLogin === 'boolean') {
    try { app.setLoginItemSettings({ openAtLogin: patch.openAtLogin }); } catch { /* unsupported */ }
  }
  const rest = { ...patch };
  delete rest.openAtLogin;
  if (Object.keys(rest).length) settings.save(rest);
  if (typeof patch.theme === 'string') nativeTheme.themeSource = patch.theme;
  updateTray(lastModel);
  sendSettings();
  return settings.getAll();
});

// ---------------------------------------------------------------------------
// Auto-update (GitHub Releases via electron-updater)
// ---------------------------------------------------------------------------
// Silent by design: macOS only applies updates to signed builds, so on ad-hoc
// dev/unsigned builds every check fails — swallow it and carry on.
function setupAutoUpdater() {
  if (!app.isPackaged) return;
  let autoUpdater;
  try { ({ autoUpdater } = require('electron-updater')); } catch { return; }
  autoUpdater.on('error', () => { /* unsigned build or no releases yet */ });
  const check = () => autoUpdater.checkForUpdatesAndNotify().catch(() => {});
  setTimeout(check, 30 * 1000); // let startup settle first
  setInterval(check, 12 * 60 * 60 * 1000);
}

// ---------------------------------------------------------------------------
// App lifecycle
// ---------------------------------------------------------------------------
if (!app.requestSingleInstanceLock()) {
  app.exit(0);
} else {
  app.whenReady().then(async () => {
    if (process.platform === 'darwin' && app.dock) app.dock.hide();

    // Look like Chrome, not Electron, so Cloudflare's Turnstile challenge on
    // claude.ai passes instead of looping. Set globally (covers challenge
    // sub-requests/iframes) before any window loads a URL.
    if (config.USER_AGENT) app.userAgentFallback = config.USER_AGENT;

    // Apply the saved appearance preference before anything renders.
    nativeTheme.themeSource = settings.load().theme || 'system';
    // Keep the tray ring colour + popover in sync when the OS theme flips.
    nativeTheme.on('updated', () => { updateTray(lastModel); sendSettings(); });

    createRingWindow();
    createSessionWindow();
    createPopover();
    createTray();
    setupAutoUpdater();

    await updateTray(lastModel); // draws mock/placeholder immediately

    // Give the session window a moment to load, then do the first poll.
    setTimeout(async () => {
      await poll();
      // Returning users are already logged in (cookies persist). Only surface
      // the sign-in window if we still have no real data after the first try.
      if (config.SHOW_LOGIN_ON_START && lastModel.signedOut) showSession();
    }, 2500);

    setInterval(poll, config.POLL_MS);
  });

  app.on('window-all-closed', () => { /* stay alive in the tray */ });
}
