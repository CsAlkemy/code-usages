'use strict';

// ---------------------------------------------------------------------------
// Tauri IPC shim: same surface the Electron preload exposed, so the rest of
// this file is identical to the Electron popover.
// ---------------------------------------------------------------------------
const TAURI = window.__TAURI__;
window.usageAPI = {
  refresh: () => TAURI.core.invoke('refresh'),
  openClaude: () => TAURI.core.invoke('open_claude'),
  getSettings: () => TAURI.core.invoke('get_settings'),
  setSettings: (patch) => TAURI.core.invoke('set_settings', { patch }),
  getUsage: () => TAURI.core.invoke('get_usage'),
  resize: (h) => TAURI.core.invoke('resize_popover', { height: h }),
  onUsage: (cb) => { TAURI.event.listen('usage', (e) => cb(e.payload)); },
  onSettings: (cb) => { TAURI.event.listen('settings', (e) => cb(e.payload)); },
  onResetView: (cb) => { TAURI.event.listen('reset-view', () => cb()); },
};

const THRESH = [
  { max: 60, light: '#0F9D6C', dark: '#33C89A' },
  { max: 85, light: '#C9820F', dark: '#E6A93B' },
  { max: 101, light: '#D64545', dark: '#F06B6B' },
];

let settings = { showNumber: false, theme: 'system', openAtLogin: false };
let lastData = null;

// macOS gets native vibrancy (applied on the Tauri window); flag it so the CSS
// drops the hand-drawn card chrome and tints the glass instead.
const GLASS = navigator.platform.toUpperCase().includes('MAC');
if (GLASS) document.documentElement.setAttribute('data-glass', '');

// Dark when explicitly chosen, otherwise follow the OS (theme === 'system').
function isDark() {
  const t = document.documentElement.getAttribute('data-theme');
  if (t === 'dark') return true;
  if (t === 'light') return false;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}
function colorFor(pct) {
  const t = THRESH.find((x) => (pct ?? 0) < x.max) || THRESH[THRESH.length - 1];
  return isDark() ? t.dark : t.light;
}

function drawRing(pct) {
  const cv = document.getElementById('ring');
  const S = cv.width; // 104 (2x of the 52px display size)
  const ctx = cv.getContext('2d');
  ctx.clearRect(0, 0, S, S);
  const lw = 11, r = (S - lw) / 2 - 2, cx = S / 2, cy = S / 2;

  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.strokeStyle = getComputedStyle(document.body).getPropertyValue('--track') || 'rgba(0,0,0,0.08)';
  ctx.lineWidth = lw;
  ctx.stroke();

  const p = Math.max(0, Math.min(100, pct || 0)) / 100;
  if (p > 0) {
    ctx.beginPath();
    ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + p * Math.PI * 2);
    ctx.strokeStyle = colorFor(pct);
    ctx.lineWidth = lw;
    ctx.lineCap = 'round';
    ctx.stroke();
  }
}

function rowHTML(item) {
  const c = colorFor(item.pct);
  return `<div class="row">
    <div class="rowhead"><span class="rowlabel">${esc(item.label)}</span><span class="rowpct">${item.pct}%</span></div>
    <div class="bar"><div class="fill" style="width:${item.pct}%;background:${c}"></div></div>
    ${item.reset ? `<div class="reset">${esc(item.reset)}</div>` : ''}
  </div>`;
}

