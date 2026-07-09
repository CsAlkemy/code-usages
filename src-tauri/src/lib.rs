mod ring;
mod settings;
mod usage;

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_positioner::{Position, WindowExt};

const CLAUDE_ORIGIN: &str = "https://claude.ai";
// The webview is WKWebView (Safari's engine), so the UA must claim Safari —
// a Chrome UA on WebKit is an engine/UA mismatch that makes Cloudflare's
// Turnstile checkbox loop forever. (Electron needed the opposite: a Chrome UA
// to match its Chromium engine.) The reqwest client reuses this UA because
// the cf_clearance cookie is bound to the UA that solved the challenge.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15";
const POLL_SECS: u64 = 240;
const POPOVER_W: f64 = 340.0;
const POPOVER_H: f64 = 384.0;

struct AppState {
    last_model: Value,
    learned_usage_url: Option<String>,
    learned_plan: Option<String>,
    tray_rect: Option<tauri::Rect>,
    last_toggle: Option<std::time::Instant>,
    popover_shown_at: Option<std::time::Instant>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_model: usage::signed_out(),
            learned_usage_url: None,
            learned_plan: None,
            tray_rect: None,
            last_toggle: None,
            popover_shown_at: None,
        }
    }
}

fn is_dark(app: &AppHandle) -> bool {
    let s = settings::load(app);
    match s.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => app
            .get_webview_window("popover")
            .and_then(|w| w.theme().ok())
            .map(|t| t == Theme::Dark)
            .unwrap_or(false),
    }
}

// ---------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------
fn update_tray(app: &AppHandle, model: &Value) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let top = usage::primary(model);
    let rgba = ring::draw(top, is_dark(app));
    let _ = tray.set_icon(Some(Image::new_owned(rgba, ring::SIZE, ring::SIZE)));

    let s = settings::load(app);
    #[cfg(target_os = "macos")]
    {
        let title = match (s.show_number, top) {
            (true, Some(t)) => Some(format!(" {}%", t)),
            _ => None,
        };
        let _ = tray.set_title(title);
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = model.pointer("/session/pct").and_then(Value::as_i64) {
        parts.push(format!("Session {}%", p));
    }
    if let Some(weekly) = model["weekly"].as_array() {
        for w in weekly {
            if let (Some(l), Some(p)) = (w["label"].as_str(), w["pct"].as_i64()) {
                parts.push(format!("{} {}%", l, p));
            }
        }
    }
    let tip = if parts.is_empty() {
        "Claude usage — not signed in".to_string()
    } else {
        format!("Claude usage — {}", parts.join(" · "))
    };
    let _ = tray.set_tooltip(Some(tip));
}

// ---------------------------------------------------------------------------
// Usage fetch: read the session webview's cookies and call the API directly.
// ---------------------------------------------------------------------------
fn cookie_header(app: &AppHandle) -> Option<String> {
    let session = app.get_webview_window("session")?;
    let url: tauri::Url = CLAUDE_ORIGIN.parse().ok()?;
    let cookies = session.cookies_for_url(url).ok()?;
    if cookies.is_empty() {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

async fn api_get(client: &reqwest::Client, cookies: &str, path: &str) -> Option<Value> {
    let resp = client
        .get(format!("{}{}", CLAUDE_ORIGIN, path))
        .header("cookie", cookies)
        .header("accept", "application/json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

async fn fetch_usage(app: &AppHandle) -> Option<Value> {
    let cookies = cookie_header(app)?;
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .ok()?;

    let (known_url, known_plan) = {
        let state = app.state::<Mutex<AppState>>();
        let s = state.lock().unwrap();
        (s.learned_usage_url.clone(), s.learned_plan.clone())
    };

    let (usage_url, plan) = match known_url {
        Some(u) => (u, known_plan),
        None => {
            let orgs = api_get(&client, &cookies, "/api/organizations").await?;
            let orgs = orgs.as_array()?;
            let org = orgs
                .iter()
                .find(|o| {
                    o["capabilities"]
                        .as_array()
                        .map(|c| c.iter().any(|v| v == "chat"))
                        .unwrap_or(false)
                })
                .or_else(|| orgs.first())?;
            let uuid = org["uuid"].as_str()?;
            let plan = org["raven_type"].as_str().map(String::from);
            let url = format!("/api/organizations/{}/usage", uuid);
            let state = app.state::<Mutex<AppState>>();
            let mut s = state.lock().unwrap();
            s.learned_usage_url = Some(url.clone());
            s.learned_plan = plan.clone();
            (url, plan)
        }
    };

    let raw = api_get(&client, &cookies, &usage_url).await?;
    usage::normalize(&raw, plan.as_deref())
}

async fn poll(app: AppHandle) {
    let model = fetch_usage(&app).await.unwrap_or_else(usage::signed_out);
    let signed_out = model["signedOut"].as_bool().unwrap_or(false);
    let was_signed_out = {
        let state = app.state::<Mutex<AppState>>();
        let mut s = state.lock().unwrap();
        let was = s.last_model["signedOut"].as_bool().unwrap_or(true);
        s.last_model = model.clone();
        was
    };
    // Sign-in just completed — tuck the login window away.
    if !signed_out && was_signed_out {
        if let Some(w) = app.get_webview_window("session") {
            if w.is_visible().unwrap_or(false) {
                let _ = w.hide();
            }
        }
    }
    update_tray(&app, &model);
    let _ = app.emit_to("popover", "usage", &model);
}

fn spawn_poll(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { poll(app).await });
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------
fn create_session_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let url: tauri::Url = CLAUDE_ORIGIN.parse().expect("origin url");
    let win = WebviewWindowBuilder::new(app, "session", WebviewUrl::External(url))
        .title("Sign in to Claude")
        .inner_size(1040.0, 820.0)
        .visible(false)
        .user_agent(USER_AGENT)
        .on_page_load({
            let app = app.clone();
            move |_webview, _payload| {
                // During the login flow every page load is a step forward —
                // poll shortly after each one so data appears right away.
                let signed_out = {
                    let state = app.state::<Mutex<AppState>>();
                    let s = state.lock().unwrap();
                    s.last_model["signedOut"].as_bool().unwrap_or(true)
                };
                if signed_out {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio_sleep(Duration::from_millis(1200)).await;
                        poll(app).await;
                    });
                }
            }
        })
        .build()?;

    // Background window: hide instead of closing.
    let w = win.clone();
    win.on_window_event(move |e| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            let _ = w.hide();
        }
    });
    Ok(win)
}

