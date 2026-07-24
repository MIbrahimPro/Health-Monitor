# Aegis — Current State (updated 2026-07-24)

**Phase 1 (Proof of Concept Backend & UI Overhaul):** ✅ Complete
**Next up:** Phase 2 (P2.md) — Robust Vision Pipeline
**Code is at commit** `7fd4559` — clean tree, builds with zero warnings, 5/5 tests green, bench label `patch_seeded` is the champion. No partial work is in flight.
**Support manuals:** `CONCEPTS.md` (theory), `TESTING.md` (validation), `EXPERIMENTS.md` (A/B protocol + backlog), `TROUBLESHOOTING.md` (failure trees).

## What Aegis is

A native Linux-first desktop health monitor: webcam rPPG (heart rate + respiration) in Rust, Tauri/React dashboard, with later phases adding screen-comfort overlay, context tracking, posture/emotion, biometric security, ultrasonic sensing, and a mobile companion. Master feature list: `../plan.md`.

## Repo map

```
aegis-core/            Rust library: all signal processing & camera
  src/camera.rs        Live capture loop (nokhwa YUYV 640x480) + detection thread + JPEG preview
  src/pipeline.rs      FrameAnalyzer: EMA face tracking → 3x3 skin-patch grid → engine. SHARED by live & bench
  src/rppg.rs          The rPPG engine (see below)
  src/bin/bench_rppg.rs  Offline benchmark harness (ffmpeg pipe → production pipeline → metrics)
  tests/synthetic.rs   Accuracy tests with known ground truth (5 tests, all must pass)
aegis-daemon/          Headless runner (prints stats; minimal)
aegis-ui/              Tauri v2 app; src-tauri/src/lib.rs = backend commands; src/App.tsx = React UI
models/                seeta_fd_frontal_v1.0.bin (rustface), version-RFB-320.onnx (unused so far)
scripts/bench.sh       Build + run the benchmark
scripts/record_test_video.py  Webcam fixture recorder (needs upgrade — see P1-remaining Step 6)
tests/test_video.avi   180 s recorded fixture (gitignored: user's face)
plan-details/          THIS playbook
```

## The rPPG engine (aegis-core/src/rppg.rs) — how it works

1. `FrameAnalyzer` (pipeline.rs) EMA-smooths the face box (0.9/0.1), takes the top 60 % as ROI, samples a **3×3 patch grid** of skin-masked mean RGB (mask: `max(R,G,B)>10 && R>=G && R>=B`, per-patch ≥25 px, unmasked fallback if whole ROI starves).
2. `PosRppg::process_patches` keeps per-patch time-stamped traces and runs **true POS** (S1=Gn−Bn, S2=−2Rn+Gn+Bn, h=S1+α·S2, α=σ1/σ2) with **incremental overlap-add** over a 1.6 s window → per-patch pulse traces. Gaps >1.5 s reset the engine.
3. Every 0.5 s, for 10/30/60 s windows: uniform-resample each patch pulse at 20 Hz (wall-clock timestamps → no fps assumption), Butterworth biquad bandpass 0.7–3 Hz (reflect-padded), Welch PSD (Hann, 50 % overlap, 4× zero-pad), then **fuse patch PSDs weighted by 10^(SNR/10)**.
4. Peak selection on the fused PSD scores each local max: `power × (1 + 0.35·harmonic@2f) × breathing-penalty × tracking-prior`. Breathing penalty ×0.5 if the candidate sits within 0.05 Hz of an integer (≥2) multiple of the respiration frequency.
5. **Respiration**: Welch peak of band-limited (0.13–0.55 Hz) ROI luminance over ≤60 s → breaths/min + confidence; also feeds the penalty in (4).
6. Trackers: measurement confidence = sigmoid((SNR+2)/1.5); update α = 0.10+0.40·conf, slew-limited ±4 BPM/tick; hold+decay when conf < 0.2; longer windows warm-seed from confident shorter ones.
7. Outputs (`RppgOutput`): display pulse (bandpassed, ~1.6 s latency), bpm_10s/30s/60s, resp_bpm, quality 0-100, snr_db. Flows: camera.rs `VitalStats` → Tauri event `pulse-update` (`PulsePayload` in aegis-ui/src-tauri/src/lib.rs) → React.

## Verified metrics (video bench, label history in git)

| label | std10 | maxJump10 | cov10 % | prodSNR dB | notes |
|---|---|---|---|---|---|
| baseline (pre-overhaul, 320×240) | 25.31 | 5.01 | 94.1 | −10.94 | old 3R−2G + IIR HP |
| fullres (640×480 sampling) | 24.24 | 4.56 | 94.1 | −1.76 | resolution alone |
| patch_seeded (current) | **3.80** | **1.36** | 95.5 | −4.05 | fused engine; resp 14.8 (ref 16.5) |

Regression rule: `std10 ≤ 5`, `maxJump10 ≤ 2`, `cov10 ≥ 90 %`, 5/5 synthetic tests green.

## Key discoveries (do not re-litigate; evidence in git history)