function esc(s) {
  return String(s == null ? '' : s).replace(/[&<>"]/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

function closest(model) {
  const all = [];
  if (model.session && typeof model.session.pct === 'number') all.push(model.session.pct);
  (model.weekly || []).forEach((w) => typeof w.pct === 'number' && all.push(w.pct));
  return all.length ? Math.max(...all) : null;
}

function render(data) {
  if (!data) return;
  lastData = data;
  const sess = data.session && typeof data.session.pct === 'number' ? data.session.pct : null;
  const top = sess != null ? sess : closest(data); // session first, highest row as fallback

  // The popover ring is the primary readout — always show its number.
  document.getElementById('ringpct').textContent = top != null ? top + '%' : '';
  drawRing(top || 0);

  const planEl = document.getElementById('plan');
  if (data.plan) { planEl.textContent = data.plan; planEl.hidden = false; } else { planEl.hidden = true; }

  const labelEl = document.getElementById('herolabel');
  const nameEl = document.getElementById('heroname');
  if (data.signedOut) {
    labelEl.textContent = 'Current session';
    nameEl.textContent = 'Not signed in';
  } else if (sess != null) {
    labelEl.textContent = 'Current session';
    nameEl.textContent = data.session.reset || `${sess}% used`;
  } else if (top == null) {
    labelEl.textContent = 'Current session';
    nameEl.textContent = 'No data yet';
  } else {
    const which = (data.weekly || []).reduce((a, w) => (w.pct >= (a?.pct ?? -1) ? w : a), null);
    labelEl.textContent = 'Closest to limit';
    nameEl.textContent = which ? which.label : 'Usage';
  }

  const rows = [];
  if (data.session) rows.push(data.session);
  (data.weekly || []).forEach((w) => rows.push(w));
  document.getElementById('rows').innerHTML = data.signedOut
    ? '<div class="signedout">Sign in to claude.ai to see your usage.</div>'
    : rows.map(rowHTML).join('');

  const updated = document.getElementById('updated');
  const stamp = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  updated.className = data.signedOut ? 'mocknote' : 'updated';
  const via = data.source === 'claude-code' ? 'via Claude Code · ' : '';
  updated.textContent = data.signedOut ? 'not signed in' : `${via}updated ${stamp}`;

  document.getElementById('signin').textContent = data.signedOut ? 'Sign in' : 'Open Claude';

  fitWindow();
}

// --------------------------------------------------------------------------
// Settings
// --------------------------------------------------------------------------
function applySettings() {
  document.documentElement.setAttribute('data-theme', settings.theme || 'system');

  const numSw = document.getElementById('toggleNumber');
  numSw.setAttribute('aria-checked', String(!!settings.showNumber));
  const loginSw = document.getElementById('toggleLogin');
  loginSw.setAttribute('aria-checked', String(!!settings.openAtLogin));

  document.querySelectorAll('#themeSeg .seg').forEach((b) => {
    b.setAttribute('aria-checked', String(b.dataset.val === (settings.theme || 'system')));
  });

  // Don't clobber the field while the user is typing in it.
  const tokenEl = document.getElementById('ccToken');
  if (tokenEl && document.activeElement !== tokenEl) tokenEl.value = settings.ccToken || '';

  if (lastData) render(lastData); // theme change → recolour ring/bars, toggle → number
}

async function commit(patch) {
  settings = { ...settings, ...patch };
  applySettings();                                  // optimistic
  const fresh = await window.usageAPI.setSettings(patch);
  if (fresh) { settings = fresh; applySettings(); } // reconcile with truth
}

function showView(name) {
  document.getElementById('mainView').hidden = name !== 'main';
  document.getElementById('settingsView').hidden = name !== 'settings';
  fitWindow();
}

// Resize the native window to the card's natural height so there's never dead
// space or a stray scrollbar, whatever the row count or which view is showing.
function fitWindow() {
  requestAnimationFrame(() => {
    const card = document.getElementById('card');
    if (card) window.usageAPI.resize(Math.ceil(card.getBoundingClientRect().height) + (GLASS ? 0 : 20));
  });
}

// --------------------------------------------------------------------------
// Wire up
// --------------------------------------------------------------------------
document.getElementById('openSettings').addEventListener('click', () => showView('settings'));
document.getElementById('backSettings').addEventListener('click', () => showView('main'));

document.getElementById('toggleNumber').addEventListener('click', () => commit({ showNumber: !settings.showNumber }));
document.getElementById('toggleLogin').addEventListener('click', () => commit({ openAtLogin: !settings.openAtLogin }));
document.querySelectorAll('#themeSeg .seg').forEach((b) => {
  b.addEventListener('click', () => commit({ theme: b.dataset.val }));
});

const saveTokenBtn = document.getElementById('saveToken');
const ccTokenInput = document.getElementById('ccToken');
if (saveTokenBtn && ccTokenInput) {
  const saveToken = async () => {
    saveTokenBtn.disabled = true;
    try { await commit({ ccToken: ccTokenInput.value.trim() }); }
    finally { saveTokenBtn.disabled = false; }
  };
  saveTokenBtn.addEventListener('click', saveToken);
  ccTokenInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') saveToken(); });
}

document.getElementById('refresh').addEventListener('click', async () => {
  const btn = document.getElementById('refresh');
  btn.disabled = true;
  try { await window.usageAPI.refresh(); } finally { btn.disabled = false; }
});
document.getElementById('signin').addEventListener('click', () => window.usageAPI.openClaude());

window.usageAPI.onUsage(render);
window.usageAPI.onSettings((s) => { settings = s; applySettings(); });
window.usageAPI.onResetView(() => showView('main'));

// Follow live OS theme changes while on "system".
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
  if ((settings.theme || 'system') === 'system' && lastData) render(lastData);
});

// Initial pull (covers pushes that fired before the listeners attached).
(async () => {
  try {
    const s = await window.usageAPI.getSettings();
    if (s) { settings = s; }
  } catch { /* keep defaults */ }
  applySettings();
  try {
    const u = await window.usageAPI.getUsage();
    if (u) render(u);
  } catch { /* poll will push soon */ }
})();
