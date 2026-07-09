'use strict';

// Shown until real data is captured (not signed in yet, or fetch failed).
// No fake numbers — the UI renders an explicit signed-out state instead.
const SIGNED_OUT = { plan: '', session: null, weekly: [], signedOut: true };

const clamp = (n) => Math.max(0, Math.min(100, Math.round(n)));

// Coerce numbers out of numbers OR strings like "17", "17%", "17% used".
function toNum(v) {
  if (typeof v === 'number' && isFinite(v)) return v;
  if (typeof v === 'string') {
    const m = v.match(/-?\d+(?:\.\d+)?/);
    if (m) return parseFloat(m[0]);
  }
  return null;
}

function firstNumber(obj, names) {
  for (const n of names) {
    if (obj && obj[n] != null) {
      const v = toNum(obj[n]);
      if (v != null) return v;
    }
  }
  return null;
}

const PCT_KEYS = ['utilization', 'percent', 'percentage', 'pct', 'percent_used',
  'used_percent', 'usage_percent', 'usagePercentage', 'fraction', 'ratio'];
const USED_KEYS = ['used', 'consumed', 'count', 'used_credits', 'current', 'value', 'usage'];
const LIMIT_KEYS = ['limit', 'cap', 'total', 'allowance', 'max', 'maximum', 'quota'];
const REMAINING_KEYS = ['remaining', 'left', 'available'];
const RESET_KEYS = ['resets_at', 'reset_at', 'resetsAt', 'resetAt', 'resets', 'reset',
  'next_reset', 'next_reset_at', 'expires_at', 'expiresAt', 'window_end', 'end_time', 'ends_at'];

// Turn one object node into a percentage, if it carries one in any known form.
function pctOf(node) {
  let p = null;
  for (const k of PCT_KEYS) {
    if (node[k] != null) { p = toNum(node[k]); if (p != null) break; }
  }
  if (p != null) { if (p > 0 && p <= 1) p *= 100; return clamp(p); } // 0..1 → 0..100

  const limit = firstNumber(node, LIMIT_KEYS);
  const used = firstNumber(node, USED_KEYS);
  if (used != null && limit != null && limit > 0) return clamp((100 * used) / limit);

  const remaining = firstNumber(node, REMAINING_KEYS);
  if (remaining != null && limit != null && limit > 0) return clamp((100 * (limit - remaining)) / limit);

  return null;
}

// ===========================================================================
// Shape-agnostic pass: walk the payload, and for every object that carries a
// percentage (or a used/limit or remaining/limit pair) plus (ideally) a reset
// timestamp, emit a row. Then split a session-ish row from the weekly ones.
// If the app shows the wrong numbers, capture the real payload (tray menu →
// "Reveal captured usage (JSON)…") and map its exact fields here.
// ===========================================================================
// Preferred path: claude.ai's /api/organizations/{uuid}/usage returns a clean
// `limits` array. Each entry: { kind, group:'session'|'weekly', percent,
// resets_at, scope:{model:{display_name}}, is_active }. This is exact — use it
// whenever it's present, and fall back to the generic walk otherwise.
function fromLimits(raw) {
  if (!Array.isArray(raw.limits) || !raw.limits.length) return null;
  const rows = raw.limits
    .filter((l) => l && (l.percent != null || l.group || l.kind))
    .map((l) => ({
      group: l.group || (/(session|5.?h|hour)/i.test(l.kind || '') ? 'session' : 'weekly'),
      label: labelForLimit(l),
      pct: clamp(toNum(l.percent) ?? 0),
      reset: formatReset(l.resets_at || l.reset_at),
    }));
  if (!rows.length) return null;
  const session = rows.find((r) => r.group === 'session') || null;
  const weekly = rows.filter((r) => r !== session).slice(0, 6);
  return { plan: planName(raw), session: strip(session), weekly: weekly.map(strip), mock: false };
}

function labelForLimit(l) {
  if (l.scope && l.scope.model && l.scope.model.display_name) return l.scope.model.display_name;
  if ((l.group || l.kind) && /session/i.test(l.kind || l.group)) return 'Current session';
  if (l.kind === 'weekly_all') return 'All models';
  return prettyLabel(l.kind || l.group);
}

function planName(raw) {
  const p = raw.__plan || raw.plan || raw.raven_type || raw.tier || raw.subscription;
  if (typeof p === 'string' && p) return p.charAt(0).toUpperCase() + p.slice(1);
  return '';
}

function normalize(raw) {
  if (!raw || typeof raw !== 'object') return null;

  const viaLimits = fromLimits(raw);
  if (viaLimits) return viaLimits;

  const rows = [];
  const seen = new Set();

  const visit = (node, keyHint) => {
    if (!node || typeof node !== 'object' || seen.has(node)) return;
    seen.add(node);

    const pct = pctOf(node);
    if (pct !== null) {
      const resetRaw = RESET_KEYS.map((k) => node[k]).find((v) => v != null) || null;
      rows.push({ label: prettyLabel(keyHint), pct, reset: formatReset(resetRaw), _hint: keyHint });
    }

    for (const k of Object.keys(node)) visit(node[k], k);
  };

  visit(raw, 'usage');
  if (!rows.length) return null;

  const sessionRow =
    rows.find((r) => /session|5.?h|five.?hour|hour|current/i.test(r._hint || '')) || rows[0];
  const weekly = rows.filter((r) => r !== sessionRow).slice(0, 5);

  return {
    plan: pickString(raw, ['plan', 'tier', 'subscription', 'account_type']) || '',
    session: strip(sessionRow),
    weekly: weekly.length ? weekly.map(strip) : [],
    mock: false,
  };
}

function pickString(obj, names) {
  for (const n of names) if (typeof obj[n] === 'string' && obj[n]) return obj[n];
  return '';
}

function strip(r) { return r ? { label: r.label, pct: r.pct, reset: r.reset } : null; }

function prettyLabel(hint) {
  if (!hint) return 'Usage';
  const map = {
    five_hour: 'Current session', five_hour_limit: 'Current session',
    session: 'Current session', current_session: 'Current session',
    seven_day: 'All models', seven_day_limit: 'All models', weekly: 'All models',
    all_models: 'All models',
  };
  if (map[hint]) return map[hint];
  return String(hint).replace(/_limit$|_usage$/i, '').replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatReset(v) {
  if (!v) return '';
  const t = typeof v === 'number' ? new Date(v > 1e12 ? v : v * 1000) : new Date(v);
  if (isNaN(t.getTime())) return '';
  const mins = Math.max(0, Math.round((t.getTime() - Date.now()) / 60000));
  if (mins <= 0) return 'Resets soon';
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h >= 12) {
    const d = Math.max(1, Math.round(h / 24));
    return `Resets in ${d} day${d > 1 ? 's' : ''}`;
  }
  return `Resets in ${h ? h + ' hr ' : ''}${m} min`;
}

// Highest of all rows — the "closest to limit" fallback value.
function closest(model) {
  const all = [];
  if (model.session && typeof model.session.pct === 'number') all.push(model.session.pct);
  (model.weekly || []).forEach((w) => typeof w.pct === 'number' && all.push(w.pct));
  return all.length ? Math.max(...all) : null;
}

// Value shown in the tray ring + menu-bar number: the current session,
// falling back to the highest row when there's no session limit in the data.
function primary(model) {
  if (model.session && typeof model.session.pct === 'number') return model.session.pct;
  return closest(model);
}

module.exports = { SIGNED_OUT, normalize, closest, primary };
