//! Synthetic-signal accuracy tests for the rPPG engine.
//!
//! These run without a camera or video: they synthesize a skin-tone RGB trace
//! with a known pulsatile frequency and assert the engine locks onto it.

use aegis_core::rppg::PosRppg;

const FPS: f64 = 16.6; // matches what the real webcam delivers

/// Feed `secs` seconds of synthetic pulse at `bpm` and return the last outputs.
fn run_synthetic(bpm: f64, secs: f64, drift_per_sec: f64, noise_amp: f32) -> (Option<f32>, Option<f32>) {
    let mut rppg = PosRppg::new();
    let f = bpm / 60.0;
    let n = (secs * FPS) as usize;
    // Deterministic pseudo-noise (no rand dependency).
    let mut seed = 0x12345678u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 16) as f32 / 32768.0 - 1.0
    };
    let mut b10 = None;
    let mut b30 = None;
    for i in 0..n {
        let t = i as f64 / FPS;
        let pulse = (2.0 * std::f64::consts::PI * f * t).sin() as f32;
        let drift = (drift_per_sec * t) as f32;
        // Blood volume raises absorption: G dips strongest, R and B follow weaker.
        let r = 150.0 + drift + 0.4 * pulse + noise_amp * noise();
        let g = 110.0 + drift * 0.8 - 1.0 * pulse + noise_amp * noise();
        let b = 90.0 + drift * 0.6 - 0.3 * pulse + noise_amp * noise();
        let out = rppg.process_frame(r, g, b, t);
        if out.bpm_10s.is_some() { b10 = out.bpm_10s; }
        if out.bpm_30s.is_some() { b30 = out.bpm_30s; }
    }
    (b10, b30)
}

#[test]
fn locks_72_bpm_clean() {
    let (b10, b30) = run_synthetic(72.0, 45.0, 0.0, 0.05);
    let b10 = b10.expect("no 10s BPM produced");
    let b30 = b30.expect("no 30s BPM produced");
    assert!((b10 - 72.0).abs() < 5.0, "10s window: got {b10}, want 72±5");
    assert!((b30 - 72.0).abs() < 5.0, "30s window: got {b30}, want 72±5");
}

#[test]
fn locks_60_bpm_with_drift() {
    // Slow illumination drift must not drag the estimate (classic detrend bug).
    let (b10, _) = run_synthetic(60.0, 45.0, 0.8, 0.05);
    let b10 = b10.expect("no 10s BPM produced");
    assert!((b10 - 60.0).abs() < 5.0, "10s window: got {b10}, want 60±5");
}

#[test]
fn locks_110_bpm_noisy() {
    let (b10, _) = run_synthetic(110.0, 45.0, 0.0, 0.4);
    let b10 = b10.expect("no 10s BPM produced");
    assert!((b10 - 110.0).abs() < 6.0, "10s window: got {b10}, want 110±6");
}

#[test]
fn rejects_breathing_harmonic_lock() {
    // Breathing at 16/min with a sawtooth-ish shape has a 3rd harmonic at 48
    // "BPM" hitting all channels as intensity/motion. True pulse at 78 BPM is
    // weaker. The engine must report ~78 and a respiration rate ~16.
    let mut rppg = PosRppg::new();
    let f_hr = 78.0 / 60.0;
    let f_br = 16.0 / 60.0;
    let n = (90.0 * FPS) as usize;
    let mut seed = 0x9e3779b9u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 16) as f32 / 32768.0 - 1.0
    };
    let mut b30 = None;
    let mut resp = None;
    for i in 0..n {
        let t = i as f64 / FPS;
        let w_hr = 2.0 * std::f64::consts::PI * f_hr * t;
        let w_br = 2.0 * std::f64::consts::PI * f_br * t;
        let pulse = w_hr.sin() as f32;
        // Harmonic-rich breathing motion artifact (equal on all channels).
        let breath = (w_br.sin() + 0.55 * (2.0 * w_br).sin() + 0.4 * (3.0 * w_br).sin()) as f32;
        let r = 150.0 + 2.0 * breath + 0.4 * pulse + 0.1 * noise();
        let g = 110.0 + 2.0 * breath - 1.0 * pulse + 0.1 * noise();
        let b = 90.0 + 2.0 * breath - 0.3 * pulse + 0.1 * noise();
        let out = rppg.process_frame(r, g, b, t);
        if out.bpm_30s.is_some() { b30 = out.bpm_30s; }
        if out.resp_bpm.is_some() { resp = out.resp_bpm; }
    }
    let b30 = b30.expect("no 30s BPM produced");
    assert!(
        (b30 - 78.0).abs() < 6.0,
        "locked to {b30} (breathing harmonic trap at 48), want 78±6"
    );
    let resp = resp.expect("no respiration rate produced");
    assert!((resp - 16.0).abs() < 3.0, "respiration {resp}, want 16±3");
}

#[test]
fn no_subharmonic_lock_at_84() {
    // The historical failure mode: reporting 42 when the truth is 84.
    let (b10, b30) = run_synthetic(84.0, 60.0, 0.0, 0.1);
    for (name, v) in [("10s", b10), ("30s", b30)] {
        let v = v.expect("no BPM produced");
        assert!(
            (v - 84.0).abs() < 6.0,
            "{name} window locked to {v}, want 84±6 (subharmonic check)"
        );
    }
}
