//! rPPG engine: multi-patch POS (Wang et al. 2016) with incremental
//! overlap-add windowing, Butterworth bandpass conditioning, SNR-weighted
//! spectral fusion across skin patches, Welch spectral estimation,
//! harmonic-aware peak selection and an SNR-gated heart-rate tracker.
//! Also estimates respiration from low-frequency ROI luminance and uses it to
//! reject breathing-motion harmonics in the heart-rate band.
//!
//! Timestamps are wall-clock seconds; the engine never assumes a frame rate.

use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;

/// POS temporal-normalization window (seconds). 1.6 s per Wang et al.
const POS_WINDOW_SECS: f64 = 1.6;
/// Heart-rate search band (Hz): 42–180 BPM.
const BAND_LO: f64 = 0.7;
const BAND_HI: f64 = 3.0;
/// Respiration search band (Hz): ~8–33 breaths/min.
const RESP_LO: f64 = 0.13;
const RESP_HI: f64 = 0.55;
/// Uniform resample rate for spectral analysis (Hz).
const RESAMPLE_HZ: f64 = 20.0;
/// Resample rate for the (slow) respiration signal (Hz).
const RESP_RESAMPLE_HZ: f64 = 4.0;
/// How often the spectral estimators run (seconds).
const ESTIMATE_PERIOD: f64 = 0.5;
/// History retention (seconds) — must cover the longest window.
const RETAIN_SECS: f64 = 75.0;
/// A gap in face samples longer than this resets the signal.
const MAX_GAP_SECS: f64 = 1.5;

/// Full per-frame output of the engine.
#[derive(Debug, Clone, Default)]
pub struct RppgOutput {
    /// Bandpassed pulse waveform sample for display (delayed ~1.6 s).
    pub pulse: f32,
    pub bpm_10s: Option<f32>,
    pub bpm_30s: Option<f32>,
    pub bpm_60s: Option<f32>,
    /// Respiration rate, breaths per minute.
    pub resp_bpm: Option<f32>,
    /// Signal quality 0–100 (best window confidence).
    pub quality: f32,
    /// SNR (dB) of the most recent 10 s spectral estimate.
    pub snr_db: f32,
}

/// 2nd-order IIR section, Direct Form I.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64, b1: f64, b2: f64, a1: f64, a2: f64,
    x1: f64, x2: f64, y1: f64, y2: f64,
}

impl Biquad {
    fn lowpass(fs: f64, fc: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * fc / fs;
        let (sw, cw) = (w.sin(), w.cos());
        let alpha = sw / std::f64::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cw) / 2.0) / a0, b1: (1.0 - cw) / a0, b2: ((1.0 - cw) / 2.0) / a0,
            a1: (-2.0 * cw) / a0, a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }
    fn highpass(fs: f64, fc: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * fc / fs;
        let (sw, cw) = (w.sin(), w.cos());
        let alpha = sw / std::f64::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cw) / 2.0) / a0, b1: (-(1.0 + cw)) / a0, b2: ((1.0 + cw) / 2.0) / a0,
            a1: (-2.0 * cw) / a0, a2: (1.0 - alpha) / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }
    #[inline]
    fn step(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x;
        self.y2 = self.y1; self.y1 = y;
        y
    }
    fn reset(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0; self.y1 = 0.0; self.y2 = 0.0;
    }
}

/// Bandpass a slice with reflect-padding to suppress the filter's edge transient.
fn bandpass(signal: &[f64], fs: f64) -> Vec<f64> {
    let pad = ((3.0 * fs) as usize).min(signal.len().saturating_sub(1));
    let mut hp = Biquad::highpass(fs, BAND_LO);
    let mut lp = Biquad::lowpass(fs, BAND_HI);
    for i in (1..=pad).rev() {
        let v = 2.0 * signal[0] - signal[i];
        lp.step(hp.step(v));
    }
    signal.iter().map(|&x| lp.step(hp.step(x))).collect()
}

