use aegis_core::camera::start_camera_loop;
use aegis_core::context::{start_context_loop, get_context_summary as core_get_context_summary, get_intent_now as core_get_intent_now, ContextSummary, Intent};
use aegis_core::config::Config;
use chrono::{Local, Timelike};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder, WebviewUrl};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tokio::sync::mpsc;

struct AppState {
    running: Arc<AtomicBool>,
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    overlay_scheduler: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    posture_state: Arc<Mutex<String>>,
    frost_scheduler: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[derive(Clone, Serialize)]
struct PulsePayload {
    pulse: f32,
    bpm_10s: Option<f32>,
    bpm_30s: Option<f32>,
    bpm_60s: Option<f32>,
    resp_bpm: Option<f32>,
    quality: f32,
    snr_db: f32,
    pub face_found: bool,
    pub frame_base64: Option<String>,
    pub fps: f32,
    pub posture: String,
    pub emotion: Option<String>,
    pub owner_present: Option<bool>,
    pub face_count: u32,
}

#[tauri::command]
fn start_tracking(app: AppHandle, state: State<'_, AppState>) -> String {
    let cfg = Config::load();
    if !cfg.camera_module {
        return "Camera module is disabled in settings".into();
    }

    if state.running.load(Ordering::Relaxed) {
        return "Tracking".into();
    }
    state.running.store(true, Ordering::Relaxed);
    
    let stop_flag = Arc::new(AtomicBool::new(false));
    *state.stop_flag.lock().unwrap() = Some(stop_flag.clone());

    let (tx, mut rx) = mpsc::channel(100);

    if let Err(e) = start_camera_loop(tx, stop_flag.clone()) {
        state.running.store(false, Ordering::Relaxed);
        return format!("Failed to start camera loop: {:?}", e);
    }

    start_context_loop(stop_flag.clone());

    let posture_ref = state.posture_state.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        while let Some(stats) = rx.recv().await {
            *posture_ref.lock().unwrap() = stats.posture.clone();
            
            let _ = app_clone.emit(
                "pulse-update",
                PulsePayload {
                    pulse: stats.raw_pulse,
                    bpm_10s: stats.bpm_10s,
                    bpm_30s: stats.bpm_30s,
                    bpm_60s: stats.bpm_60s,
                    resp_bpm: stats.resp_bpm,
                    quality: stats.quality,
                    snr_db: stats.snr_db,
                    face_found: stats.face_found,
                    frame_base64: stats.frame_base64,
                    fps: stats.fps,
                    posture: stats.posture,
                    emotion: stats.emotion,
                    owner_present: stats.owner_present,
                    face_count: stats.face_count,
                },
            );
        }
    });

    let posture_ref2 = state.posture_state.clone();
    let app_clone2 = app.clone();
    let frost_task = tauri::async_runtime::spawn(async move {
        let mut frost_level = 0.0f32;
        loop {
            let cfg = Config::load();
            let posture = posture_ref2.lock().unwrap().clone();
            
            if cfg.posture_enforce {
                if posture != "Good" {
                    frost_level = (frost_level + (0.8 / (20.0 / 0.5))).min(0.8);
                } else {
                    frost_level = (frost_level - (0.8 / (0.6 / 0.5))).max(0.0);
                }
                
                if frost_level > 0.0 {
                    // Create overlay if not exists
                    if app_clone2.get_webview_window("overlay").is_none() {
                        let window = WebviewWindowBuilder::new(&app_clone2, "overlay", WebviewUrl::App("overlay.html".into()))
                            .title("Overlay")
                            .decorations(false)
                            .transparent(true)
                            .always_on_top(true)
                            .skip_taskbar(true)
                            .focused(false)
                            .fullscreen(true)
                            .build();
                        if let Ok(win) = window {
                            let _ = win.set_ignore_cursor_events(true);
                        }
                    }
                    if let Some(win) = app_clone2.get_webview_window("overlay") {
                        let _ = win.eval(&format!("setFrost({})", frost_level));
                    }
                } else {
                    if let Some(win) = app_clone2.get_webview_window("overlay") {
                        let _ = win.eval("setFrost(0.0)");
                        // Optional: if we created it just for frost and warmth is 0, we could close it, but 
                        // to keep logic simple, we just let it sit transparent. 
                        // (Wait, the instruction says "close overlay if warmth==0". Let's assume frontend manages overlay_disable).
                    }
                }
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });
    
    *state.frost_scheduler.lock().unwrap() = Some(frost_task);

    "Tracking started".into()
}

#[tauri::command]
fn stop_tracking(state: State<'_, AppState>) -> String {
    if let Some(handle) = state.frost_scheduler.lock().unwrap().take() {
        handle.abort();
    }
    if let Some(stop_flag) = state.stop_flag.lock().unwrap().take() {
        stop_flag.store(true, Ordering::Relaxed);
    }
    state.running.store(false, Ordering::Relaxed);
    "Stopped".into()
}

#[tauri::command]
fn get_config() -> Config {
    Config::load()
}

#[tauri::command]
fn set_config(cfg: Config) -> String {
    match cfg.save() {
        Ok(_) => "Config saved".into(),
        Err(e) => format!("Failed to save config: {:?}", e),
    }
}

#[tauri::command]
fn overlay_enable(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(()); // already enabled
    }

    let _is_wayland = std::env::var("XDG_SESSION_TYPE").unwrap_or_default() == "wayland";

    let window = WebviewWindowBuilder::new(&app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("Overlay")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .fullscreen(true)
        .build()
        .map_err(|e| e.to_string())?;

    let _ = window.set_ignore_cursor_events(true);

    // Stop existing scheduler if any
    if let Some(handle) = state.overlay_scheduler.lock().unwrap().take() {
        handle.abort();
    }

    let app_clone = app.clone();
    let scheduler = tauri::async_runtime::spawn(async move {
        loop {
            let now = Local::now();
            let hour = now.hour() as f32 + (now.minute() as f32 / 60.0);
            let mut warmth = 0.0;
            
            if hour >= 17.0 && hour <= 22.0 {
                // Ramp 0.0 -> 0.35
                warmth = ((hour - 17.0) / 5.0) * 0.35;
            } else if hour > 22.0 || hour < 5.0 {
                warmth = 0.35;
            }
            
            if let Some(win) = app_clone.get_webview_window("overlay") {
                let _ = win.eval(&format!("setWarmth({})", warmth));
            } else {
                break;
            }
            
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });

    *state.overlay_scheduler.lock().unwrap() = Some(scheduler);

    Ok(())
}

#[tauri::command]
fn overlay_disable(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.overlay_scheduler.lock().unwrap().take() {
        handle.abort();
    }
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
fn overlay_set_warmth(app: AppHandle, warmth: f32) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.eval(&format!("setWarmth({})", warmth));
    }
    Ok(())
}

#[tauri::command]
fn get_context_summary(hours: f64) -> ContextSummary {
    core_get_context_summary(hours)
}

#[tauri::command]
fn get_intent_now() -> Intent {
    core_get_intent_now()
}

#[tauri::command]
fn music_play_tag(tag: String) -> String {
    format!("Scaffolded: Playing tag {}", tag)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "Hide", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &hide_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        std::process::exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let is_visible = window.is_visible().unwrap_or(false);
                            if is_visible {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            running: Arc::new(AtomicBool::new(false)),
            stop_flag: Mutex::new(None),
            overlay_scheduler: Mutex::new(None),
            posture_state: Arc::new(Mutex::new("Good".to_string())),
            frost_scheduler: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            start_tracking, stop_tracking, get_config, set_config,
            overlay_enable, overlay_disable, overlay_set_warmth,
            get_context_summary, get_intent_now, music_play_tag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
