# Aegis: Comprehensive Project & Implementation Plan

**Aegis** is a native, low-level desktop background daemon designed to monitor user health, ergonomics, productivity, and device security via advanced biometric profiling and ambient sensing.

This document serves as the master blueprint. It is designed to be highly technical and structured into distinct phases so that AI coding assistants can consume a single phase and implement it accurately.

## Core Tech Stack
*   **Language:** Rust (for the core daemon, memory safety, and native OS APIs).
*   **Desktop UI:** Tauri (for a lightweight, web-based system tray dashboard).
*   **Computer Vision/ML:** `opencv-rust`, `tract` or `onnxruntime-rs` for executing tiny, local ML models.
*   **Platforms:** Linux (Primary: X11/Wayland, V4L2, ALSA), Windows (Secondary: Win32, WASAPI), macOS (Tertiary).
*   **Audio/Media:** `rodio` (Rust audio playback) and `aubio` / `essentia` (for audio feature extraction).

---

## Phase 1: Core Foundation & Vitals Engine
**Goal:** Establish the background daemon, the modular UI, and the camera-based vital sign extraction.

### 1.1 Modular Control Dashboard (Tauri)
*   **Implementation:** Initialize a Tauri project. Create a minimal React/SolidJS frontend that lives in the OS system tray.
*   **Functionality:** Expose Rust commands to toggle individual modules (e.g., `enable_camera_module`, `enable_keystroke_hook`). Store settings in a local SQLite or JSON config file.

### 1.2 Real-Time Heart Rate (rPPG) & Respiration
*   **Implementation:** Use `opencv-rust` to capture webcam frames. Implement a facial landmark detector (via a lightweight ONNX model) to isolate the forehead and cheeks.
*   **Algorithm:** Maintain a sliding window buffer of the last ~90-150 frames. Convert the ROI (Region of Interest) to YUV/LAB color space. Apply the **CHROM** or **POS** rPPG algorithm (remote photoplethysmography) to extract the pulse signal.
*   **Respiration:** Track the vertical Y-axis movement of the shoulder landmarks over a 10-second sliding window to calculate breaths per minute.

---

## Phase 2: Context-Aware Productivity & Environmental Hooks
**Goal:** Hook into the OS to monitor screen activity and adjust the display for eye comfort.

### 2.1 E-Ink / Vision Comfort Mode
*   **Implementation:** Use OS-specific compositing APIs (e.g., X11 overlay windows, Win32 transparent layered windows) to render a full-screen, click-through overlay.
*   **Features:** Apply a procedural noise shader (for "paper" grain) and a warm color temperature filter (similar to f.lux) that dynamically adjusts based on the time of day or user fatigue metrics.

### 2.2 Smart Context & Intent Tracking
*   **Implementation:** Hook into OS window managers to track the title and executable of the active window. Hook into local DNS/network APIs (`pnet` or `pcap` crate) to log active domains.
*   **AI Intent Inference:** Pass the active context (e.g., `[App: Premiere Pro] + [Browser: youtube.com]`) into a tiny, local LLM or a fast text-classification model (using `tract`). The model classifies the state as "Deep Work", "Research", or "Distraction".

### 2.3 Phone Distraction Detection
*   **Implementation:** Train or use a lightweight YOLO/MobileNet model via ONNX. Run inference on the webcam feed at a low framerate (e.g., 2 FPS) to detect the "cell phone" object class specifically when it intersects with the user's hand/face bounding boxes.

---

## Phase 3: Ergonomics & The Smart Music Engine
**Goal:** Correct physical posture and regulate emotional state via AI-driven local music playback.

### 3.1 Posture-Linked Screen Blurring
*   **Implementation:** Using the facial landmarks from Phase 1, calculate the distance between eyes (to estimate distance from screen) and the angle of the neck.
*   **Correction Loop:** If the user slouches beyond a threshold for >30 seconds, trigger an IPC call to the Phase 2 Screen Overlay. The overlay gradually applies a Gaussian blur. When the camera detects the posture is corrected, the blur instantly animates away.

