use crate::rppg::PosRppg;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use image::{codecs::jpeg::JpegEncoder, ImageBuffer, Rgb};
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType, CameraFormat, Resolution, FrameFormat},
    Camera,
};
use rustface::ImageData;
use std::io::Cursor;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use tokio::sync::mpsc;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH, Instant};

pub struct VitalStats {
    pub raw_pulse: f32,
    pub bpm_10s: Option<f32>,
    pub bpm_30s: Option<f32>,
    pub bpm_60s: Option<f32>,
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

/// Downscale grayscale image by factor for faster face detection.
fn downscale_gray(pixels: &[u8], width: u32, height: u32, factor: u32) -> (Vec<u8>, u32, u32) {
    let new_w = width / factor;
    let new_h = height / factor;
    let mut out = Vec::with_capacity((new_w * new_h) as usize);
    for y in 0..new_h {
        for x in 0..new_w {
            let src_idx = ((y * factor) * width + (x * factor)) as usize;
            if src_idx < pixels.len() {
                out.push(pixels[src_idx]);
            } else {
                out.push(0);
            }
        }
    }
    (out, new_w, new_h)
}

/// Convert RGB buffer to grayscale.
fn rgb_to_gray(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut gray = Vec::with_capacity((width * height) as usize);
    for i in (0..rgb.len()).step_by(3) {
        let r = rgb[i] as f32;
        let g = rgb[i + 1] as f32;
        let b = rgb[i + 2] as f32;
        gray.push((r * 0.299 + g * 0.587 + b * 0.114) as u8);
    }
    gray
}

pub fn start_camera_loop(sender: mpsc::Sender<VitalStats>) -> Result<()> {
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
        let index = CameraIndex::Index(0);
        let format = CameraFormat::new(Resolution::new(320, 240), FrameFormat::YUYV, 30);
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

        // --- Face detection model (find the model file) ---
        let model_path = if std::fs::metadata("../../models/seeta_fd_frontal_v1.0.bin").is_ok() {
            "../../models/seeta_fd_frontal_v1.0.bin".to_string()
        } else if std::fs::metadata("../models/seeta_fd_frontal_v1.0.bin").is_ok() {
            "../models/seeta_fd_frontal_v1.0.bin".to_string()
        } else {
            log_msg!(log_file, "FATAL: Could not find rustface model binary.");
            return;
        };
        log_msg!(log_file, "Model found at: {}", model_path);

        // --- Shared face rect between threads ---
        let face_rect: Arc<Mutex<Option<(u32, u32, u32, u32)>>> = Arc::new(Mutex::new(None));
        let face_rect_for_detect = face_rect.clone();

        // Channel to send grayscale frames to the detection thread
        let (detect_tx, detect_rx) = std_mpsc::sync_channel::<(Vec<u8>, u32, u32)>(1);

        // --- Spawn face detection on a SEPARATE thread ---
        std::thread::spawn(move || {
            let mut detector = match rustface::create_detector(&model_path) {
                Ok(d) => d,
                Err(_) => return,
            };
            detector.set_min_face_size(30); // Lowered min face size for 320x240
            detector.set_score_thresh(2.0);

            while let Ok((gray, w, h)) = detect_rx.recv() {
                // For 320x240, don't downscale. Factor = 1
                let (small, sw, sh) = downscale_gray(&gray, w, h, 1);
                let mut image_data = ImageData::new(&small, sw, sh);
                let faces = detector.detect(&mut image_data);

                let result = if let Some(face) = faces.into_iter()
                    .max_by_key(|f| f.bbox().width() * f.bbox().height())
                {
                    let bbox = face.bbox();
                    // Since factor = 1, we don't need to multiply by 2
                    let factor = 1;
                    let x = ((bbox.x().max(0) as u32) * factor).min(w.saturating_sub(1));
                    let y = ((bbox.y().max(0) as u32) * factor).min(h.saturating_sub(1));
                    let fw = ((bbox.width().max(0) as u32) * factor).min(w - x);
                    let fh = ((bbox.height().max(0) as u32) * factor).min(h - y);
                    // Forehead ROI: top 30% of face
                    let roi_h = (fh as f32 * 0.3).max(2.0) as u32;
                    Some((x, y, fw, roi_h))
                } else {
                    None
                };

                if let Ok(mut rect) = face_rect_for_detect.lock() {
                    *rect = result;
                }
            }
        });
        log_msg!(log_file, "Face detection thread spawned.");

        // --- Main capture loop ---
        let mut rppg = PosRppg::new(45);
        let tracking_start = Instant::now();
        let mut frame_counter: u64 = 0;
        let mut fps_counter = 0u32;
        let mut fps_timer = Instant::now();
        let mut current_fps: f32 = 0.0;

        let mut smoothed_face_rect: Option<(f32, f32, f32, f32)> = None;

        log_msg!(log_file, "Entering main capture loop.");
        loop {
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
                let gray = rgb_to_gray(decoded.as_raw(), width, height);
                let _ = detect_tx.try_send((gray, width, height)); // Non-blocking!
            }

            // --- 4. Read face rect (fast mutex read) ---
            let current_face = face_rect.lock().ok().and_then(|guard| *guard);
            let face_found = current_face.is_some();

            if let Some((fx, fy, fw, fh)) = current_face {
                if let Some((sx, sy, sw, sh)) = smoothed_face_rect {
                    smoothed_face_rect = Some((
                        sx * 0.9 + (fx as f32) * 0.1,
                        sy * 0.9 + (fy as f32) * 0.1,
                        sw * 0.9 + (fw as f32) * 0.1,
                        sh * 0.9 + (fh as f32) * 0.1,
                    ));
                } else {
                    smoothed_face_rect = Some((fx as f32, fy as f32, fw as f32, fh as f32));
                }
            } else {
                smoothed_face_rect = None;
            }

            let face_to_use = smoothed_face_rect.map(|(x, y, w, h)| (x as u32, y as u32, w as u32, h as u32));

            // --- 5. rPPG processing ---
            let elapsed = tracking_start.elapsed().as_secs_f64();
            let mut raw_pulse = 0.0;
            let mut bpm_10s = None;
            let mut bpm_30s = None;
            let mut bpm_60s = None;

            if let Some((start_x, start_y, roi_w, roi_h)) = face_to_use {
                let pixels = decoded.as_raw();
                let end_y = (start_y + roi_h).min(height);
                let end_x = (start_x + roi_w).min(width);

                let mut sum_r = 0.0_f64;
                let mut sum_g = 0.0_f64;
                let mut sum_b = 0.0_f64;
                let mut count = 0.0_f64;
                
                // Fallback counters just in case skin mask fails completely
                let mut fallback_r = 0.0;
                let mut fallback_g = 0.0;
                let mut fallback_b = 0.0;
                let mut fallback_c = 0.0;

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let idx = ((y * width + x) * 3) as usize;
                        if idx + 2 < pixels.len() {
                            let r = pixels[idx] as f64;
                            let g = pixels[idx + 1] as f64;
                            let b = pixels[idx + 2] as f64;
                            
                            fallback_r += r;
                            fallback_g += g;
                            fallback_b += b;
                            fallback_c += 1.0;

                            // Advanced Skin Mask (Kovac et al. modified for low light):
                            // 1. R > G > B (Human skin reflects more red)
                            // 2. Reject pure black hair / dark shadows (R > 40, G > 20)
                            // 3. Ensure color isn't purely grayscale background (max - min > 10)
                            // 4. Ensure it's not a yellow/white wall (abs(R - G) > 10)
                            let max_val = r.max(g).max(b);
                            let min_val = r.min(g).min(b);
                            
                            if r > g && r > b 
                               && r > 40.0 && g > 20.0 
                               && (max_val - min_val) > 10.0 
                               && (r - g).abs() > 10.0 
                            {
                                sum_r += r;
                                sum_g += g;
                                sum_b += b;
                                count += 1.0;
                            }
                        }
                    }
                }

