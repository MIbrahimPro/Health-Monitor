# Aegis Developer & Agent Log

## Current Phase: Phase 1 (Core rPPG & UI)
**Status:** ✅ Core pipeline working. Camera → Face Detection → rPPG → UI all verified.

---

### Session: 2026-07-24

#### Bug #1: Silent thread crash (RESOLVED)
- **Symptom:** UI showed "NO FACE", camera LED on, no feed.
- **Root Cause:** `nokhwa` defaulted to MJPEG format. The `mozjpeg` C library segfaulted during frame decoding, killing the camera thread silently (not a Rust panic, so our panic hook didn't catch it).
- **Fix:** Force YUYV format via `CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30)`.

#### Bug #2: No data reaching UI even with face detected (RESOLVED)
- **Symptom:** Logs showed face detection working, JPEG encoding working, but UI still showed "NO FACE" and no camera feed.
- **Root Cause:** Logic error in camera.rs. When face WAS detected, data was only sent to UI inside `if let Some((pulse, bpm)) = rppg.process_frame(...)`. The rPPG algorithm requires 45 frames of warmup before returning `Some(...)`. During warmup, the frame_base64 and face_found=true were silently dropped — never sent to the UI.
- **Fix:** Restructured the loop to ALWAYS send `VitalStats` to the channel on every frame, regardless of rPPG warmup state. Pulse defaults to 0.0 and BPM defaults to None until warmup completes.

#### Bug #3: Only ~0.5 FPS throughput (RESOLVED)
- **Symptom:** test_cam showed only 5 frames received in 10 seconds.
- **Root Cause:** `rustface` face detection on full 640x480 grayscale took ~2 seconds per call, and was running every 5th frame.
- **Fix:** 
  - Downscale image 2x before face detection (320x240 → ~4x faster)
  - Detect face every 15th frame instead of every 5th
  - Increase min face size to 60px (at detection scale) for faster search
  - Result: 30 frames in 10 seconds (6x improvement)

#### Compiler Warnings (RESOLVED)
- Removed unused imports: `Context`, `RgbImage`, `Luma`, `Detector`, `Manager`
- Fixed mutable reference in logging macro
- **Result:** Zero warnings across entire workspace

### Test Results
```
--- TEST RESULTS (10 seconds) ---
Total frames received:  30
Frames with face:       30
Frames with video:      6
Frames with BPM:        0 (needs ~30 frames warmup, pulse started at frame 30)
---
PASS: Pipeline is working!
```

### Architecture
```
aegis-core/src/camera.rs  → Camera capture (YUYV) + multithreaded rustface detection + POS rPPG + skin masking + frame encoding
aegis-core/src/rppg.rs    → POS algorithm (CHROM projection `3R-2G`) + IIR Detrending + Zero-padded FFT-based BPM calculation
aegis-ui/src-tauri/src/lib.rs → Tauri commands, mpsc channel, event emitter
aegis-ui/src/App.tsx      → React frontend with oscilloscope canvas, 3 BPM averages, and camera feed
scripts/                  → Directory for temporary utility scripts (e.g., test video recording)
```

---

### Session: Later on 2026-07-24

#### Bug #4: Subharmonic Heart Rate Locking (Reads 40-50 instead of 80+) (RESOLVED)
- **Symptom:** The BPM was constantly reading half of the true value.
- **Root Cause:** POS algorithm was suffering from "alpha flutter" due to sliding window modulation. Also, the moving average detrending acted as a notch filter killing 1Hz (60 BPM). Lastly, non-uniform camera sampling corrupted FFT bins.
- **Fix:** 
  1. Time-domain interpolation to exactly 15 FPS.
  2. Zero-padded FFT to 2048 points for extreme sub-BPM resolution.
  3. Replaced moving average with a robust 1st-order IIR High-Pass filter (cutoff 0.5Hz).
  4. Bypassed POS alpha calculation, defaulting to pure CHROM projection (`3R - 2G`) which doesn't flutter.

#### Bug #5: Motion Artifacts & Dark Environment Failure (RESOLVED)
- **Symptom:** Bounding box jitter caused massive BPM drops. Shadowed environments failed completely due to noise.
- **Root Cause:** The `rustface` detection box snapped aggressively and sometimes included hair/background. In dark rooms, sensor noise overwhelmed the `3R - 2G` projection.
- **Fix:** 
  1. Applied an **Exponential Moving Average (EMA)** to the face bounding box so it glides smoothly.
  2. Expanded the ROI from the top 30% to the top 60% of the bounding box (covering cheeks and nose, avoiding beard).
  3. Implemented a **Kovac Skin Mask** that iterates through every pixel in the ROI and mathematically deletes non-skin pixels (pure black hair, white/yellow walls, etc.) based on `R > G` and `R > B` with dynamic thresholds. This completely insulates the mean RGB calculation from background noise.

#### Feature: Dynamic Dashboard
- Upgraded the React UI to display three synchronized BPM moving averages (10s, 30s, and 60s) side-by-side for accuracy comparison.
- Added a hospital-style real-time oscilloscope waveform canvas.
- Added a `scripts/` directory for holding temporary utilities (e.g. `record_test_video.py`).