/// Welch PSD (Hann, 50% overlap, 4x zero-padding). Returns (df_hz, psd).
fn welch_psd(signal: &[f64], fs: f64, seg_secs: f64, planner: &mut FftPlanner<f64>) -> (f64, Vec<f64>) {
    let seg_len = ((seg_secs * fs) as usize).min(signal.len()).max(16);
    let hop = (seg_len / 2).max(1);
    let nfft = (seg_len * 4).next_power_of_two();
    let fft = planner.plan_fft_forward(nfft);

    let hann: Vec<f64> = (0..seg_len)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (seg_len - 1) as f64).cos())
        .collect();

    let mut psd = vec![0.0f64; nfft / 2];
    let mut segments = 0usize;
    let mut start = 0usize;
    let mut buf: Vec<Complex<f64>> = vec![Complex::new(0.0, 0.0); nfft];
    while start + seg_len <= signal.len() {
        let seg = &signal[start..start + seg_len];
        let mean: f64 = seg.iter().sum::<f64>() / seg_len as f64;
        for v in buf.iter_mut() { *v = Complex::new(0.0, 0.0); }
        for i in 0..seg_len {
            buf[i] = Complex::new((seg[i] - mean) * hann[i], 0.0);
        }
        fft.process(&mut buf);
        for i in 0..nfft / 2 {
            psd[i] += buf[i].norm_sqr();
        }
        segments += 1;
        start += hop;
    }
    if segments > 0 {
        for v in psd.iter_mut() { *v /= segments as f64; }
    }
    (fs / nfft as f64, psd)
}

/// de Haan SNR: power near f0 + its 2nd harmonic vs the rest of the band.
fn snr_db_at(df: f64, psd: &[f64], f0: f64) -> f64 {
    let lo = (BAND_LO / df).ceil() as usize;
    let hi = ((BAND_HI / df).floor() as usize).min(psd.len().saturating_sub(1));
    let mut sig = 0.0;
    let mut noise = 0.0;
    for i in lo..=hi {
        let f = i as f64 * df;
        if (f - f0).abs() <= 0.1 || (f - 2.0 * f0).abs() <= 0.2 {
            sig += psd[i];
        } else {
            noise += psd[i];
        }
    }
    if noise <= 0.0 { 40.0 } else { 10.0 * (sig / noise).log10() }
}

fn sigmoid_conf(snr_db: f64) -> f64 {
    1.0 / (1.0 + (-(snr_db + 2.0) / 1.5).exp())
}

/// One BPM estimator over a trailing time window.
struct WindowEstimator {
    tracked_bpm: Option<f64>,
    confidence: f64,
}

impl WindowEstimator {
    fn new() -> Self {
        Self { tracked_bpm: None, confidence: 0.0 }
    }

    fn reset(&mut self) {
        self.tracked_bpm = None;
        self.confidence = 0.0;
    }

    /// Warm-start from a shorter window that has already locked, so the longer
    /// (higher-resolution but slower-to-fill) window doesn't sit blank while a
    /// confident estimate already exists.
    fn seed_from(&mut self, bpm: f64, conf: f64) {
        if self.tracked_bpm.is_none() && conf >= 0.3 {
            self.tracked_bpm = Some(bpm);
            self.confidence = conf * 0.6;
        }
    }

