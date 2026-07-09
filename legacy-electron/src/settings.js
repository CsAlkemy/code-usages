'use strict';

// Persistent user preferences, stored as JSON in the OS userData dir.
// `openAtLogin` is not kept here — it's owned by the OS (app.getLoginItemSettings).

const { app } = require('electron');
const path = require('path');
const fs = require('fs');

const DEFAULTS = {
  showNumber: false,        // show the % next to the menu-bar ring / in the popover ring
  theme: 'system',          // 'system' | 'light' | 'dark'
};

let cache = null;

function file() {
  return path.join(app.getPath('userData'), 'settings.json');
}

function load() {
  if (cache) return cache;
  try {
    cache = { ...DEFAULTS, ...JSON.parse(fs.readFileSync(file(), 'utf8')) };
  } catch {
    cache = { ...DEFAULTS };
  }
  return cache;
}

function save(patch) {
  cache = { ...load(), ...patch };
  try { fs.writeFileSync(file(), JSON.stringify(cache, null, 2)); } catch { /* best effort */ }
  return cache;
}

// The full settings object the UI sees, including OS-backed openAtLogin.
function getAll() {
  const s = load();
  let openAtLogin = false;
  try { openAtLogin = app.getLoginItemSettings().openAtLogin; } catch { /* unsupported */ }
  return { showNumber: s.showNumber, theme: s.theme, openAtLogin };
}

module.exports = { DEFAULTS, load, save, getAll };
