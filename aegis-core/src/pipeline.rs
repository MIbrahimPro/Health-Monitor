//! Shared frame-analysis pipeline.
//!
//! Both the live camera loop (`camera.rs`) and the offline benchmark harness
//! (`bin/bench_rppg.rs`) run this exact code, so accuracy measured on the test
//! video is the accuracy of the production pipeline.

use crate::rppg::PosRppg;

/// Sliding POS window length in frames.
pub const RPPG_WINDOW: usize = 45;

/// A face bounding box in pixel coordinates (full face, eyebrows→chin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Per-frame output of the analyzer.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub raw_pulse: f32,
    pub bpm_10s: Option<f32>,
    pub bpm_30s: Option<f32>,
    pub bpm_60s: Option<f32>,
    pub face_found: bool,
    /// Smoothed ROI actually sampled this frame (upper-face region).
    pub roi: Option<FaceBox>,
    /// Mean skin RGB fed to the rPPG stage this frame.
    pub mean_rgb: Option<(f32, f32, f32)>,
}

/// Stateful per-frame analyzer: EMA face tracking → skin-masked ROI mean → rPPG.
pub struct FrameAnalyzer {
    rppg: PosRppg,
    smoothed_rect: Option<(f32, f32, f32, f32)>,
}

impl Default for FrameAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameAnalyzer {
    pub fn new() -> Self {
        Self {
            rppg: PosRppg::new(RPPG_WINDOW),
            smoothed_rect: None,
        }
    }

    /// Process one RGB frame.
    ///
    /// `face` is the most recent raw detection result (full face box) or `None`
    /// when no face is currently known. Detection cadence is the caller's
    /// concern (async thread live, every Nth frame offline).
    pub fn process_frame(
        &mut self,
        rgb: &[u8],
        width: u32,
        height: u32,
        face: Option<FaceBox>,
        elapsed_secs: f64,
    ) -> AnalysisResult {
        let face_found = face.is_some();

        // The face box from rustface spans eyebrows→chin. The pulse signal lives
        // in the forehead/cheeks, so sample the top 60% (avoids beard + mouth).
        let roi_target = face.map(|f| {
            let roi_h = (f.h as f32 * 0.6).max(2.0);
            (f.x as f32, f.y as f32, f.w as f32, roi_h)
        });

        // EMA glide so detection jitter doesn't yank the sampled region.
        match (roi_target, self.smoothed_rect) {
            (Some((tx, ty, tw, th)), Some((sx, sy, sw, sh))) => {
                self.smoothed_rect = Some((
                    sx * 0.9 + tx * 0.1,
                    sy * 0.9 + ty * 0.1,
                    sw * 0.9 + tw * 0.1,
                    sh * 0.9 + th * 0.1,
                ));
            }
            (Some(t), None) => self.smoothed_rect = Some(t),
            (None, _) => self.smoothed_rect = None,
        }

        let roi = self.smoothed_rect.map(|(x, y, w, h)| FaceBox {
            x: x as u32,
            y: y as u32,
            w: w as u32,
            h: h as u32,
        });

        let mut result = AnalysisResult {
            raw_pulse: 0.0,
            bpm_10s: None,
            bpm_30s: None,
            bpm_60s: None,
            face_found,
            roi,
            mean_rgb: None,
        };

        if let Some(r) = roi {
            if let Some((mean_r, mean_g, mean_b)) = skin_mean_rgb(rgb, width, height, r) {
                let (pulse, b10, b30, b60) =
                    self.rppg
                        .process_frame(mean_r, mean_g, mean_b, elapsed_secs);
                result.raw_pulse = pulse;
                result.bpm_10s = b10;
                result.bpm_30s = b30;
                result.bpm_60s = b60;
                result.mean_rgb = Some((mean_r, mean_g, mean_b));
            }
        }

        result
    }
}

/// Mean RGB over the ROI restricted to skin-classified pixels.
///
/// Skin mask: pixel must not be near-black and red must dominate (rejects
/// white/blue-ish backgrounds, walls and dark hair). Falls back to the plain
/// ROI mean if the mask rejects nearly everything (e.g. heavy shadow).
pub fn skin_mean_rgb(
    rgb: &[u8],
    width: u32,
    height: u32,
    roi: FaceBox,
) -> Option<(f32, f32, f32)> {
    let end_y = (roi.y + roi.h).min(height);
    let end_x = (roi.x + roi.w).min(width);

    let mut sum_r = 0.0_f64;
    let mut sum_g = 0.0_f64;
    let mut sum_b = 0.0_f64;
    let mut count = 0.0_f64;

    let mut fallback_r = 0.0_f64;
    let mut fallback_g = 0.0_f64;
    let mut fallback_b = 0.0_f64;
    let mut fallback_c = 0.0_f64;

    for y in roi.y..end_y {
        let row = (y * width) as usize * 3;
        for x in roi.x..end_x {
            let idx = row + x as usize * 3;
            if idx + 2 < rgb.len() {
                let r = rgb[idx] as f64;
                let g = rgb[idx + 1] as f64;
                let b = rgb[idx + 2] as f64;

                fallback_r += r;
                fallback_g += g;
                fallback_b += b;
                fallback_c += 1.0;

                let max_val = r.max(g).max(b);
                if max_val > 10.0 && r >= g && r >= b {
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
        Some((
            (sum_r / count) as f32,
            (sum_g / count) as f32,
            (sum_b / count) as f32,
        ))
    } else {
        None
    }
}

/// Convert an RGB24 buffer to grayscale (BT.601 luma).
pub fn rgb_to_gray(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut gray = Vec::with_capacity((width * height) as usize);
    for px in rgb.chunks_exact(3) {
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        gray.push((r * 0.299 + g * 0.587 + b * 0.114) as u8);
    }
    gray
}
