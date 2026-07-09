// Injected into the claude.ai page (main world) via executeJavaScript.
// It hooks fetch + XHR, and whenever a JSON response "looks like" the usage
// payload, it stashes { url, raw, ts } on window.__CLAUDE_USAGE_CAPTURE__.
// The main process reads that, and once it knows the URL it re-fetches on a
// schedule. Nothing here is claude.ai-specific beyond the heuristic keywords,
// so it self-heals if the endpoint path changes.
(function () {
  if (window.__CLAUDE_USAGE_HOOKED__) return;
  window.__CLAUDE_USAGE_HOOKED__ = true;
  if (typeof window.__CLAUDE_USAGE_CAPTURE__ === 'undefined') {
    window.__CLAUDE_USAGE_CAPTURE__ = null;
  }

  // Usage-specific tokens (STRONG) vs. generic ones (WEAK). Analytics / config
  // blobs (Segment etc.) also contain generic words, so BAD rejects them first —
  // otherwise a tracking-config response gets mistaken for the usage payload.
  var STRONG = ['utilization', 'resets_at', 'reset_at', 'five_hour', 'seven_day', 'remaining', 'rate_limit', 'usage_limit'];
  var WEAK = ['session', 'weekly', 'limit', 'used', 'quota', 'usage'];
  var BAD = ['segment.io', 'consentsettings', 'unbundledintegrations', 'maybebundledconfigids',
    'componenttypes', 'versionsettings', 'apihost', '"integrations"', 'tracking'];

  function looksLikeUsage(txt, url) {
    if (!txt || txt.length > 500000) return false;
    var low = txt.toLowerCase();
    for (var b = 0; b < BAD.length; b++) if (low.indexOf(BAD[b]) !== -1) return false; // analytics/config
    var u = (url || '').toLowerCase();
    if (/usage|rate.?limit|\/limits?(\b|\/|\?)/.test(u)) return true;                  // usage-ish endpoint URL
    var s = 0, w = 0;
    for (var i = 0; i < STRONG.length; i++) if (low.indexOf(STRONG[i]) !== -1) s++;
    for (var j = 0; j < WEAK.length; j++) if (low.indexOf(WEAK[j]) !== -1) w++;
    return s >= 2 || (s >= 1 && w >= 2);
  }

  if (!Array.isArray(window.__CLAUDE_USAGE_CAPTURES__)) window.__CLAUDE_USAGE_CAPTURES__ = [];

  function consider(url, txt, method) {
    try {
      if (!looksLikeUsage(txt, url)) return;
      var data = JSON.parse(txt);
      var entry = { url: String(url), method: method || 'GET', raw: data, ts: Date.now() };
      window.__CLAUDE_USAGE_CAPTURE__ = entry;
      // Keep a short history too — handy when several endpoints look usage-like.
      window.__CLAUDE_USAGE_CAPTURES__.push({ url: entry.url, method: entry.method, ts: entry.ts });
      if (window.__CLAUDE_USAGE_CAPTURES__.length > 20) window.__CLAUDE_USAGE_CAPTURES__.shift();
    } catch (e) { /* not JSON — ignore */ }
  }

  var origFetch = window.fetch;
  if (origFetch) {
    window.fetch = function () {
      var args = arguments;
      return origFetch.apply(this, args).then(function (res) {
        try {
          var u = (res && res.url) ||
            (typeof args[0] === 'string' ? args[0] : (args[0] && args[0].url));
          var m = (args[1] && args[1].method) ||
            (args[0] && typeof args[0] === 'object' && args[0].method) || 'GET';
          var ct = (res.headers && res.headers.get && res.headers.get('content-type')) || '';
          if (ct.indexOf('json') !== -1) {
            res.clone().text().then(function (t) { consider(u, t, m); }).catch(function () {});
          }
        } catch (e) {}
        return res;
      });
    };
  }

  var xo = XMLHttpRequest.prototype.open;
  var xs = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (m, u) { this.__usageUrl = u; this.__usageMethod = m; return xo.apply(this, arguments); };
  XMLHttpRequest.prototype.send = function () {
    var self = this;
    this.addEventListener('load', function () {
      try { consider(self.__usageUrl, self.responseText, self.__usageMethod); } catch (e) {}
    });
    return xs.apply(this, arguments);
  };
})();
