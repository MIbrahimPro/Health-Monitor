# Aegis — Remote Health Monitor

Aegis is a native, local-first desktop health monitor. It reads your **heart rate and respiration through your webcam** (remote photoplethysmography — rPPG), with a roadmap covering screen-comfort overlays, focus/context tracking, posture & emotion awareness, biometric security, ultrasonic ambient sensing, and a mobile companion. Everything runs locally; nothing leaves your machine.

## How the vitals engine works

Camera frames → rustface detection (async, downscaled) → EMA-smoothed ROI → 3×3 skin-patch grid (masked mean RGB) → **true POS** (Wang 2016) with incremental overlap-add → per-patch pulse traces → Butterworth bandpass (0.7–3 Hz) → Welch PSDs fused by SNR across patches → harmonic-aware peak selection (with breathing-harmonic rejection) → slew-limited, confidence-gated HR trackers (10/30/60 s) + respiration from low-frequency ROI luminance. All timing is wall-clock; the engine never assumes a frame rate.

## Build & run

```bash
# prerequisites: Rust stable, node/npm, ffmpeg (for the benchmark), a webcam
cargo build --release            # whole workspace
cd aegis-ui && npm install && npm run tauri dev    # desktop app
```

## Testing

```bash
cargo test --release -p aegis-core   # synthetic-signal accuracy tests (ground truth known)
scripts/bench.sh --label mytest      # offline benchmark on tests/test_video.avi (not in git)
```

The benchmark streams a recorded video through the exact production pipeline and grades stability, coverage, SNR, and BPM timelines against a full-recording spectral reference. See `plan-details/STATE.md` for the metrics history and why the lossy test fixture bounds absolute accuracy.

## Repository layout

```
aegis-core/     signal processing, camera pipeline, benchmark harness, tests
aegis-daemon/   headless runner
aegis-ui/       Tauri v2 + React dashboard
models/         local ML models (face detection)
scripts/        bench + recording utilities
plan.md         master phase roadmap
plan-details/   step-by-step execution playbook (state, per-phase guides)
```

## Roadmap status

| Phase | Scope | Status |
|---|---|---|
| 1 | Vitals engine (HR + respiration) + dashboard | Engine ✅ overhauled & benchmarked · backend/UI polish in progress |
| 2 | Comfort overlay, context/intent, phone detection | Planned — `plan-details/P2.md` |
| 3 | Posture, emotion, smart music | Planned — `plan-details/P3.md` |
| 4 | Face recognition, input biometrics | Planned — `plan-details/P4.md` |
| 5 | Ultrasonic sonar, Wi-Fi CSI, pairing | Planned — `plan-details/P5.md` |
| 6 | Mobile companion, 1.0 release | Planned — `plan-details/P6.md` |

## Privacy

Local-only by design: no cloud calls; test recordings of faces are gitignored; later phases store only embeddings/timing-histograms (never images or keystrokes) and default sensitive modules to off.
