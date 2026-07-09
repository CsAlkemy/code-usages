// Port of the Electron app's src/usage.js: turn the raw claude.ai usage
// payload into the UI model { plan, session, weekly, signedOut }.
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

pub fn signed_out() -> Value {
    json!({ "plan": "", "session": null, "weekly": [], "signedOut": true })
}

fn clamp(n: f64) -> i64 {
    n.round().max(0.0).min(100.0) as i64
}

fn to_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let digits: String = s
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '-')
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .collect();
            digits.parse().ok()
        }
        _ => None,
    }
}

const PCT_KEYS: &[&str] = &["utilization", "percent", "percentage", "pct", "percent_used",
    "used_percent", "usage_percent", "usagePercentage", "fraction", "ratio"];
const USED_KEYS: &[&str] = &["used", "consumed", "count", "used_credits", "current", "value", "usage"];
const LIMIT_KEYS: &[&str] = &["limit", "cap", "total", "allowance", "max", "maximum", "quota"];
const RESET_KEYS: &[&str] = &["resets_at", "reset_at", "resetsAt", "resetAt", "resets", "reset",
    "next_reset", "next_reset_at", "expires_at", "expiresAt", "window_end", "end_time", "ends_at"];

fn first_number(obj: &Map<String, Value>, names: &[&str]) -> Option<f64> {
    names.iter().find_map(|n| obj.get(*n).and_then(to_num))
}

fn pct_of(obj: &Map<String, Value>) -> Option<i64> {
    if let Some(mut p) = first_number(obj, PCT_KEYS) {
        if p > 0.0 && p <= 1.0 {
            p *= 100.0;
        }
        return Some(clamp(p));
    }
    let limit = first_number(obj, LIMIT_KEYS)?;
    if limit <= 0.0 {
        return None;
    }
    if let Some(used) = first_number(obj, USED_KEYS) {
        return Some(clamp(100.0 * used / limit));
    }
    let remaining = first_number(obj, &["remaining", "left", "available"])?;
    Some(clamp(100.0 * (limit - remaining) / limit))
}

pub fn format_reset(v: &Value) -> String {
    let t: Option<DateTime<Utc>> = match v {
        Value::String(s) => DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc)),
        Value::Number(n) => n.as_f64().and_then(|secs| {
            let secs = if secs > 1e12 { secs / 1000.0 } else { secs };
            DateTime::from_timestamp(secs as i64, 0)
        }),
        _ => None,
    };
    let Some(t) = t else { return String::new() };
    let mins = (t - Utc::now()).num_minutes();
    if mins <= 0 {
        return "Resets soon".into();
    }
    let h = mins / 60;
    let m = mins % 60;
    if h >= 12 {
        let d = ((h as f64) / 24.0).round().max(1.0) as i64;
        return format!("Resets in {} day{}", d, if d > 1 { "s" } else { "" });
    }
    if h > 0 {
        format!("Resets in {} hr {} min", h, m)
    } else {
        format!("Resets in {} min", m)
    }
}

fn pretty_label(hint: &str) -> String {
    match hint {
        "five_hour" | "five_hour_limit" | "session" | "current_session" => "Current session".into(),
        "seven_day" | "seven_day_limit" | "weekly" | "all_models" | "weekly_all" => "All models".into(),
        "" => "Usage".into(),
        other => {
            let base = other.trim_end_matches("_limit").trim_end_matches("_usage");
            let mut out = String::new();
            for (i, part) in base.split(['_', '-']).enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let mut cs = part.chars();
                if let Some(c) = cs.next() {
                    out.extend(c.to_uppercase());
                    out.push_str(cs.as_str());
                }
            }
            out
        }
    }
}

fn label_for_limit(l: &Value) -> String {
    if let Some(name) = l.pointer("/scope/model/display_name").and_then(Value::as_str) {
        return name.into();
    }
    let kind = l["kind"].as_str().unwrap_or("");
    let group = l["group"].as_str().unwrap_or("");
    if kind.to_lowercase().contains("session") || group == "session" {
        return "Current session".into();
    }
    if kind == "weekly_all" {
        return "All models".into();
    }
    pretty_label(if kind.is_empty() { group } else { kind })
}

