use aegis_core::camera::start_camera_loop;
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
    face_found: bool,
    frame_base64: Option<String>,
    fps: f32,
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

    if let Err(e) = start_camera_loop(tx, stop_flag) {
        state.running.store(false, Ordering::Relaxed);
        return format!("Failed to start camera loop: {:?}", e);
    }

    tauri::async_runtime::spawn(async move {
        while let Some(stats) = rx.recv().await {
            let _ = app.emit(
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
                },
            );
        }
    });

    "Tracking started".into()
}

#[tauri::command]
fn stop_tracking(state: State<'_, AppState>) -> String {
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
        })
        .invoke_handler(tauri::generate_handler![
            start_tracking, stop_tracking, get_config, set_config,
            overlay_enable, overlay_disable, overlay_set_warmth
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
