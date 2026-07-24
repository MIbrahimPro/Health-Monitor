use aegis_core::camera::start_camera_loop;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

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
fn start_tracking(app: AppHandle) -> String {
    let (tx, mut rx) = mpsc::channel(100);

    if let Err(e) = start_camera_loop(tx) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_tracking])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