fn create_popover(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("index.html".into()))
        .inner_size(POPOVER_W, POPOVER_H)
        .visible(false)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        // Menu = the material system menu-bar dropdowns use: more blur and
        // more translucency than Popover. Radius must match the CSS card.
        let _ = apply_vibrancy(&win, NSVisualEffectMaterial::Menu, None, Some(12.0));
    }

    let w = win.clone();
    let app_for_blur = app.clone();
    win.on_window_event(move |e| {
        if let tauri::WindowEvent::Focused(false) = e {
            // macOS bounces focus right after a menu-bar-anchored window is
            // shown from an Accessory app — a blur that quickly means "the
            // show itself", not the user clicking away. Ignore it.
            let just_shown = {
                let state = app_for_blur.state::<Mutex<AppState>>();
                let s = state.lock().unwrap();
                s.popover_shown_at
                    .is_some_and(|t| t.elapsed().as_millis() < 400)
            };
            if !just_shown {
                let _ = w.hide();
            }
        }
    });
    Ok(win)
}

fn toggle_popover(app: &AppHandle) {
    let Some(win) = app.get_webview_window("popover") else {
        eprintln!("[popover] window missing");
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    let moved = win.as_ref().window().move_window(Position::TrayBottomCenter);
    if moved.is_err() {
        position_below_tray(app, &win);
    }
    eprintln!("[popover] move: {:?}, pos now: {:?}", moved, win.outer_position());
    let (model, _) = current(app);
    let _ = app.emit_to("popover", "settings", settings_json(app));
    let _ = app.emit_to("popover", "usage", &model);
    let _ = app.emit_to("popover", "reset-view", ());
    {
        let state = app.state::<Mutex<AppState>>();
        state.lock().unwrap().popover_shown_at = Some(std::time::Instant::now());
    }
    let shown = win.show();
    let focused = win.set_focus();
    eprintln!(
        "[popover] show: {:?}, focus: {:?}, visible now: {:?}",
        shown,
        focused,
        win.is_visible()
    );
    spawn_poll(app);
}

// Fallback when the positioner plugin has no tray position: center the window
// under the tray icon rect captured from the click event (physical pixels).
fn position_below_tray(app: &AppHandle, win: &WebviewWindow) {
    let rect = {
        let state = app.state::<Mutex<AppState>>();
        let s = state.lock().unwrap();
        s.tray_rect
    };
    let Some(rect) = rect else { return };
    let sf = win.scale_factor().unwrap_or(1.0);
    let (x, y) = match rect.position {
        tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
        tauri::Position::Logical(p) => (p.x * sf, p.y * sf),
    };
    let (w, h) = match rect.size {
        tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
        tauri::Size::Logical(s) => (s.width * sf, s.height * sf),
    };
    let win_w = win.outer_size().map(|s| s.width as f64).unwrap_or(POPOVER_W * sf);
    let _ = win.set_position(tauri::PhysicalPosition::new(
        (x + w / 2.0 - win_w / 2.0) as i32,
        (y + h + 2.0) as i32,
    ));
}

fn current(app: &AppHandle) -> (Value, bool) {
    let state = app.state::<Mutex<AppState>>();
    let s = state.lock().unwrap();
    let signed_out = s.last_model["signedOut"].as_bool().unwrap_or(true);
    (s.last_model.clone(), signed_out)
}

fn settings_json(app: &AppHandle) -> Value {
    let s = settings::load(app);
    let open_at_login = app.autolaunch().is_enabled().unwrap_or(false);
    json!({ "showNumber": s.show_number, "theme": s.theme, "openAtLogin": open_at_login })
}

// ---------------------------------------------------------------------------
// Commands (invoked from the popover UI)
// ---------------------------------------------------------------------------
#[tauri::command]
fn get_settings(app: AppHandle) -> Value {
    settings_json(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, patch: Value) -> Value {
    if let Some(open) = patch["openAtLogin"].as_bool() {
        let al = app.autolaunch();
        let _ = if open { al.enable() } else { al.disable() };
    }
    let mut s = settings::load(&app);
    if let Some(v) = patch["showNumber"].as_bool() {
        s.show_number = v;
    }
    if let Some(v) = patch["theme"].as_str() {
        s.theme = v.to_string();
    }
    settings::save(&app, &s);
    let (model, _) = current(&app);
    update_tray(&app, &model);
    let out = settings_json(&app);
    let _ = app.emit_to("popover", "settings", &out);
    out
}

#[tauri::command]
fn get_usage(app: AppHandle) -> Value {
    current(&app).0
}

#[tauri::command]
async fn refresh(app: AppHandle) {
    poll(app).await;
}

#[tauri::command]
fn open_claude(app: AppHandle) {
    let win = match app.get_webview_window("session") {
        Some(w) => w,
        None => match create_session_window(&app) {
            Ok(w) => w,
            Err(_) => return,
        },
    };
    let _ = win.show();
    let _ = win.set_focus();
}

#[tauri::command]
fn resize_popover(app: AppHandle, height: f64) {
    let Some(win) = app.get_webview_window("popover") else { return };
    let h = height.clamp(160.0, 600.0);
    let _ = win.set_size(tauri::LogicalSize::new(POPOVER_W, h));
}

async fn tokio_sleep(d: Duration) {
    tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d))
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            get_usage,
            refresh,
            open_claude,
            resize_popover
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            create_popover(&handle)?;
            create_session_window(&handle)?;

            // Tray + menu
            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let open_i = MenuItem::with_id(app, "open", "Open Claude (sign in)…", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_i, &open_i, &PredefinedMenuItem::separator(app)?, &quit_i])?;

            let rgba = ring::draw(None, is_dark(&handle));
            TrayIconBuilder::with_id("main")
                .icon(Image::new_owned(rgba, ring::SIZE, ring::SIZE))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "refresh" => spawn_poll(app),
                    "open" => open_claude(app.clone()),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
                    eprintln!("[tray] event: {:?}", event);
                    let app = tray.app_handle();
                    // Whether macOS delivers Down, Up, or both varies; react to
                    // any left-click and debounce so a Down+Up pair toggles once.
                    if let TrayIconEvent::Click { button: MouseButton::Left, rect, .. } = event {
                        {
                            let state = app.state::<Mutex<AppState>>();
                            let mut s = state.lock().unwrap();
                            s.tray_rect = Some(rect);
                            let now = std::time::Instant::now();
                            if s.last_toggle.is_some_and(|t| now.duration_since(t).as_millis() < 300) {
                                return;
                            }
                            s.last_toggle = Some(now);
                        }
                        toggle_popover(app);
                    }
                })
                .build(app)?;

            // Debug-only: drive the app from stdin so the popover can be
            // exercised without clicking the real tray icon (automation can't).
            #[cfg(debug_assertions)]
            {
                let h = handle.clone();
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                        let h2 = h.clone();
                        match line.trim() {
                            "toggle" => {
                                let _ = h.run_on_main_thread(move || toggle_popover(&h2));
                            }
                            "poll" => spawn_poll(&h),
                            _ => {}
                        }
                    }
                });
            }

            // First poll after the session webview has had a moment to load;
            // show the sign-in window if there's still no data. Then keep polling.
            let app_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio_sleep(Duration::from_millis(2500)).await;
                poll(app_handle.clone()).await;
                let (_, signed_out) = current(&app_handle);
                if signed_out {
                    open_claude(app_handle.clone());
                }
                loop {
                    tokio_sleep(Duration::from_secs(POLL_SECS)).await;
                    poll(app_handle.clone()).await;
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