    /// Select a peak from a (fused) PSD and update the tracker.
    /// `resp_hz` (when known) penalizes candidates sitting on breathing-motion
    /// harmonics — the classic false lock (e.g. 16 br/min × 3 = 48 "BPM").
    /// Returns (bpm, snr_db) of the measurement when available.
    fn update_from_psd(&mut self, df: f64, psd: &[f64], resp_hz: Option<f64>) -> Option<(f64, f64)> {
        let lo = (BAND_LO / df).ceil() as usize;
        let hi = ((BAND_HI / df).floor() as usize).min(psd.len().saturating_sub(2));
        let mut cands: Vec<(f64, f64)> = Vec::new();
        for i in lo.max(1)..=hi {
            if psd[i] > psd[i - 1] && psd[i] >= psd[i + 1] {
                cands.push((i as f64 * df, psd[i]));
            }
        }
        if cands.is_empty() { return None; }
        let pmax = cands.iter().map(|c| c.1).fold(0.0f64, f64::max);
        let p_at = |f: f64| -> f64 {
            let i = (f / df).round() as usize;
            let a = i.saturating_sub(2);
            let b = (i + 2).min(psd.len() - 1);
            psd[a..=b].iter().cloned().fold(0.0, f64::max)
        };

        // Score: power, plus evidence from the 2nd harmonic (true pulse has one),
        // minus a penalty for sitting on a breathing harmonic, plus temporal
        // continuity once the tracker is locked.
        let mut best: Option<(f64, f64)> = None; // (score, freq)
        for &(f, p) in &cands {
            let harmonic = if 2.0 * f <= BAND_HI + 0.5 { 0.35 * p_at(2.0 * f) / pmax } else { 0.0 };
            let breath_penalty = match resp_hz {
                Some(fr) if fr > 0.05 => {
                    let k = (f / fr).round();
                    if k >= 2.0 && (f - k * fr).abs() < 0.05 { 0.5 } else { 1.0 }
                }
                _ => 1.0,
            };
            let prior = match self.tracked_bpm {
                Some(bpm) if self.confidence > 0.3 => {
                    let d = f - bpm / 60.0;
                    0.3 + 0.7 * (-0.5 * (d / 0.25) * (d / 0.25)).exp()
                }
                _ => 1.0,
            };
            let score = (p / pmax) * (1.0 + harmonic) * breath_penalty * prior;
            if best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, f));
            }
        }
        let (_, f_sel) = best?;
        let snr = snr_db_at(df, psd, f_sel);
        let meas_bpm = f_sel * 60.0;
        let meas_conf = sigmoid_conf(snr);

        match self.tracked_bpm {
            None => {
                if meas_conf >= 0.3 {
                    self.tracked_bpm = Some(meas_bpm);
                    self.confidence = meas_conf;
                }
            }
            Some(cur) => {
                if meas_conf >= 0.2 {
                    // Confidence-weighted approach with a slew limit (max ±4 BPM per tick).
                    let alpha = 0.10 + 0.40 * meas_conf;
                    let delta = (meas_bpm - cur).clamp(-4.0, 4.0);
                    self.tracked_bpm = Some(cur + delta * alpha);
                    self.confidence = 0.9 * self.confidence + 0.1 * meas_conf;
                } else {
                    self.confidence *= 0.99; // hold, decay slowly
                }
            }
        }
        Some((meas_bpm, snr))
    }
}

/// Per-patch color/pulse history (timestamps shared engine-wide).
struct PatchTrace {
    r: VecDeque<f32>,
    g: VecDeque<f32>,
    b: VecDeque<f32>,
    pulse: VecDeque<f32>,
}

impl PatchTrace {
    fn new() -> Self {
        Self {
            r: VecDeque::with_capacity(2048),
            g: VecDeque::with_capacity(2048),
            b: VecDeque::with_capacity(2048),
            pulse: VecDeque::with_capacity(2048),
        }
    }
}

/// The streaming rPPG engine.
pub struct PosRppg {
    times: VecDeque<f64>,
    patches: Vec<PatchTrace>,
    /// Overall ROI luminance — carries breathing body-motion.
    ch_lum: VecDeque<f32>,
    /// Display filter (persistent biquads over fused, fully-accumulated pulse).
    disp_hp: Biquad,
    disp_lp: Biquad,
    disp_fed: usize,
    last_pulse_out: f32,
    est_10: WindowEstimator,
    est_30: WindowEstimator,
    est_60: WindowEstimator,
    last_estimate_t: f64,
    last_snr_db: f32,
    resp_hz: Option<f64>,
    resp_conf: f64,
    planner: FftPlanner<f64>,
}

