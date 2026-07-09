// Fallback data source: Claude Code's local credentials.
//
// Used only when the claude.ai *web* session has no usage (the user signs in
// to Claude Code with a setup token and never does the browser login). We take
// the OAuth/setup token Claude Code already holds and call the same usage API
// the web path uses, but authenticated with `Authorization: Bearer` instead of
// cookies — so the response feeds the existing usage::normalize() unchanged.
//
// Token sources, in order (see manual_or_env / read_auto_token): a token the
// user pasted into settings, then the OAuth-token env var, then Claude Code's
// credential file, then the macOS Keychain. The auto-read paths (file/Keychain)
// are only touched when the web session has no data — so a user signed in on
// the web never triggers a Keychain prompt. (A user who *intends* to sign in on
// the web but hasn't yet also has an empty session, so they may see the one-time
// prompt during onboarding; clicking "Always Allow" or signing in dismisses it.)
use crate::usage;
use serde_json::Value;
use std::process::Command;

// Candidate usage APIs, tried in order until one returns a parseable payload.
// api.anthropic.com first: Claude Code's setup tokens (sk-ant-oat01-…) are
// API-scoped and authenticate there; claude.ai Cloudflare-blocks non-browser
// requests, so it's only a fallback for cookie-less subscription tokens.
const HOSTS: &[&str] = &["https://api.anthropic.com", "https://claude.ai"];

