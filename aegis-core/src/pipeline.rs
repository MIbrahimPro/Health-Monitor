//! Shared frame-analysis pipeline.
//!
//! Both the live camera loop (`camera.rs`) and the offline benchmark harness
//! (`bin/bench_rppg.rs`) run this exact code, so accuracy measured on the test
//! video is the accuracy of the production pipeline.

use crate::rppg::PosRppg;

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
    /// Respiration rate, breaths per minute.
    pub resp_bpm: Option<f32>,
    /// Signal quality 0–100.
    pub quality: f32,
    /// SNR (dB) of the latest 10 s spectral estimate.
    pub snr_db: f32,
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
            rppg: PosRppg::new(),
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
            resp_bpm: None,
            quality: 0.0,
            snr_db: 0.0,
            face_found,
            roi,
            mean_rgb: None,
        };

        if let Some(r) = roi {
            let (patches, overall) = skin_patch_grid(rgb, width, height, r, GRID_X, GRID_Y);
            if overall.is_some() {
                let out = self.rppg.process_patches(&patches, elapsed_secs);
                result.raw_pulse = out.pulse;
                result.bpm_10s = out.bpm_10s;
                result.bpm_30s = out.bpm_30s;
                result.bpm_60s = out.bpm_60s;
                result.resp_bpm = out.resp_bpm;
                result.quality = out.quality;
                result.snr_db = out.snr_db;
                result.mean_rgb = overall;
            }
        }

        result
    }
}

/// Skin-patch grid dimensions used by the analyzer (patches fused by SNR in
/// the rPPG engine — the pulse is coherent across patches, noise is not).
pub const GRID_X: u32 = 3;
pub const GRID_Y: u32 = 3;
/// Minimum skin pixels for a patch to be considered valid.
const MIN_PATCH_PIXELS: u32 = 25;

/// Sample per-patch mean skin RGB over a `gx` x `gy` grid across the ROI.
///
/// Skin mask: pixel must not be near-black and red must dominate (rejects
/// white/blue-ish backgrounds, walls and dark hair). If the mask starves the
/// whole ROI (heavy shadow), falls back to unmasked per-patch means.
/// Returns (patches, overall pixel-weighted mean).
pub fn skin_patch_grid(
    rgb: &[u8],
    width: u32,
    height: u32,
    roi: FaceBox,
    gx: u32,
    gy: u32,
) -> (Vec<Option<(f32, f32, f32)>>, Option<(f32, f32, f32)>) {
    let n_patches = (gx * gy) as usize;
    let end_y = (roi.y + roi.h).min(height);
    let end_x = (roi.x + roi.w).min(width);
    if roi.w == 0 || roi.h == 0 || roi.x >= end_x || roi.y >= end_y {
        return (vec![None; n_patches], None);
    }

    // Per-patch accumulators: [masked r,g,b,count, unmasked r,g,b,count]
    let mut acc = vec![[0.0f64; 8]; n_patches];

    for y in roi.y..end_y {
        let row = (y * width) as usize * 3;
        let py = ((y - roi.y) * gy / roi.h).min(gy - 1);
        for x in roi.x..end_x {
            let idx = row + x as usize * 3;
            if idx + 2 >= rgb.len() { continue; }
            let r = rgb[idx] as f64;
            let g = rgb[idx + 1] as f64;
            let b = rgb[idx + 2] as f64;
            let px = ((x - roi.x) * gx / roi.w).min(gx - 1);
            let a = &mut acc[(py * gx + px) as usize];

            a[4] += r; a[5] += g; a[6] += b; a[7] += 1.0;
            let max_val = r.max(g).max(b);
            if max_val > 10.0 && r >= g && r >= b {
                a[0] += r; a[1] += g; a[2] += b; a[3] += 1.0;
            }
        }
    }

    let total_masked: f64 = acc.iter().map(|a| a[3]).sum();
    let use_fallback = total_masked < 10.0;

    let mut patches = Vec::with_capacity(n_patches);
    let (mut sr, mut sg, mut sb, mut sc) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for a in &acc {
        let (r, g, b, c) = if use_fallback {
            (a[4], a[5], a[6], a[7])
        } else {
            (a[0], a[1], a[2], a[3])
        };
        if c >= MIN_PATCH_PIXELS as f64 {
            patches.push(Some(((r / c) as f32, (g / c) as f32, (b / c) as f32)));
            sr += r; sg += g; sb += b; sc += c;
        } else {
            patches.push(None);
        }
    }
    let overall = if sc > 0.0 {
        Some(((sr / sc) as f32, (sg / sc) as f32, (sb / sc) as f32))
    } else {
        None
    };
    (patches, overall)
}

/// Downscale a grayscale image by an integer factor (nearest-neighbor).
pub fn downscale_gray(pixels: &[u8], width: u32, height: u32, factor: u32) -> (Vec<u8>, u32, u32) {
    let new_w = width / factor;
    let new_h = height / factor;
    let mut out = Vec::with_capacity((new_w * new_h) as usize);
    for y in 0..new_h {
        for x in 0..new_w {
            let src_idx = ((y * factor) * width + (x * factor)) as usize;
            out.push(*pixels.get(src_idx).unwrap_or(&0));
        }
    }
    (out, new_w, new_h)
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
