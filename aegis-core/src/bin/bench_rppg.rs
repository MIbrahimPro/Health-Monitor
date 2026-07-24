//! Offline rPPG benchmark harness.
//!
//! Streams a recorded video through the EXACT production pipeline
//! (`pipeline::FrameAnalyzer`) via an ffmpeg rawvideo pipe, then grades the
//! output against a high-quality offline reference estimate computed from the
//! full recording (POS overlap-add + zero-phase bandpass + Welch PSD).
//!
//! Usage:
//!   bench_rppg [--video tests/test_video.avi] [--wall-secs 180] [--fps F]
//!              [--label name] [--csv tests/results/name.csv] [--detect-every 10]
//!
//! `--wall-secs` overrides the container frame rate: real webcams often deliver
//! ~16 fps while the AVI header claims 30, which would inflate BPM by ~1.8x.

use aegis_core::camera::find_face_model;
use aegis_core::pipeline::{downscale_gray, rgb_to_gray, FaceBox, FrameAnalyzer};
use anyhow::{bail, Context, Result};
use rustface::ImageData;
use rustfft::{num_complex::Complex, FftPlanner};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Instant;

const BAND_LO: f64 = 0.7; // Hz (42 BPM)
const BAND_HI: f64 = 3.0; // Hz (180 BPM)

struct Args {
    video: String,
    wall_secs: Option<f64>,
    fps_override: Option<f64>,
    label: String,
    csv: Option<String>,
    detect_every: u64,
    dump_rois: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args {
        video: "tests/test_video.avi".into(),
        wall_secs: Some(180.0),
        fps_override: None,
        label: "run".into(),
        csv: None,
        detect_every: 10,
        dump_rois: None,
    };
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--video" => { args.video = argv[i + 1].clone(); i += 2; }
            "--wall-secs" => { args.wall_secs = Some(argv[i + 1].parse().expect("bad --wall-secs")); i += 2; }
            "--container-fps" => { args.wall_secs = None; i += 1; }
            "--fps" => { args.fps_override = Some(argv[i + 1].parse().expect("bad --fps")); i += 2; }
            "--label" => { args.label = argv[i + 1].clone(); i += 2; }
            "--csv" => { args.csv = Some(argv[i + 1].clone()); i += 2; }
            "--detect-every" => { args.detect_every = argv[i + 1].parse().expect("bad --detect-every"); i += 2; }
            "--dump-rois" => { args.dump_rois = Some(argv[i + 1].clone()); i += 2; }
            other => { eprintln!("Unknown arg: {}", other); std::process::exit(2); }
        }
    }
    args
}

fn ffprobe_meta(video: &str) -> Result<(u32, u32, f64, u64)> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
            "-of", "csv=p=0", video,
        ])
        .output()
        .context("ffprobe not found — install ffmpeg")?;
    if !out.status.success() {
        bail!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = text.trim().split(',').collect();
    if parts.len() < 4 {
        bail!("unexpected ffprobe output: {}", text);
    }
    let w: u32 = parts[0].parse()?;
    let h: u32 = parts[1].parse()?;
    let rate: f64 = {
        let fr: Vec<&str> = parts[2].split('/').collect();
        let num: f64 = fr[0].parse()?;
        let den: f64 = if fr.len() > 1 { fr[1].parse()? } else { 1.0 };
        num / den.max(1.0)
    };
    let nb: u64 = parts[3].parse().unwrap_or(0);
    Ok((w, h, rate, nb))
}

// ---------------------------------------------------------------------------
// Offline reference analysis (gold standard over the whole recording)
// ---------------------------------------------------------------------------

/// 2nd-order Butterworth section (RBJ cookbook), Direct Form I.
#[derive(Clone)]
struct Biquad { b0: f64, b1: f64, b2: f64, a1: f64, a2: f64, x1: f64, x2: f64, y1: f64, y2: f64 }