impl Default for PosRppg {
    fn default() -> Self { Self::new() }
}

impl PosRppg {
    pub fn new() -> Self {
        Self {
            times: VecDeque::with_capacity(2048),
            patches: Vec::new(),
            ch_lum: VecDeque::with_capacity(2048),
            disp_hp: Biquad::highpass(16.6, BAND_LO),
            disp_lp: Biquad::lowpass(16.6, BAND_HI),
            disp_fed: 0,
            last_pulse_out: 0.0,
            est_10: WindowEstimator::new(),
            est_30: WindowEstimator::new(),
            est_60: WindowEstimator::new(),
            last_estimate_t: f64::NEG_INFINITY,
            last_snr_db: 0.0,
            resp_hz: None,
            resp_conf: 0.0,
            planner: FftPlanner::new(),
        }
    }

    /// Full engine reset (face lost too long / module restarted).
    fn reset_signal(&mut self) {
        self.times.clear();
        self.patches.clear();
        self.ch_lum.clear();
        self.resp_hz = None;
        self.resp_conf = 0.0;
        self.disp_hp.reset();
        self.disp_lp.reset();
        self.disp_fed = 0;
        self.last_pulse_out = 0.0;
        self.est_10.reset();
        self.est_30.reset();
        self.est_60.reset();
    }

    /// Single-signal convenience entry (used by tests and simple callers).
    pub fn process_frame(&mut self, r: f32, g: f32, b: f32, elapsed_secs: f64) -> RppgOutput {
        self.process_patches(&[Some((r, g, b))], elapsed_secs)
    }

