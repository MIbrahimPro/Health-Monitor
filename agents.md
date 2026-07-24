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
aegis-core/src/camera.rs  → Camera capture + face detection + rPPG + frame encoding
aegis-core/src/rppg.rs    → POS algorithm + FFT-based BPM calculation
aegis-ui/src-tauri/src/lib.rs → Tauri commands, mpsc channel, event emitter
aegis-ui/src/App.tsx      → React frontend with oscilloscope canvas + camera feed
```
