# Aegis: Phase 1 Implementation Plan

**Objective:** Build the core foundation of Aegis. This includes setting up the Rust workspace, creating the modular Tauri dashboard (system tray), and implementing real-time heart rate (rPPG) and respiration monitoring using the POS algorithm.

---

## 1. Project Initialization & Architecture

### 1.1 Cargo Workspace Setup
We will use a Cargo workspace to separate the UI (Tauri) from the Core Engine (Computer Vision/rPPG).
*   **`aegis-core` (Library Crate):** Contains the heavy-lifting logic (OpenCV, Math, AI models).
*   **`aegis-daemon` (Binary Crate):** The headless background worker that runs the loops.
*   **`aegis-ui` (Tauri App):** The frontend that runs in the system tray and communicates with `aegis-daemon` via IPC/WebSockets.

**Required Crates for Phase 1:**
*   `tauri` (System tray and UI)
*   `opencv` (Rust bindings for OpenCV - V4L2 camera capture)
*   `ndarray` or `nalgebra` (For linear algebra / matrix operations required by the rPPG algorithm)
*   `tract-onnx` or `ort` (For running a lightweight face detection model like UltraFace or BlazeFace locally)
*   `tokio` (For async runtime, managing the camera loop independently from the UI loop)
*   `rustfft` (For frequency analysis to extract BPM)

---

## 2. Implementing the Modular Dashboard (Tauri)

### 2.1 The System Tray
*   Initialize Tauri with `cargo tauri init`.
*   Configure the app to be **borderless** and **hidden from the dock/taskbar**.
*   Implement a `SystemTray` in `main.rs` that spawns the UI window only when the tray icon is clicked.
*   The UI (React/SolidJS) will feature simple toggles (e.g., "Enable Camera Tracking", "Show Vitals").
*   Use Tauri's `invoke` system to send commands to the Rust backend to start/stop the `tokio` camera task.

---

## 3. Implementing Real-Time Vitals (The Engine)

Unlike the old `stattohr.py` which processed entire video files using simple moving averages, we are building a **live, real-time streaming pipeline**.

### 3.1 The Camera Loop (`tokio` thread)
1.  Spawn a dedicated `tokio` thread for the camera to ensure UI never blocks.
2.  Use `opencv::videoio::VideoCapture` to open device `0`.
3.  Capture frames at ~30 FPS. Resize frames to a smaller resolution (e.g., 640x480) immediately to save CPU cycles.

### 3.2 Face & Landmark Detection
*   We will **not** rely on manual clicks for a chroma key anymore.
*   Load a lightweight ONNX face detector (like UltraFace) using the `ort` crate.
*   Extract the bounding box of the face.
*   Define the **Region of Interest (ROI)** as the upper cheeks and forehead (avoiding the eyes and mouth where movement causes noise).

### 3.3 The POS (Plane-Orthogonal-to-Skin) rPPG Algorithm
We are migrating away from simple RGB averaging to the highly robust **POS algorithm** (Wang et al., 2016), which is mathematically proven to suppress motion artifacts.

**The Sliding Window Buffer:**
*   Maintain a circular buffer (`VecDeque` or `ndarray`) of the average RGB values of the ROI for the last `N` frames (e.g., `N = 45` frames for a 1.5-second window at 30fps).

**The Math (Implemented via `ndarray`):**
For every new frame added to the window:
1.  **Temporal Normalization:** Divide the RGB signals in the window by their temporal mean. (This creates $R_n, G_n, B_n$).
2.  **Projection:** Project the normalized signals onto two orthogonal axes:
    *   $X = 3 \cdot R_n - 2 \cdot G_n$
    *   $Y = 1.5 \cdot R_n + G_n - 1.5 \cdot B_n$
3.  **Alpha Tuning:** Calculate the standard deviation of $X$ and $Y$ over the window.
    *   $\alpha = \frac{std(X)}{std(Y)}$
4.  **Signal Extraction:** Calculate the raw pulse signal: 
    *   $H = X - \alpha \cdot Y$
5.  **Filtering & BPM:**
    *   Apply a Bandpass Filter (e.g., 0.7 Hz to 3.0 Hz, corresponding to 42 - 180 BPM) to $H$.
    *   Run `rustfft` on the filtered signal. The frequency bin with the highest power magnitude is the heart rate!

### 3.4 Respiration (Breathing Rate)
*   Instead of color changes, respiration is tracked via optical flow or landmark tracking on the **shoulders/chest**.
*   Using OpenCV, track the vertical (Y-axis) displacement of the lower bounding box of the user.
*   Apply a strict low-pass filter (0.2 Hz to 0.5 Hz) over a much longer sliding window (e.g., 10 seconds).
*   Count the peaks in this wave to calculate Breaths Per Minute.

---

## 4. Hand-off to LLM (Next Steps for AI Coder)
If you are passing this to an LLM to generate code, prompt it with the following sequence:

1.  *"Generate the `Cargo.toml` workspace configuring `aegis-core`, `aegis-daemon`, and `aegis-ui`."*
2.  *"Write the `aegis-core` OpenCV camera loop using a `tokio` channel to broadcast frames."*
3.  *"Implement the POS rPPG algorithm in Rust using `ndarray`, taking a sliding window of RGB vectors as input."*
4.  *"Wire the Tauri system tray in `aegis-ui` to start and stop the `tokio` thread."*