    /// Process one frame of per-patch mean skin RGB samples.
    ///
    /// Patch layout must stay constant between frames; a change in patch count
    /// resets the engine. `None` marks a patch with no valid skin this frame
    /// (its trace holds the last value to preserve alignment).
    pub fn process_patches(
        &mut self,
        patch_rgb: &[Option<(f32, f32, f32)>],
        elapsed_secs: f64,
    ) -> RppgOutput {
        if let Some(&last_t) = self.times.back() {
            if elapsed_secs - last_t > MAX_GAP_SECS || elapsed_secs < last_t {
                self.reset_signal();
            }
        }
        if self.patches.len() != patch_rgb.len() {
            self.reset_signal();
            self.patches = (0..patch_rgb.len()).map(|_| PatchTrace::new()).collect();
        }

        // Overall mean over valid patches (for luminance / hold-fill).
        let mut acc = (0.0f32, 0.0f32, 0.0f32, 0u32);
        for p in patch_rgb.iter().flatten() {
            acc.0 += p.0; acc.1 += p.1; acc.2 += p.2; acc.3 += 1;
        }
        if acc.3 == 0 {
            // No skin at all this frame: treat as a gap (no sample pushed).
            return self.current_output();
        }
        let overall = (acc.0 / acc.3 as f32, acc.1 / acc.3 as f32, acc.2 / acc.3 as f32);

        self.times.push_back(elapsed_secs);
        self.ch_lum
            .push_back(0.299 * overall.0 + 0.587 * overall.1 + 0.114 * overall.2);
        for (patch, sample) in self.patches.iter_mut().zip(patch_rgb) {
            let (r, g, b) = match sample {
                Some(v) => *v,
                None => (
                    // Hold last value to preserve alignment without transients.
                    patch.r.back().copied().unwrap_or(overall.0),
                    patch.g.back().copied().unwrap_or(overall.1),
                    patch.b.back().copied().unwrap_or(overall.2),
                ),
            };
            patch.r.push_back(r);
            patch.g.push_back(g);
            patch.b.push_back(b);
            patch.pulse.push_back(0.0);
        }

        // Trim history.
        while let (Some(&t0), true) = (self.times.front(), self.times.len() > 4) {
            if elapsed_secs - t0 > RETAIN_SECS {
                self.times.pop_front();
                self.ch_lum.pop_front();
                for patch in self.patches.iter_mut() {
                    patch.r.pop_front();
                    patch.g.pop_front();
                    patch.b.pop_front();
                    patch.pulse.pop_front();
                }
                self.disp_fed = self.disp_fed.saturating_sub(1);
            } else {
                break;
            }
        }

        // --- Incremental POS overlap-add over the trailing 1.6 s window ---
        let n = self.times.len();
        let mut w_start = n - 1;
        while w_start > 0 && elapsed_secs - self.times[w_start - 1] <= POS_WINDOW_SECS {
            w_start -= 1;
        }
        let l = n - w_start;
        if l >= 8 {
            let mut s1 = vec![0.0f64; l];
            let mut s2 = vec![0.0f64; l];
            for patch in self.patches.iter_mut() {
                let (mut mr, mut mg, mut mb) = (0.0f64, 0.0f64, 0.0f64);
                for i in w_start..n {
                    mr += patch.r[i] as f64;
                    mg += patch.g[i] as f64;
                    mb += patch.b[i] as f64;
                }
                mr /= l as f64; mg /= l as f64; mb /= l as f64;
                if mr <= 1e-6 || mg <= 1e-6 || mb <= 1e-6 { continue; }

                for (k, i) in (w_start..n).enumerate() {
                    let rn = patch.r[i] as f64 / mr;
                    let gn = patch.g[i] as f64 / mg;
                    let bn = patch.b[i] as f64 / mb;
                    s1[k] = gn - bn;
                    s2[k] = -2.0 * rn + gn + bn;
                }
                let m1: f64 = s1.iter().sum::<f64>() / l as f64;
                let m2: f64 = s2.iter().sum::<f64>() / l as f64;
                let v1: f64 = s1.iter().map(|x| (x - m1) * (x - m1)).sum::<f64>() / l as f64;
                let v2: f64 = s2.iter().map(|x| (x - m2) * (x - m2)).sum::<f64>() / l as f64;
                let alpha = if v2 > 1e-18 { (v1 / v2).sqrt() } else { 0.0 };
                let mut mean_h = 0.0f64;
                for k in 0..l {
                    // reuse s1 as h
                    s1[k] += alpha * s2[k];
                    mean_h += s1[k];
                }
                mean_h /= l as f64;
                for (k, i) in (w_start..n).enumerate() {
                    patch.pulse[i] += (s1[k] - mean_h) as f32;
                }
            }
        }

        // --- Display waveform: fused (mean over patches) fully-accumulated
        // samples through persistent bandpass biquads ---
        let mut fully_done = n;
        while fully_done > 0 && elapsed_secs - self.times[fully_done - 1] <= POS_WINDOW_SECS {
            fully_done -= 1;
        }
        let n_patches = self.patches.len().max(1) as f64;
        while self.disp_fed < fully_done {
            let fused: f64 = self
                .patches
                .iter()
                .map(|p| p.pulse[self.disp_fed] as f64)
                .sum::<f64>()
                / n_patches;
            self.last_pulse_out = self.disp_lp.step(self.disp_hp.step(fused)) as f32;
            self.disp_fed += 1;
        }

        // --- Spectral estimation at a fixed cadence ---
        if elapsed_secs - self.last_estimate_t >= ESTIMATE_PERIOD {
            self.last_estimate_t = elapsed_secs;
            self.run_estimators(elapsed_secs);
        }

        self.current_output()
    }

