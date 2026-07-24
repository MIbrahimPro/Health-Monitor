//! Synthetic-signal accuracy tests for the rPPG engine.
//!
//! These run without a camera or video: they synthesize a skin-tone RGB trace
//! with a known pulsatile frequency and assert the engine locks onto it.

use aegis_core::rppg::PosRppg;

const FPS: f64 = 16.6; // matches what the real webcam delivers

/// Feed `secs` seconds of synthetic pulse at `bpm` and return the last outputs.
fn run_synthetic(bpm: f64, secs: f64, drift_per_sec: f64, noise_amp: f32) -> (Option<f32>, Option<f32>) {
    let mut rppg = PosRppg::new(45);
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
        let (_, o10, o30, _) = rppg.process_frame(r, g, b, t);
        if o10.is_some() { b10 = o10; }
        if o30.is_some() { b30 = o30; }
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