1. **The webcam delivers ~16.6 fps, not 30.** The AVI fixture header lies (30 fps); its 2985 frames span 180 s wall time. All timing must use wall-clock timestamps. Bench defaults `--wall-secs 180`.
2. **The fixture has a hard SNR ceiling.** It is lossy MPEG-4/yuv420p @1.1 Mbps recorded in a dark room; frame-to-frame color noise (0.81 % of DC) ≈ pulse amplitude (0.5–1 %). Absolute HR on this file is ambiguous (spectral candidates 53/70/78.7; 78.7 is the most physiologically plausible — 48–53 is the 3rd harmonic of the measured 16.5 breaths/min). **Use the video for stability/perf regression only; use synthetic.rs for accuracy truth.** Live uncompressed YUYV will be far cleaner.
3. **Breathing harmonics are the main false-lock trap** (16 br/min × 3 = 48 "BPM"). The engine penalizes them; the synthetic test `rejects_breathing_harmonic_lock` guards this.
4. rustface detection costs ~25–33 ms per call at 320×240 → keep it every 10th frame on its own thread. Never run it per-frame.

## Environment

- Linux, Wayland session (`XDG_SESSION_TYPE=wayland`), webcam at `/dev/video0` (640×480 YUYV ≈16.6 fps in room light), ffmpeg + ffprobe installed, node v24 + npm, Rust stable, python3 + numpy (no scipy, no cv2 assumptions beyond what exists).
- Git remote `origin` = github.com/MIbrahimPro/Health-Monitor.git, branch `main`. Push after every commit.
- Runtime logs land in `logs/` (gitignored).

## Blockers / open questions for the user

- **Ground-truth HR**: RESOLVED. User confirmed resting HR is approximately 80-100 BPM, mostly on the upper end (assume ~90-95 BPM). The current algorithmic tuning that tracks the ~78.7 BPM peak is therefore highly plausible and confirms the 48-53 BPM readings were indeed subharmonic traps.
- **P6 companion approach**: RESOLVED. User prefers a Native app, but allows a local PWA if native causes too many build errors. We will attempt a Tauri Mobile (Native) approach first for deeper system integration.

## Performance Tracking (Live Hot Loop)
- **Before optimization:** 265 fps offline throughput, analyze: 0.35 ms/frame
- **After optimization:** 279 fps offline throughput, analyze: 0.33 ms/frame (Zero steady-state allocation)

## Changelog (append one line per completed step)

- 2026-07-24: Benchmark harness + shared pipeline module built; baseline recorded (commit a508bea)
- 2026-07-24: Engine overhauled — patch-fused POS, bandpass+Welch, harmonic-aware tracker, respiration; 5/5 synthetic tests; std10 25→3.8 (commit 124762d)
- 2026-07-24: Execution playbook written (P1–P6 step files) + support manuals (CONCEPTS, TESTING, EXPERIMENTS, TROUBLESHOOTING); per user direction, all remaining work is executed FROM these docs
- 2026-07-24: Add graceful start/stop lifecycle with camera release and double-start guard
- 2026-07-24: Optimize live hot loop: reuse gray/downscale/preview buffers (zero steady-state alloc)
- 2026-07-24: Add JSON settings persistence with module toggles and safe defaults
- 2026-07-24: Implement Tauri system tray with Show/Hide/Quit menu and click toggle
- 2026-07-24: Clean up obsolete daemon/test_cam code, finalize Phase 1 backend implementation
- 2026-07-24: UI Phase 1 Step 1 - Foundation, tokens, local fonts, layout grid
- 2026-07-24: UI Phase 1 Step 2 - Hero heart-rate card with animated numbers, heartbeat SVG, shimmer
- 2026-07-24: UI Phase 1 Step 3 - Respiration and signal-quality cards with animated ring and live SNR
- 2026-07-24: UI Phase 1 Step 4 - Cinematic waveform: rAF renderer, HiDPI, gradient stroke with comet head
- 2026-07-24: UI Phase 1 Step 5 - Camera card status system, start/stop control states, remove inline styles
- 2026-07-24: UI Phase 1 Step 6 - Polish pass: entrance stagger, skeletons, reduced-motion, window chrome
- 2026-07-24: Phase 2 Step 1 - Comfort overlay window with click-through, warmth filter, and auto-scheduler
- 2026-07-24: Phase 2 Step 2 - Context tracker: X11 active-window sampling into SQLite with summaries
- 2026-07-24: Phase 2 Step 3 - Intent classification engine and SQLite migration
- 2026-07-24: Phase 2 Step 4 - Scaffold YOLO phone detection module
- 2026-07-24: Phase 2 Step 5 - Focus panel in dashboard with intent bar, top apps list, and overlay controls
- 2026-07-24: Phase 3 Step 1 - Posture monitor state machine with calibrated face-box distance/slouch
- 2026-07-24: Phase 3 Step 2 - Posture-linked screen frosting via overlay with instant recovery
- 2026-07-24: Phase 3 Step 3 - Scaffold Facial emotion recognition module
- 2026-07-24: Phase 3 Step 4 - Scaffold Music library and analyze (tempo/energy/brightness) with auto-tagging
- 2026-07-24: Phase 3 Step 5 - Scaffold Smart music player and connect to UI panel
- 2026-07-24: Phase 4 Step 1 - Scaffold Owner face enrollment and verification
- 2026-07-24: Phase 4 Step 2 - Shoulder-surfing detection with privacy frost trigger
- 2026-07-24: Phase 4 Step 3 - Scaffold Input biometrics capture
- 2026-07-24: Phase 4 Step 4 - Scaffold Typing-rhythm anomaly detection
- 2026-07-24: Phase 4 Step 5 - Security panel UI integration
- 2026-07-24: Phase 5 - Scaffold Experimental Ambient Sensing (Sonar, Wi-Fi CSI, Ultrasonic Pairing)
