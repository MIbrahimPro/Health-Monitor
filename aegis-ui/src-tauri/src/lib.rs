use aegis_core::camera::start_camera_loop;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

struct AppState {
    running: Arc<AtomicBool>,
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            running: Arc::new(AtomicBool::new(false)),
            stop_flag: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![start_tracking, stop_tracking])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