    fn current_output(&self) -> RppgOutput {
        RppgOutput {
            pulse: self.last_pulse_out,
            bpm_10s: self.est_10.tracked_bpm.map(|v| v as f32),
            bpm_30s: self.est_30.tracked_bpm.map(|v| v as f32),
            bpm_60s: self.est_60.tracked_bpm.map(|v| v as f32),
            resp_bpm: if self.resp_conf > 0.2 {
                self.resp_hz.map(|f| (f * 60.0) as f32)
            } else {
                None
            },
            quality: (self
                .est_10
                .confidence
                .max(self.est_30.confidence)
                .max(self.est_60.confidence)
                * 100.0) as f32,
            snr_db: self.last_snr_db,
        }
    }

    /// Linear resample of one patch's pulse over [t_start, now] at `hz`.
    fn resample_pulse(&self, patch: usize, i0: usize, hz: f64) -> Vec<f64> {
        let n = self.times.len();
        let t0 = self.times[i0];
        let t1 = self.times[n - 1];
        let m = ((t1 - t0) * hz) as usize;
        let mut out = Vec::with_capacity(m);
        let pulse = &self.patches[patch].pulse;
        let mut idx = i0;
        for k in 0..m {
            let t = t0 + k as f64 / hz;
            while idx + 1 < n - 1 && self.times[idx + 1] < t { idx += 1; }
            let ta = self.times[idx];
            let tb = self.times[idx + 1];
            let va = pulse[idx] as f64;
            let vb = pulse[idx + 1] as f64;
            let w = ((t - ta) / (tb - ta).max(1e-9)).clamp(0.0, 1.0);
            out.push(va + (vb - va) * w);
        }
        out
    }

    /// Estimate respiration from the low-frequency ROI luminance (body motion
    /// and subtle intensity change track the breathing cycle).
    fn estimate_respiration(&mut self, now: f64) {
        let n = self.times.len();
        let span = now - self.times[0];
        if span < 20.0 { return; }

        let t_start = now - span.min(60.0);
        let mut i0 = 0;
        while i0 < n && self.times[i0] < t_start { i0 += 1; }
        if n - i0 < 32 { return; }

        // Uniform resample at 4 Hz.
        let t0 = self.times[i0];
        let t1 = self.times[n - 1];
        let m = ((t1 - t0) * RESP_RESAMPLE_HZ) as usize;
        if m < 64 { return; }
        let mut uniform = Vec::with_capacity(m);
        let mut idx = i0;
        for k in 0..m {
            let t = t0 + k as f64 / RESP_RESAMPLE_HZ;
            while idx + 1 < n - 1 && self.times[idx + 1] < t { idx += 1; }
            let ta = self.times[idx];
            let tb = self.times[idx + 1];
            let va = self.ch_lum[idx] as f64;
            let vb = self.ch_lum[idx + 1] as f64;
            let w = ((t - ta) / (tb - ta).max(1e-9)).clamp(0.0, 1.0);
            uniform.push(va + (vb - va) * w);
        }

        // Band-limit to the respiration band.
        let mut hp = Biquad::highpass(RESP_RESAMPLE_HZ, RESP_LO * 0.7);
        let mut lp = Biquad::lowpass(RESP_RESAMPLE_HZ, RESP_HI * 1.3);
        let pad = ((8.0 * RESP_RESAMPLE_HZ) as usize).min(uniform.len() - 1);
        for i in (1..=pad).rev() {
            let v = 2.0 * uniform[0] - uniform[i];
            lp.step(hp.step(v));
        }
        let filtered: Vec<f64> = uniform.iter().map(|&x| lp.step(hp.step(x))).collect();

        let (df, psd) = welch_psd(&filtered, RESP_RESAMPLE_HZ, 20.0, &mut self.planner);
        let lo = (RESP_LO / df).ceil() as usize;
        let hi = ((RESP_HI / df).floor() as usize).min(psd.len().saturating_sub(1));
        if lo >= hi { return; }
        let mut peak_i = lo;
        let mut band_sum = 0.0;
        for i in lo..=hi {
            if psd[i] > psd[peak_i] { peak_i = i; }
            band_sum += psd[i];
        }
        let f_resp = peak_i as f64 * df;
        // Peak dominance within the band as confidence proxy.
        let mut sig = 0.0;
        for i in lo..=hi {
            if ((i as f64 * df) - f_resp).abs() <= 0.04 { sig += psd[i]; }
        }
        let dominance = if band_sum > 0.0 { sig / band_sum } else { 0.0 };
        let conf = (dominance * 2.5).min(1.0);

        if conf > 0.15 {
            self.resp_hz = Some(match self.resp_hz {
                Some(prev) => 0.8 * prev + 0.2 * f_resp,
                None => f_resp,
            });
            self.resp_conf = 0.8 * self.resp_conf + 0.2 * conf;
        } else {
            self.resp_conf *= 0.97;
        }
    }