fn plan_name(raw: &Value, plan_hint: Option<&str>) -> String {
    let p = plan_hint
        .map(String::from)
        .or_else(|| raw["__plan"].as_str().map(String::from))
        .or_else(|| raw["plan"].as_str().map(String::from))
        .or_else(|| raw["raven_type"].as_str().map(String::from))
        .unwrap_or_default();
    let mut cs = p.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

// Preferred path: /api/organizations/{uuid}/usage returns a clean `limits`
// array: { kind, group: 'session'|'weekly', percent, resets_at, scope }.
fn from_limits(raw: &Value, plan_hint: Option<&str>) -> Option<Value> {
    let limits = raw["limits"].as_array()?;
    let rows: Vec<Value> = limits
        .iter()
        .filter(|l| l.is_object() && (!l["percent"].is_null() || !l["group"].is_null() || !l["kind"].is_null()))
        .map(|l| {
            let kind = l["kind"].as_str().unwrap_or("");
            let group = match l["group"].as_str() {
                Some(g) => g.to_string(),
                None => {
                    let k = kind.to_lowercase();
                    if k.contains("session") || k.contains("hour") || k.contains("5h") {
                        "session".into()
                    } else {
                        "weekly".into()
                    }
                }
            };
            let reset = l.get("resets_at").or_else(|| l.get("reset_at")).map(format_reset).unwrap_or_default();
            json!({
                "group": group,
                "label": label_for_limit(l),
                "pct": clamp(to_num(&l["percent"]).unwrap_or(0.0)),
                "reset": reset,
            })
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    let session_idx = rows.iter().position(|r| r["group"] == "session");
    let session = session_idx.map(|i| strip(&rows[i]));
    let weekly: Vec<Value> = rows
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != session_idx)
        .take(6)
        .map(|(_, r)| strip(r))
        .collect();
    Some(json!({
        "plan": plan_name(raw, plan_hint),
        "session": session,
        "weekly": weekly,
        "signedOut": false,
    }))
}

fn strip(r: &Value) -> Value {
    json!({ "label": r["label"], "pct": r["pct"], "reset": r["reset"] })
}

// Fallback: walk the payload for anything percentage-like with a reset stamp.
fn generic_walk(raw: &Value, plan_hint: Option<&str>) -> Option<Value> {
    let mut rows: Vec<(String, Value)> = Vec::new();
    fn visit(node: &Value, hint: &str, rows: &mut Vec<(String, Value)>) {
        let Some(obj) = node.as_object() else { return };
        if let Some(pct) = pct_of(obj) {
            let reset = RESET_KEYS
                .iter()
                .find_map(|k| obj.get(*k))
                .map(format_reset)
                .unwrap_or_default();
            rows.push((hint.to_string(), json!({ "label": pretty_label(hint), "pct": pct, "reset": reset })));
        }
        for (k, v) in obj {
            visit(v, k, rows);
        }
    }
    visit(raw, "usage", &mut rows);
    if rows.is_empty() {
        return None;
    }
    let session_idx = rows
        .iter()
        .position(|(h, _)| {
            let h = h.to_lowercase();
            h.contains("session") || h.contains("hour") || h.contains("current")
        })
        .unwrap_or(0);
    let session = rows[session_idx].1.clone();
    let weekly: Vec<Value> = rows
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != session_idx)
        .take(5)
        .map(|(_, item)| item.1.clone())
        .collect();
    Some(json!({
        "plan": plan_name(raw, plan_hint),
        "session": session,
        "weekly": weekly,
        "signedOut": false,
    }))
}

pub fn normalize(raw: &Value, plan_hint: Option<&str>) -> Option<Value> {
    if !raw.is_object() {
        return None;
    }
    from_limits(raw, plan_hint).or_else(|| generic_walk(raw, plan_hint))
}

// Value for the tray ring: current session first, highest row as fallback.
pub fn primary(model: &Value) -> Option<i64> {
    if let Some(p) = model.pointer("/session/pct").and_then(Value::as_i64) {
        return Some(p);
    }
    model["weekly"]
        .as_array()?
        .iter()
        .filter_map(|w| w["pct"].as_i64())
        .max()
}
