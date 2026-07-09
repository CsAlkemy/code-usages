'use strict';
// Preload for the hidden claude.ai session window. Runs at document-start in
// the page world (the window uses contextIsolation:false), so the fetch/XHR
// hooks in inject-session.js are installed BEFORE claude.ai fires its first
// request — including the initial bootstrap that carries the usage payload.
// Injecting later (on did-finish-load) misses that early request entirely.
try {
  require('./inject-session.js');
} catch (e) {
  /* ignore — the did-finish-load fallback in main.js still runs */
}