impl Biquad {
    fn lowpass(fs: f64, fc: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * fc / fs;
        let (sw, cw) = (w.sin(), w.cos());
        let alpha = sw / (2.0 * std::f64::consts::FRAC_1_SQRT_2.recip());
        let a0 = 1.0 + alpha;
        Self { b0: ((1.0 - cw) / 2.0) / a0, b1: (1.0 - cw) / a0, b2: ((1.0 - cw) / 2.0) / a0,
               a1: (-2.0 * cw) / a0, a2: (1.0 - alpha) / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
    fn highpass(fs: f64, fc: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * fc / fs;
        let (sw, cw) = (w.sin(), w.cos());
        let alpha = sw / (2.0 * std::f64::consts::FRAC_1_SQRT_2.recip());
        let a0 = 1.0 + alpha;
        Self { b0: ((1.0 + cw) / 2.0) / a0, b1: (-(1.0 + cw)) / a0, b2: ((1.0 + cw) / 2.0) / a0,
               a1: (-2.0 * cw) / a0, a2: (1.0 - alpha) / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
    fn step(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x;
        self.y2 = self.y1; self.y1 = y;
        y
    }
    fn run(&self, signal: &[f64]) -> Vec<f64> {
        let mut f = self.clone();
        signal.iter().map(|&x| f.step(x)).collect()
    }
}

/// Zero-phase 0.7–3 Hz bandpass (forward-backward Butterworth HP+LP).
fn bandpass_zero_phase(signal: &[f64], fs: f64) -> Vec<f64> {
    let hp = Biquad::highpass(fs, BAND_LO);
    let lp = Biquad::lowpass(fs, BAND_HI);
    let fwd = lp.run(&hp.run(signal));
    let mut rev: Vec<f64> = fwd.into_iter().rev().collect();
    rev = lp.run(&hp.run(&rev));
    rev.reverse();
    rev
}

/// True POS (Wang et al. 2016) with overlap-add over the full trace.
fn pos_overlap_add(r: &[f64], g: &[f64], b: &[f64], fs: f64) -> Vec<f64> {
    let n = r.len();
    let l = ((1.6 * fs).round() as usize).max(8).min(n);
    let mut h = vec![0.0f64; n];
    for s in 0..=(n - l) {
        let (mut mr, mut mg, mut mb) = (0.0, 0.0, 0.0);
        for i in s..s + l { mr += r[i]; mg += g[i]; mb += b[i]; }
        mr /= l as f64; mg /= l as f64; mb /= l as f64;
        if mr <= 0.0 || mg <= 0.0 || mb <= 0.0 { continue; }

        let mut s1 = vec![0.0f64; l];
        let mut s2 = vec![0.0f64; l];
        for i in 0..l {
            let rn = r[s + i] / mr;
            let gn = g[s + i] / mg;
            let bn = b[s + i] / mb;
            s1[i] = gn - bn;
            s2[i] = -2.0 * rn + gn + bn;
        }
        let std1 = std_dev(&s1);
        let std2 = std_dev(&s2);
        let alpha = if std2 > 1e-12 { std1 / std2 } else { 0.0 };
        let mut hw: Vec<f64> = (0..l).map(|i| s1[i] + alpha * s2[i]).collect();
        let mean_h: f64 = hw.iter().sum::<f64>() / l as f64;
        for v in hw.iter_mut() { *v -= mean_h; }
        for i in 0..l { h[s + i] += hw[i]; }
    }
    h
}

fn std_dev(v: &[f64]) -> f64 {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64).sqrt()
}

/// Welch PSD with Hann windows. Returns (freq_step_hz, psd).
fn welch_psd(signal: &[f64], fs: f64, seg_secs: f64) -> (f64, Vec<f64>) {
    let seg_len = ((seg_secs * fs) as usize).min(signal.len()).max(16);
    let hop = seg_len / 2;
    let nfft = (seg_len * 4).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);

    let hann: Vec<f64> = (0..seg_len)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (seg_len - 1) as f64).cos())
        .collect();