### 3.2 Emotion Detection (Facial)
*   **Implementation:** Run a lightweight facial expression recognition model (e.g., Mini-Xception) on the facial crop via ONNX. Classify into states: Neutral, Happy, Sad, Stressed, Fatigued.

### 3.3 AI Music Tagging & Smart Player
*   **Implementation:** Build a local audio player using the `rodio` crate.
*   **Local AI Tagging:** Scan a local folder of `.mp3`/`.flac` files. Use audio feature extraction libraries (like `essentia` or a tiny neural network via ONNX) to analyze BPM, key, and spectral features.
*   **Tagging Output:** Automatically tag local tracks as *Calm, Focus, Energetic, Sad, Motivational*.
*   **Auto-Selector:** Cross-reference Phase 3.2 (Emotion). If the user is flagged as "Stressed", the player automatically queues and crossfades into music tagged as *Calm/Focus*.

---

## Phase 4: Advanced Security & Biometrics
**Goal:** Implement invisible, zero-friction security measures using input rhythms and facial recognition.

### 4.1 Facial Recognition & Shoulder Surfing
*   **Implementation:** Use a face embedding model (e.g., MobileFaceNet). Calculate the cosine similarity of the current face against the owner's baseline embedding.
*   **Shoulder Surfing:** If `number_of_faces > 1`, instantly trigger the Phase 2 screen overlay to blur the screen or minimize windows via OS hooks.

### 4.2 Keystroke & Mouse Biometric Signature
*   **Implementation:** Use the `rdev` crate (or raw X11/Win32 hooks) to capture global input events.
*   **Keyboard:** Record *Dwell Time* (Key Down -> Key Up) and *Flight Time* (Key Up -> Next Key Down). 
*   **Mouse:** Record velocity, acceleration curves, and click frequency.
*   **Profiling:** Feed these continuous timing arrays into a local Anomaly Detection algorithm (e.g., Isolation Forest or a tiny Autoencoder).
*   **Active Protection:** If an anomaly is detected (intruder typing), silently start recording the webcam, lock the screen, or trigger an alert.

---

## Phase 5: Experimental Ambient Sensing
**Goal:** Utilize sound and radio waves for non-optical sensing.

### 5.1 Active Acoustic Sonar (Ultrasonic Radar)
*   **Implementation:** Use `cpal` to generate a continuous 18kHz-22kHz sine wave from the speakers. Simultaneously record audio from the microphone.
*   **Signal Processing:** Run an FFT (Fast Fourier Transform) via the `rustfft` crate. Look for Doppler shifts in the 18-22kHz band. 
*   **Feature:** Translate these shifts into physical movement detection (breathing micro-movements, or large movements of someone walking behind the user).

### 5.2 Wi-Fi CSI (Channel State Information) Sensing
*   **Implementation:** Extract CSI data from the Wi-Fi NIC (requires specific Linux drivers like `nexmon` or Intel CSI Tool).
*   **Feature:** Feed the amplitude and phase shifts of the Wi-Fi subcarriers into a machine learning model to detect whole-room occupancy without a camera.

### 5.3 Ultrasonic Air-Gapped Device Pairing
*   **Implementation:** Implement a basic modulation scheme (e.g., FSK or Chirp Spread Spectrum) in the 18kHz+ range.
*   **Feature:** The laptop broadcasts an ultrasonic challenge. A trusted device (phone) hears it and responds with a cryptographic signature. This acts as an invisible, air-gapped proximity unlock token.

---

## Phase 6: Mobile Application & Ecosystem (Late Stage)
**Goal:** Extend the system's reach beyond the desktop.

### 6.1 Companion App (React Native / Flutter)
*   **Implementation:** Build a mobile app that acts as the trusted token for Phase 5.3.
*   **Features:**
    *   View health, productivity, and ergonomic statistics synced locally over LAN from the desktop daemon.
    *   Remote control the desktop dashboard.
    *   Act as a secondary microphone/sensor for the ultrasonic sonar network.