    fn run_estimators(&mut self, now: f64) {
        let n = self.times.len();
        if n < 16 || self.patches.is_empty() { return; }
        let span = now - self.times[0];

        self.estimate_respiration(now);
        let resp_hz = if self.resp_conf > 0.2 { self.resp_hz } else { None };

        // Warm-start longer windows from confident shorter ones (they estimate
        // the same HR; the 60 s window otherwise sits blank for a minute).
        if let (Some(b10), c10) = (self.est_10.tracked_bpm, self.est_10.confidence) {
            self.est_30.seed_from(b10, c10);
        }
        if let (Some(b30), c30) = (self.est_30.tracked_bpm, self.est_30.confidence) {
            self.est_60.seed_from(b30, c30);
        }

        for which in 0..3 {
            let (window_secs, min_span) = match which {
                0 => (10.0, 8.0),
                1 => (30.0, 24.0),
                _ => (60.0, 48.0),
            };
            if span < min_span { continue; }

            let t_start = now - window_secs;
            let mut i0 = 0;
            while i0 < n && self.times[i0] < t_start { i0 += 1; }
            if n - i0 < 16 { continue; }
            if self.times[n - 1] - self.times[i0] < min_span { continue; }

            // Per-patch spectra, fused by SNR weight. The pulse is coherent
            // across skin patches; noise is not — fusion lifts the true peak.
            let seg_secs = window_secs.min(15.0);
            let mut fused: Option<Vec<f64>> = None;
            let mut df_out = 0.0;
            for p in 0..self.patches.len() {
                let uniform = self.resample_pulse(p, i0, RESAMPLE_HZ);
                if uniform.len() < 32 { continue; }
                let filtered = bandpass(&uniform, RESAMPLE_HZ);
                let (df, psd) = welch_psd(&filtered, RESAMPLE_HZ, seg_secs, &mut self.planner);
                df_out = df;
                // Band-normalize and weight by own-peak SNR.
                let lo = (BAND_LO / df).ceil() as usize;
                let hi = ((BAND_HI / df).floor() as usize).min(psd.len().saturating_sub(1));
                if lo >= hi { continue; }
                let mut peak_f = lo as f64 * df;
                let mut peak_v = 0.0;
                let mut band_sum = 0.0;
                for i in lo..=hi {
                    band_sum += psd[i];
                    if psd[i] > peak_v { peak_v = psd[i]; peak_f = i as f64 * df; }
                }
                if band_sum <= 0.0 { continue; }
                let w = 10f64.powf(snr_db_at(df, &psd, peak_f) / 10.0).clamp(1e-3, 1e3);
                let acc = fused.get_or_insert_with(|| vec![0.0; psd.len()]);
                for i in 0..psd.len().min(acc.len()) {
                    acc[i] += w * psd[i] / band_sum;
                }
            }
            let Some(psd) = fused else { continue; };

            let est = match which {
                0 => &mut self.est_10,
                1 => &mut self.est_30,
                _ => &mut self.est_60,
            };
            let out = est.update_from_psd(df_out, &psd, resp_hz);
            if which == 0 {
                if let Some((_, snr)) = out {
                    self.last_snr_db = snr as f32;
                }
            }
        }
    }
}