// Cheap, prompt-free sources: a token the user pasted, or the OAuth-token env
// var. NOT ANTHROPIC_API_KEY — that's a console API key (sk-ant-api03-…) used
// via x-api-key; it can't authenticate the OAuth usage endpoint and would only
// shadow the real token in the credential store.
pub fn manual_or_env(manual: Option<&str>) -> Option<String> {
    if let Some(t) = manual {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Ok(v) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

// Credential-store sources (may trigger a one-time macOS Keychain prompt). The
// caller decides when this is allowed and caches the result so it runs rarely.
pub fn read_auto_token() -> Option<String> {
    if let Some(t) = token_from_file() {
        return Some(t);
    }
    #[cfg(target_os = "macos")]
    if let Some(t) = token_from_keychain() {
        return Some(t);
    }
    None
}

fn token_from_file() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let body = std::fs::read_to_string(format!("{}/.claude/.credentials.json", home)).ok()?;
    let json: Value = serde_json::from_str(&body).ok()?;
    token_from_blob(&json)
}

#[cfg(target_os = "macos")]
fn token_from_keychain() -> Option<String> {
    // Reads the item Claude Code created; macOS prompts once for our app to
    // access it (the user clicks Always Allow). No secret is logged.
    let out = Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8(out.stdout).ok()?;
    let raw = raw.trim();
    // The stored password is usually a JSON blob; occasionally a bare token.
    match serde_json::from_str::<Value>(raw) {
        Ok(json) => token_from_blob(&json),
        Err(_) if looks_like_token(raw) => Some(raw.to_string()),
        Err(_) => None,
    }
}

// Pull the access token out of Claude Code's credential JSON. Known shape:
// { "claudeAiOauth": { "accessToken": "sk-ant-oat01-…", … } }. We also accept
// any nested string under an access-token-ish key, so a schema tweak upstream
// doesn't silently break us.
fn token_from_blob(json: &Value) -> Option<String> {
    if let Some(t) = json.pointer("/claudeAiOauth/accessToken").and_then(Value::as_str) {
        return Some(t.to_string());
    }
    fn walk(node: &Value) -> Option<String> {
        match node {
            Value::Object(map) => {
                for (k, v) in map {
                    let kl = k.to_lowercase();
                    if (kl.contains("access") && kl.contains("token")) || kl == "token" {
                        if let Some(s) = v.as_str() {
                            if looks_like_token(s) {
                                return Some(s.to_string());
                            }
                        }
                    }
                    if let Some(found) = walk(v) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    walk(json)
}

fn looks_like_token(s: &str) -> bool {
    let s = s.trim();
    s.len() > 20 && (s.starts_with("sk-ant-") || s.starts_with("oauth") || !s.contains(char::is_whitespace))
}

// (body-if-2xx-json, http status or 0 on network error).
async fn api_get(client: &reqwest::Client, url: &str, token: &str) -> (Option<Value>, u16) {
    let resp = match client
        .get(url)
        .header("authorization", format!("Bearer {}", token))
        .header("accept", "application/json")
        // Anthropic's OAuth-scoped endpoints expect these; claude.ai ignores them.
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return (None, 0),
    };
    let status = resp.status().as_u16();
    eprintln!("[claude-code] GET {} -> {}", url, status);
    if !(200..300).contains(&status) {
        return (None, status);
    }
    (resp.json::<Value>().await.ok(), status)
}

// A short, human-readable reason when the fetch produced no data — shown in
// the settings token field so failures aren't silent.
pub fn status_note(status: u16) -> &'static str {
    match status {
        0 => "couldn't reach the usage API (network?)",
        401 => "token rejected (401) — try `claude setup-token` again",
        403 => "access denied (403) — token may lack usage scope",
        429 => "rate-limited (429) — will retry automatically",
        s if (500..600).contains(&s) => "Anthropic API error (5xx) — will retry",
        200 => "connected, but the response wasn't recognized",
        _ => "usage API returned an unexpected status",
    }
}

pub struct Outcome {
    pub model: Option<Value>,
    pub status: u16, // the most informative status seen (for diagnostics)
}

// Call the usage API with the given token. Setup tokens authenticate at
// api.anthropic.com; the flat /api/oauth/usage returns the same `limits` shape
// the web path uses, so it's tried first, with org-scoped paths as fallback.
pub async fn fetch_with_token(token: &str, user_agent: &str) -> Outcome {
    let client = match reqwest::Client::builder().user_agent(user_agent).build() {
        Ok(c) => c,
        Err(_) => return Outcome { model: None, status: 0 },
    };
    let mut last_status = 0u16;

    for host in HOSTS {
        let (raw, status) = api_get(&client, &format!("{}/api/oauth/usage", host), token).await;
        if status != 0 {
            last_status = status;
        }
        if let Some(raw) = raw {
            if let Some(model) = usage::normalize(&raw, None) {
                return Outcome { model: Some(model), status };
            }
            last_status = 200; // reached it but couldn't parse
        }
        // 429 is a global rate limit for this token — more requests only
        // deepen it, so stop entirely and report it.
        if status == 429 {
            break;
        }
        // 401/403 means THIS host rejected the token; a different host may
        // accept it (setup tokens → api.anthropic.com, subscription/cookie
        // tokens → claude.ai). Skip this host's org path, try the next host.
        if matches!(status, 401 | 403) {
            continue;
        }

        let (orgs, _) = api_get(&client, &format!("{}/api/organizations", host), token).await;
        if let Some(list) = orgs.as_ref().and_then(Value::as_array) {
            let org = list
                .iter()
                .find(|o| {
                    o["capabilities"]
                        .as_array()
                        .map(|c| c.iter().any(|v| v == "chat"))
                        .unwrap_or(false)
                })
                .or_else(|| list.first());
            if let Some(uuid) = org.and_then(|o| o["uuid"].as_str()) {
                let plan = org.and_then(|o| o["raven_type"].as_str());
                let url = format!("{}/api/organizations/{}/usage", host, uuid);
                let (raw, status) = api_get(&client, &url, token).await;
                if status != 0 {
                    last_status = status;
                }
                if let Some(raw) = raw {
                    if let Some(model) = usage::normalize(&raw, plan) {
                        return Outcome { model: Some(model), status };
                    }
                    last_status = 200;
                }
            }
        }
    }
    Outcome { model: None, status: last_status }
}