    let mut psd = vec![0.0f64; nfft / 2];
    let mut segments = 0usize;
    let mut start = 0usize;
    while start + seg_len <= signal.len() {
        let seg = &signal[start..start + seg_len];
        let mean: f64 = seg.iter().sum::<f64>() / seg_len as f64;
        let mut buf: Vec<Complex<f64>> = vec![Complex::new(0.0, 0.0); nfft];
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

/// Local maxima of the PSD inside the HR band, sorted by power desc.
fn band_peaks(df: f64, psd: &[f64]) -> Vec<(f64, f64)> {
    let lo = (BAND_LO / df).ceil() as usize;
    let hi = ((BAND_HI / df).floor() as usize).min(psd.len().saturating_sub(2));
    let mut peaks = Vec::new();
    for i in lo.max(1)..=hi {
        if psd[i] > psd[i - 1] && psd[i] >= psd[i + 1] {
            peaks.push((i as f64 * df, psd[i]));
        }
    }
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    peaks
}

/// de Haan SNR: power near f0 and its 2nd harmonic vs the rest of the band.
fn snr_db(df: f64, psd: &[f64], f0: f64) -> f64 {
    let lo = (BAND_LO / df).ceil() as usize;
    let hi = ((BAND_HI / df).floor() as usize).min(psd.len() - 1);
    let mut sig = 0.0;
    let mut noise = 0.0;
    for i in lo..=hi {
        let f = i as f64 * df;
        let in_sig = (f - f0).abs() <= 0.1 || (f - 2.0 * f0).abs() <= 0.2;
        if in_sig { sig += psd[i]; } else { noise += psd[i]; }
    }
    if noise <= 0.0 { return 99.0; }
    10.0 * (sig / noise).log10()
}

// ---------------------------------------------------------------------------
// Multi-ROI experiment dump: samples several candidate face regions per frame
// (EMA-smoothed like production) so extraction geometry can be compared offline.
// ---------------------------------------------------------------------------

const ROI_NAMES: [&str; 5] = ["upper60", "forehead", "cheeks", "midband", "fhcheeks"];
const MASK_NAMES: [&str; 2] = ["loose", "strict"];

struct RoiDump {
    smoothed: Option<(f32, f32, f32, f32)>,
    rows: Vec<String>,
}

impl RoiDump {
    fn new() -> Self {
        Self { smoothed: None, rows: Vec::new() }
    }

    fn header() -> String {
        let mut h = String::from("frame,t,face");
        for roi in ROI_NAMES {
            for mask in MASK_NAMES {
                h.push_str(&format!(",{roi}_{mask}_r,{roi}_{mask}_g,{roi}_{mask}_b,{roi}_{mask}_n"));
            }
        }
        h.push('\n');
        h
    }

    fn record(&mut self, rgb: &[u8], w: u32, h: u32, face: Option<FaceBox>, frame: u64, t: f64) {
        match (face, self.smoothed) {
            (Some(f), Some((sx, sy, sw, sh))) => {
                self.smoothed = Some((
                    sx * 0.9 + f.x as f32 * 0.1,
                    sy * 0.9 + f.y as f32 * 0.1,
                    sw * 0.9 + f.w as f32 * 0.1,
                    sh * 0.9 + f.h as f32 * 0.1,
                ));
            }
            (Some(f), None) => self.smoothed = Some((f.x as f32, f.y as f32, f.w as f32, f.h as f32)),
            (None, _) => self.smoothed = None,
        }

        let mut row = format!("{},{:.3},{}", frame, t, self.smoothed.is_some() as u8);
        if let Some((fx, fy, fw, fh)) = self.smoothed {
            // Candidate regions, expressed in face-box fractions. The rustface
            // box spans roughly eyebrows→chin, so "forehead" extends above it.
            let regions: [Vec<(f32, f32, f32, f32)>; 5] = [
                vec![(0.0, 0.0, 1.0, 0.6)],                       // upper60 (current prod)
                vec![(0.22, -0.24, 0.56, 0.26)],                  // forehead (above box)
                vec![(0.06, 0.42, 0.30, 0.28), (0.64, 0.42, 0.30, 0.28)], // both cheeks
                vec![(0.05, 0.35, 0.90, 0.38)],                   // midface band
                vec![(0.22, -0.24, 0.56, 0.26), (0.06, 0.42, 0.30, 0.28), (0.64, 0.42, 0.30, 0.28)], // forehead+cheeks
            ];
            for patches in &regions {
                for strict in [false, true] {
                    let (mut sr, mut sg, mut sb, mut n) = (0.0f64, 0.0f64, 0.0f64, 0u64);
                    for &(rx, ry, rw, rh) in patches {
                        let x0 = (fx + rx * fw).max(0.0) as u32;
                        let y0 = (fy + ry * fh).max(0.0) as u32;
                        let x1 = ((fx + (rx + rw) * fw).max(0.0) as u32).min(w);
                        let y1 = ((fy + (ry + rh) * fh).max(0.0) as u32).min(h);
                        for y in y0..y1 {
                            let rowbase = (y * w) as usize * 3;
                            for x in x0..x1 {
                                let idx = rowbase + x as usize * 3;
                                if idx + 2 < rgb.len() {
                                    let r = rgb[idx] as f64;
                                    let g = rgb[idx + 1] as f64;
                                    let b = rgb[idx + 2] as f64;
                                    let keep = if strict {
                                        r > g + 4.0 && r > b + 4.0 && r.max(g).max(b) > 20.0 && r < 250.0
                                    } else {
                                        r.max(g).max(b) > 10.0 && r >= g && r >= b
                                    };
                                    if keep {
                                        sr += r; sg += g; sb += b; n += 1;
                                    }
                                }
                            }
                        }
                    }
                    if n > 0 {
                        row.push_str(&format!(",{:.3},{:.3},{:.3},{}", sr / n as f64, sg / n as f64, sb / n as f64, n));
                    } else {
                        row.push_str(",,,,0");
                    }
                }
            }
        } else {
            for _ in 0..ROI_NAMES.len() * MASK_NAMES.len() {
                row.push_str(",,,,0");
            }
        }
        row.push('\n');
        self.rows.push(row);
    }
}

#[derive(Default)]
struct TimelineStats {
    count: usize,
    mae: f64,
    rmse: f64,
    within5: f64,
    std: f64,
    max_jump: f64,
    first_t: f64,
    last: Option<f64>,
}

fn grade_timeline(times: &[f64], values: &[Option<f32>], reference: f64) -> TimelineStats {
    let mut st = TimelineStats::default();
    let mut vals: Vec<f64> = Vec::new();
    let mut prev: Option<f64> = None;
    let mut first_t = None;
    for (i, v) in values.iter().enumerate() {
        if let Some(b) = v {
            let b = *b as f64;
            if first_t.is_none() { first_t = Some(times[i]); }
            vals.push(b);
            let err = (b - reference).abs();
            st.mae += err;
            st.rmse += err * err;
            if err <= 5.0 { st.within5 += 1.0; }
            if let Some(p) = prev {
                let jump = (b - p).abs();
                if jump > st.max_jump { st.max_jump = jump; }
            }
            prev = Some(b);
        }
    }
    st.count = vals.len();
    if st.count > 0 {
        st.mae /= st.count as f64;
        st.rmse = (st.rmse / st.count as f64).sqrt();
        st.within5 = 100.0 * st.within5 / st.count as f64;
        let mean = vals.iter().sum::<f64>() / st.count as f64;
        st.std = (vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / st.count as f64).sqrt();
        st.first_t = first_t.unwrap_or(0.0);
        st.last = vals.last().copied();
    }
    st
}

fn print_timeline(name: &str, st: &TimelineStats, total_frames: usize) {
    if st.count == 0 {
        println!("  {:<8} NO OUTPUT PRODUCED", name);
        return;
    }
    println!(
        "  {:<8} n={:<5} cov={:>5.1}%  MAE={:>5.2}  RMSE={:>5.2}  ±5bpm={:>5.1}%  std={:>5.2}  maxJump={:>5.2}  last={:>6.1}  firstAt={:>5.1}s",
        name, st.count,
        100.0 * st.count as f64 / total_frames as f64,
        st.mae, st.rmse, st.within5, st.std, st.max_jump,
        st.last.unwrap_or(0.0), st.first_t
    );
}

fn main() -> Result<()> {
    let args = parse_args();

    let (src_w, src_h, container_fps, nb_frames) = ffprobe_meta(&args.video)?;
    let fps = if let Some(f) = args.fps_override {
        f
    } else if let Some(wall) = args.wall_secs {
        if nb_frames == 0 { bail!("container reports no frame count; pass --fps"); }
        nb_frames as f64 / wall
    } else {
        container_fps
    };

    println!("=== Aegis rPPG Benchmark ===");
    println!("video:  {} ({}x{}, container {}fps, {} frames)", args.video, src_w, src_h, container_fps, nb_frames);
    println!("timing: {:.3} fps effective ({})", fps,
        if args.fps_override.is_some() { "manual override".into() }
        else if let Some(w) = args.wall_secs { format!("{} frames over {}s wall clock", nb_frames, w) }
        else { "container rate".to_string() });
    println!("label:  {}", args.label);

    // --- decode pipe at native resolution: rPPG samples full-res pixels,
    // face detection runs on a 2x-downscaled grayscale (production design) ---
    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", &args.video,
            "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .context("ffmpeg not found — install ffmpeg")?;
    let mut pipe = child.stdout.take().unwrap();
    let detect_factor: u32 = if src_w >= 640 { 2 } else { 1 };

    // --- face detector (identical settings to production) ---
    let model_path = find_face_model().context("rustface model not found — run from repo root")?;
    let mut detector = rustface::create_detector(&model_path)
        .map_err(|e| anyhow::anyhow!("detector init: {:?}", e))?;
    detector.set_min_face_size(30);
    detector.set_score_thresh(2.0);

    let mut analyzer = FrameAnalyzer::new();

    let frame_size = (src_w * src_h * 3) as usize;
    let mut buf = vec![0u8; frame_size];

    let mut times: Vec<f64> = Vec::new();
    let mut faces: Vec<bool> = Vec::new();
    let mut rgb_trace: Vec<Option<(f32, f32, f32)>> = Vec::new();
    let mut pulses: Vec<f32> = Vec::new();
    let mut bpm10: Vec<Option<f32>> = Vec::new();
    let mut bpm30: Vec<Option<f32>> = Vec::new();
    let mut bpm60: Vec<Option<f32>> = Vec::new();
    let mut resp: Vec<Option<f32>> = Vec::new();
    let mut qualities: Vec<f32> = Vec::new();

    let mut last_face: Option<FaceBox> = None;
    let mut roi_dump = RoiDump::new();
    let mut frame_idx: u64 = 0;
    let mut detect_time = 0.0f64;
    let mut detect_calls = 0u64;
    let mut analyze_time = 0.0f64;

    let wall_start = Instant::now();
    loop {
        match pipe.read_exact(&mut buf) {
            Ok(()) => {}
            Err(_) => break, // EOF
        }
        let t = frame_idx as f64 / fps;

        if frame_idx % args.detect_every == 0 {
            let d0 = Instant::now();
            let gray_full = rgb_to_gray(&buf, src_w, src_h);
            let (gray, gw, gh) = downscale_gray(&gray_full, src_w, src_h, detect_factor);
            let mut image_data = ImageData::new(&gray, gw, gh);
            let detections = detector.detect(&mut image_data);
            last_face = detections
                .into_iter()
                .max_by_key(|f| f.bbox().width() * f.bbox().height())
                .map(|face| {
                    let bbox = face.bbox();
                    let f = detect_factor;
                    let x = ((bbox.x().max(0) as u32) * f).min(src_w - 1);
                    let y = ((bbox.y().max(0) as u32) * f).min(src_h - 1);
                    let fw = ((bbox.width().max(0) as u32) * f).min(src_w - x);
                    let fh = ((bbox.height().max(0) as u32) * f).min(src_h - y);
                    FaceBox { x, y, w: fw, h: fh }
                });
            detect_time += d0.elapsed().as_secs_f64();
            detect_calls += 1;
        }

        let a0 = Instant::now();
        let res = analyzer.process_frame(&buf, src_w, src_h, last_face, t);
        analyze_time += a0.elapsed().as_secs_f64();

        if args.dump_rois.is_some() {
            roi_dump.record(&buf, src_w, src_h, last_face, frame_idx, t);
        }

        times.push(t);
        faces.push(res.face_found);
        rgb_trace.push(res.mean_rgb);
        pulses.push(res.raw_pulse);
        bpm10.push(res.bpm_10s);
        bpm30.push(res.bpm_30s);
        bpm60.push(res.bpm_60s);
        resp.push(res.resp_bpm);
        qualities.push(res.quality);

        frame_idx += 1;
    }
    let _ = child.wait();
    if let Some(path) = &args.dump_rois {
        if let Some(dir) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut out = RoiDump::header();
        for r in &roi_dump.rows { out.push_str(r); }
        std::fs::write(path, out)?;
        println!("ROI dump written: {}", path);
    }
    let total_wall = wall_start.elapsed().as_secs_f64();
    let n = times.len();
    if n < 100 {
        bail!("only {} frames decoded — video unreadable?", n);
    }

    let face_frames = faces.iter().filter(|f| **f).count();
    println!("\n--- Pipeline run ---");
    println!("  frames: {}   face detected: {} ({:.1}%)", n, face_frames, 100.0 * face_frames as f64 / n as f64);
    println!("  wall: {:.1}s  ({:.0} fps offline)   detect: {:.1} ms/call x{}   analyze: {:.2} ms/frame",
        total_wall, n as f64 / total_wall,
        1000.0 * detect_time / detect_calls.max(1) as f64, detect_calls,
        1000.0 * analyze_time / n as f64);

    // --- Reference estimate from the full recording ---
    // Interpolate mean-RGB gaps to a contiguous uniform trace.
    let first = rgb_trace.iter().position(|v| v.is_some());
    let last = rgb_trace.iter().rposition(|v| v.is_some());
    let (Some(first), Some(last)) = (first, last) else {
        bail!("no face/skin samples found in the entire video");
    };
    let mut rr = Vec::with_capacity(last - first + 1);
    let mut gg = Vec::with_capacity(last - first + 1);
    let mut bb = Vec::with_capacity(last - first + 1);
    {
        let mut i = first;
        while i <= last {
            if let Some((r, g, b)) = rgb_trace[i] {
                rr.push(r as f64); gg.push(g as f64); bb.push(b as f64);
                i += 1;
            } else {
                // linear interp across the gap
                let prev = i - 1;
                let mut j = i;
                while rgb_trace[j].is_none() { j += 1; }
                let (pr, pg, pb) = rgb_trace[prev].unwrap();
                let (nr, ng, nb2) = rgb_trace[j].unwrap();
                let gap = (j - prev) as f64;
                for k in i..j {
                    let w = (k - prev) as f64 / gap;
                    rr.push(pr as f64 + (nr as f64 - pr as f64) * w);
                    gg.push(pg as f64 + (ng as f64 - pg as f64) * w);
                    bb.push(pb as f64 + (nb2 as f64 - pb as f64) * w);
                }
                i = j;
            }
        }
    }

    // Respiration reference from ROI luminance (breathing body motion).
    let lum: Vec<f64> = rr.iter().zip(&gg).zip(&bb)
        .map(|((r, g), b)| 0.299 * r + 0.587 * g + 0.114 * b)
        .collect();
    let resp_ref_hz = {
        let (dfl, psdl) = welch_psd(&lum, fps, 40.0);
        let lo = (0.13 / dfl).ceil() as usize;
        let hi = ((0.55 / dfl).floor() as usize).min(psdl.len() - 1);
        let mut peak = lo;
        for i in lo..=hi {
            if psdl[i] > psdl[peak] { peak = i; }
        }
        peak as f64 * dfl
    };

    let pos = pos_overlap_add(&rr, &gg, &bb, fps);
    let pos_bp = bandpass_zero_phase(&pos, fps);
    let (df, psd) = welch_psd(&pos_bp, fps, 30.0);
    let peaks = band_peaks(df, &psd);
    if peaks.is_empty() { bail!("no spectral peaks in HR band — reference failed"); }
    let pmax = peaks[0].1;

    // Same evidence-based selection the engine uses: harmonic support bonus,
    // breathing-harmonic penalty (no tracking prior offline).
    let p_at = |f: f64| -> f64 {
        let i = (f / df).round() as usize;
        let a = i.saturating_sub(2);
        let b = (i + 2).min(psd.len() - 1);
        psd[a..=b].iter().cloned().fold(0.0, f64::max)
    };
    let mut scored: Vec<(f64, f64, f64)> = peaks.iter().map(|&(f, p)| {
        let harmonic = if 2.0 * f <= BAND_HI + 0.5 { 0.35 * p_at(2.0 * f) / pmax } else { 0.0 };
        let k = (f / resp_ref_hz).round();
        let breath_penalty = if resp_ref_hz > 0.05 && k >= 2.0 && (f - k * resp_ref_hz).abs() < 0.05 { 0.5 } else { 1.0 };
        ((p / pmax) * (1.0 + harmonic) * breath_penalty, f, p)
    }).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let (_, f0, p0) = scored[0];
    let ref_bpm = f0 * 60.0;
    let ref_snr = snr_db(df, &psd, f0);

    println!("\n--- Reference (full-recording POS + Welch, harmonic-aware) ---");
    println!("  REFERENCE HR: {:.1} BPM  (f0={:.3} Hz)   SNR: {:+.2} dB", ref_bpm, f0, ref_snr);
    println!("  REFERENCE RESP: {:.1} breaths/min", resp_ref_hz * 60.0);
    println!("  top peaks (score | rel power):");
    for (s, f, p) in scored.iter().take(5) {
        println!("    {:>6.1} BPM   score {:>5.3}   rel {:>5.3}", f * 60.0, s, p / p0.max(1e-12));
    }

    // Production pulse-signal quality: bandpass the raw production pulse and
    // measure its SNR at the reference frequency.
    let pulse_f64: Vec<f64> = pulses[first..=last].iter().map(|p| *p as f64).collect();
    let pulse_bp = bandpass_zero_phase(&pulse_f64, fps);
    let (dfp, psd_p) = welch_psd(&pulse_bp, fps, 30.0);
    let prod_snr = snr_db(dfp, &psd_p, f0);
    let prod_peak = band_peaks(dfp, &psd_p).first().map(|(f, _)| f * 60.0).unwrap_or(0.0);
    println!("\n--- Production pulse signal ---");
    println!("  spectral peak: {:.1} BPM   SNR@ref: {:+.2} dB", prod_peak, prod_snr);

    // --- Grade the live BPM timelines ---
    println!("\n--- Production BPM timelines vs reference {:.1} BPM ---", ref_bpm);
    let st10 = grade_timeline(&times, &bpm10, ref_bpm);
    let st30 = grade_timeline(&times, &bpm30, ref_bpm);
    let st60 = grade_timeline(&times, &bpm60, ref_bpm);
    print_timeline("bpm10", &st10, n);
    print_timeline("bpm30", &st30, n);
    print_timeline("bpm60", &st60, n);

    // --- CSV dump ---
    let csv_path = args.csv.unwrap_or(format!("tests/results/{}.csv", args.label));
    if let Some(dir) = std::path::Path::new(&csv_path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut csv = String::from("frame,t,face,r,g,b,pulse,bpm10,bpm30,bpm60,resp,quality\n");
    for i in 0..n {
        let (r, g, b) = rgb_trace[i].map(|(r, g, b)| (r, g, b)).unwrap_or((0.0, 0.0, 0.0));
        csv.push_str(&format!(
            "{},{:.3},{},{:.2},{:.2},{:.2},{:.5},{},{},{},{},{:.1}\n",
            i, times[i], faces[i] as u8, r, g, b, pulses[i],
            bpm10[i].map(|v| format!("{:.2}", v)).unwrap_or_default(),
            bpm30[i].map(|v| format!("{:.2}", v)).unwrap_or_default(),
            bpm60[i].map(|v| format!("{:.2}", v)).unwrap_or_default(),
            resp[i].map(|v| format!("{:.2}", v)).unwrap_or_default(),
            qualities[i],
        ));
    }
    std::fs::write(&csv_path, csv)?;
    println!("\nCSV written: {}", csv_path);

    let resp_vals: Vec<f64> = resp.iter().flatten().map(|v| *v as f64).collect();
    let resp_mean = if resp_vals.is_empty() { 0.0 } else { resp_vals.iter().sum::<f64>() / resp_vals.len() as f64 };
    println!("  production respiration: mean {:.1} breaths/min (ref {:.1}), coverage {:.1}%",
        resp_mean, resp_ref_hz * 60.0, 100.0 * resp_vals.len() as f64 / n as f64);

    // Machine-readable one-liner for tracking across commits.
    println!(
        "\nSUMMARY {} refBPM={:.1} refSNR={:+.2} prodSNR={:+.2} prodPeak={:.1} mae10={:.2} mae30={:.2} mae60={:.2} rmse10={:.2} within5_10={:.1} cov10={:.1} std10={:.2} resp={:.1} respRef={:.1}",
        args.label, ref_bpm, ref_snr, prod_snr, prod_peak,
        st10.mae, st30.mae, st60.mae, st10.rmse, st10.within5,
        100.0 * st10.count as f64 / n as f64, st10.std, resp_mean, resp_ref_hz * 60.0
    );

    Ok(())
}
