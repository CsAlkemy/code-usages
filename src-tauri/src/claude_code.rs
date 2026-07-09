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
// claude.ai first: subscription/setup tokens are claude.ai OAuth tokens, and
// that endpoint returns the `limits` array normalize() already understands.
const HOSTS: &[&str] = &["https://claude.ai", "https://api.anthropic.com"];

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

async fn api_get(client: &reqwest::Client, url: &str, token: &str) -> Option<Value> {
    let resp = client
        .get(url)
        .header("authorization", format!("Bearer {}", token))
        .header("accept", "application/json")
        // Anthropic's OAuth-scoped endpoints expect these; claude.ai ignores them.
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .ok()?;
    let status = resp.status();
    eprintln!("[claude-code] GET {} -> {}", url, status);
    if !status.is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

// Call the usage API with the given token. Verified live: claude.ai's flat
// /api/oauth/usage returns the same `limits` shape the web path uses, so it's
// tried first; the org-scoped and api.anthropic.com paths remain as fallbacks
// in case that endpoint changes.
pub async fn fetch_with_token(token: &str, user_agent: &str) -> Option<Value> {
    let client = reqwest::Client::builder().user_agent(user_agent).build().ok()?;

    for host in HOSTS {
        if let Some(raw) = api_get(&client, &format!("{}/api/oauth/usage", host), token).await {
            if let Some(model) = usage::normalize(&raw, None) {
                return Some(model);
            }
        }
        if let Some(orgs) = api_get(&client, &format!("{}/api/organizations", host), token).await {
            if let Some(list) = orgs.as_array() {
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
                    if let Some(raw) = api_get(&client, &url, token).await {
                        if let Some(model) = usage::normalize(&raw, plan) {
                            return Some(model);
                        }
                    }
                }
            }
        }
    }
    None
}
