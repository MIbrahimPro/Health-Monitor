use crate::pipeline::{downscale_gray_into, rgb_to_gray_into, FaceBox, FrameAnalyzer};
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use image::{codecs::jpeg::JpegEncoder, ImageBuffer, Rgb};
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution},
    Camera,
};
use rustface::ImageData;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Cursor;
use std::io::Write;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

pub struct VitalStats {
    pub raw_pulse: f32,
    pub bpm_10s: Option<f32>,
    pub bpm_30s: Option<f32>,
    pub bpm_60s: Option<f32>,
    /// Respiration rate, breaths per minute.
    pub resp_bpm: Option<f32>,
    /// Signal quality 0–100.
    pub quality: f32,
    /// SNR (dB) of the latest 10 s spectral estimate.
    pub snr_db: f32,
    pub face_found: bool,
    pub frame_base64: Option<String>,
    pub fps: f32,
}

macro_rules! log_msg {
    ($file:expr, $($arg:tt)*) => {
        if let Some(f) = $file.as_mut() {
            let _ = writeln!(f, $($arg)*);
            let _ = f.flush();
        }
    };
}

/// Locate the rustface model binary relative to the current working directory.
pub fn find_face_model() -> Option<String> {
    for candidate in [
        "models/seeta_fd_frontal_v1.0.bin",
        "../models/seeta_fd_frontal_v1.0.bin",
        "../../models/seeta_fd_frontal_v1.0.bin",
    ] {
        if std::fs::metadata(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

pub fn start_camera_loop(sender: mpsc::Sender<VitalStats>, stop: Arc<AtomicBool>) -> Result<()> {
    std::thread::spawn(move || {
        let _ = create_dir_all("../logs");
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let log_path = format!("../logs/aegis_run_{}.log", timestamp);

        let log_path_clone = log_path.clone();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(mut f) = OpenOptions::new().create(true).write(true).append(true).open(&log_path_clone) {
                let _ = writeln!(f, "FATAL THREAD PANIC: {:?}", info);
            }
        }));

        let mut log_file = OpenOptions::new().create(true).write(true).append(true).open(&log_path).ok();
        log_msg!(log_file, "--- AEGIS CAMERA LOOP STARTED ---");

        // --- Camera setup ---
        // Capture at full 640x480: rPPG samples 4x more skin pixels (2x SNR).
        // Face detection runs on a 2x-downscaled copy, so it stays fast.
        let index = CameraIndex::Index(0);
        let format = CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30);
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(format));

        let mut camera = match Camera::new(index, requested) {
            Ok(cam) => cam,
            Err(e) => {
                log_msg!(log_file, "FATAL: Camera init failed: {:?}", e);
                return;
            }
        };
        if let Err(e) = camera.open_stream() {
            log_msg!(log_file, "FATAL: Stream open failed: {:?}", e);
            return;
        }
        log_msg!(log_file, "Camera stream opened.");

        // --- Face detection model ---
        let model_path = match find_face_model() {
            Some(p) => p,
            None => {
                log_msg!(log_file, "FATAL: Could not find rustface model binary.");
                return;
            }
        };
        log_msg!(log_file, "Model found at: {}", model_path);

        // --- Shared face box between threads ---
        let face_rect: Arc<Mutex<Option<FaceBox>>> = Arc::new(Mutex::new(None));
        let face_rect_for_detect = face_rect.clone();

        // Channel to send grayscale frames to the detection thread
        let (detect_tx, detect_rx) = std_mpsc::sync_channel::<(Vec<u8>, u32, u32, u32)>(1);

        // --- Face detection on a separate thread (rustface is slow) ---
        std::thread::spawn(move || {
            let mut detector = match rustface::create_detector(&model_path) {
                Ok(d) => d,
                Err(_) => return,
            };
            detector.set_min_face_size(30);
            detector.set_score_thresh(2.0);

            while let Ok((gray, w, h, factor)) = detect_rx.recv() {
                let mut image_data = ImageData::new(&gray, w, h);
                let faces = detector.detect(&mut image_data);

                // Scale detection coordinates back to full capture resolution.
                let full_w = w * factor;
                let full_h = h * factor;
                let result = faces
                    .into_iter()
                    .max_by_key(|f| f.bbox().width() * f.bbox().height())
                    .map(|face| {
                        let bbox = face.bbox();
                        let x = ((bbox.x().max(0) as u32) * factor).min(full_w.saturating_sub(1));
                        let y = ((bbox.y().max(0) as u32) * factor).min(full_h.saturating_sub(1));
                        let fw = ((bbox.width().max(0) as u32) * factor).min(full_w - x);
                        let fh = ((bbox.height().max(0) as u32) * factor).min(full_h - y);
                        FaceBox { x, y, w: fw, h: fh }
                    });

                if let Ok(mut rect) = face_rect_for_detect.lock() {
                    *rect = result;
                }
            }
        });
        log_msg!(log_file, "Face detection thread spawned.");

        // --- Main capture loop ---
        let mut analyzer = FrameAnalyzer::new();
        let tracking_start = Instant::now();
        let mut frame_counter: u64 = 0;
        let mut fps_counter = 0u32;
        let mut fps_timer = Instant::now();
        let mut current_fps: f32 = 0.0;
        
        let mut gray_full = Vec::new();
        let mut gray_downscaled = Vec::new();
        let mut preview_buf = Vec::new();

        log_msg!(log_file, "Entering main capture loop.");
        loop {
            if stop.load(Ordering::Relaxed) {
                log_msg!(log_file, "Stop requested.");
                break;
            }

            // --- 1. Capture frame ---
            let frame = match camera.frame() {
                Ok(f) => f,
                Err(e) => {
                    if frame_counter % 100 == 0 {
                        log_msg!(log_file, "ERROR: Frame capture: {:?}", e);
                    }
                    continue;
                }
            };
            let decoded = match frame.decode_image::<RgbFormat>() {
                Ok(img) => img,
                Err(e) => {
                    if frame_counter % 100 == 0 {
                        log_msg!(log_file, "ERROR: Frame decode: {:?}", e);
                    }
                    continue;
                }
            };
            let width = decoded.width();
            let height = decoded.height();

            // --- 2. FPS tracking ---
            fps_counter += 1;
            let fps_elapsed = fps_timer.elapsed().as_secs_f32();
            if fps_elapsed >= 1.0 {
                current_fps = fps_counter as f32 / fps_elapsed;
                fps_counter = 0;
                fps_timer = Instant::now();
                log_msg!(log_file, "[F{}] FPS: {:.1}", frame_counter, current_fps);
            }

            // --- 3. Send to face detection thread (non-blocking, every 10th frame) ---
            if frame_counter % 10 == 0 {
                let factor = if width >= 640 { 2 } else { 1 };
                rgb_to_gray_into(decoded.as_raw(), width, height, &mut gray_full);
                let (gw, gh) = downscale_gray_into(&gray_full, width, height, factor, &mut gray_downscaled);
                let _ = detect_tx.try_send((gray_downscaled.clone(), gw, gh, factor));
            }

            // --- 4. Read latest detection + run the shared analysis pipeline ---
            let current_face = face_rect.lock().ok().and_then(|guard| *guard);
            let elapsed = tracking_start.elapsed().as_secs_f64();
            let analysis = analyzer.process_frame(decoded.as_raw(), width, height, current_face, elapsed);

            // --- 5. Encode preview frame (every 5th frame, downscaled 2x) ---
            let mut frame_base64 = None;
            if frame_counter % 5 == 0 {
                let pf = if width >= 640 { 2u32 } else { 1 };
                let (pw, ph) = (width / pf, height / pf);
                let src = decoded.as_raw();
                preview_buf.clear();
                for y in 0..ph {
                    let row = ((y * pf) * width) as usize * 3;
                    for x in 0..pw {
                        let idx = row + (x * pf) as usize * 3;
                        preview_buf.extend_from_slice(&src[idx..idx + 3]);
                    }
                }
                if let Some(mut display_img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(pw, ph, std::mem::take(&mut preview_buf)) {
                    // Draw the smoothed ROI actually being sampled
                    if let Some(roi) = analysis.roi {
                        let x1 = (roi.x / pf).min(pw.saturating_sub(1));
                        let y1 = (roi.y / pf).min(ph.saturating_sub(1));
                        let x2 = ((roi.x + roi.w) / pf).min(pw.saturating_sub(1));
                        let y2 = ((roi.y + roi.h) / pf).min(ph.saturating_sub(1));
                        for t in 0..2u32 {
                            let y1t = y1.saturating_sub(t).min(ph.saturating_sub(1));
                            let y2t = (y2 + t).min(ph.saturating_sub(1));
                            for x in x1..=x2 {
                                display_img.put_pixel(x, y1t, Rgb([0, 255, 0]));
                                display_img.put_pixel(x, y2t, Rgb([0, 255, 0]));
                            }
                            let x1t = x1.saturating_sub(t).min(pw.saturating_sub(1));
                            let x2t = (x2 + t).min(pw.saturating_sub(1));
                            for y in y1..=y2 {
                                display_img.put_pixel(x1t, y, Rgb([0, 255, 0]));
                                display_img.put_pixel(x2t, y, Rgb([0, 255, 0]));
                            }
                        }
                    }

                    let mut buf = Cursor::new(Vec::new());
                    let mut encoder = JpegEncoder::new_with_quality(&mut buf, 55);
                    if encoder.encode_image(&display_img).is_ok() {
                        frame_base64 = Some(general_purpose::STANDARD.encode(buf.into_inner()));
                    }
                    
                    preview_buf = display_img.into_raw();
                }
            }

            // --- 6. ALWAYS send stats ---
            let stats = VitalStats {
                raw_pulse: analysis.raw_pulse,
                bpm_10s: analysis.bpm_10s,
                bpm_30s: analysis.bpm_30s,
                bpm_60s: analysis.bpm_60s,
                resp_bpm: analysis.resp_bpm,
                quality: analysis.quality,
                snr_db: analysis.snr_db,
                face_found: analysis.face_found,
                frame_base64,
                fps: current_fps,
            };
            if sender.blocking_send(stats).is_err() {
                log_msg!(log_file, "Channel closed, exiting.");
                break;
            }

            frame_counter += 1;
        }
        
        drop(detect_tx);
        drop(camera);
        log_msg!(log_file, "Camera released.");
    });

    Ok(())
}