                if count < 10.0 {
                    sum_r = fallback_r;
                    sum_g = fallback_g;
                    sum_b = fallback_b;
                    count = fallback_c;
                }

                if count > 0.0 {
                    let mean_r = (sum_r / count) as f32;
                    let mean_g = (sum_g / count) as f32;
                    let mean_b = (sum_b / count) as f32;
                    let (pulse, b10, b30, b60) = rppg.process_frame(mean_r, mean_g, mean_b, elapsed);
                    raw_pulse = pulse;
                    bpm_10s = b10;
                    bpm_30s = b30;
                    bpm_60s = b60;
                }
            }

            // --- 6. Encode video frame (every 5th frame) ---
            let mut frame_base64 = None;
            if frame_counter % 5 == 0 {
                let img_vec = decoded.as_raw().to_vec();
                if let Some(mut display_img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(width, height, img_vec) {
                    // Draw face box
                    if let Some((fx, fy, fw, fh)) = current_face {
                        let x1 = fx.min(width.saturating_sub(1));
                        let y1 = fy.min(height.saturating_sub(1));
                        let x2 = (fx + fw).min(width.saturating_sub(1));
                        let y2 = (fy + fh).min(height.saturating_sub(1));
                        for t in 0..2u32 {
                            let y1t = y1.saturating_sub(t).min(height.saturating_sub(1));
                            let y2t = (y2 + t).min(height.saturating_sub(1));
                            for x in x1..=x2 {
                                display_img.put_pixel(x, y1t, Rgb([0, 255, 0]));
                                display_img.put_pixel(x, y2t, Rgb([0, 255, 0]));
                            }
                            let x1t = x1.saturating_sub(t).min(width.saturating_sub(1));
                            let x2t = (x2 + t).min(width.saturating_sub(1));
                            for y in y1..=y2 {
                                display_img.put_pixel(x1t, y, Rgb([0, 255, 0]));
                                display_img.put_pixel(x2t, y, Rgb([0, 255, 0]));
                            }
                        }
                    }

                    let mut buf = Cursor::new(Vec::new());
                    let mut encoder = JpegEncoder::new_with_quality(&mut buf, 40);
                    if encoder.encode_image(&display_img).is_ok() {
                        frame_base64 = Some(general_purpose::STANDARD.encode(buf.into_inner()));
                    }
                }
            }

            // --- 7. ALWAYS send stats ---
            let stats = VitalStats {
                raw_pulse,
                bpm_10s,
                bpm_30s,
                bpm_60s,
                face_found,
                frame_base64,
                fps: current_fps,
            };
            if sender.blocking_send(stats).is_err() {
                log_msg!(log_file, "Channel closed, exiting.");
                break;
            }

            frame_counter += 1;
        }
    });

    Ok(())
}
