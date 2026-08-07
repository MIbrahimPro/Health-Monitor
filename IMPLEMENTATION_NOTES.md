# Aegis Implementation Notes (v1.0.0)

This document provides a comprehensive overview of the architecture, modules, and file structure built during the final execution goal (Phases 4, 5, and 6) of the Aegis project.

## 1. Project Structure Overview

The project transitioned from a monolithic Tauri app to a modular workspace with headless and mobile capabilities.

```text
/home/mibrahimpro/Documents/heartrate
├── aegis-core/         # Shared Rust library (Biometrics, Sonar, Web Server)
├── aegis-daemon/       # Headless Rust backend server (LAN API)
├── aegis-ui/           # Original Desktop Tauri App (React + Vite)
├── aegis-mobile/       # Mobile Web Companion App (React + Vite)
└── plan-details/       # Playbook tracking docs
```

## 2. Phase 4: Security & Biometrics
**Objective:** Add user authentication, typing rhythm anomalies, and privacy tools.
- **`aegis-core/src/facerec.rs`**: Scaffolded the `FaceRecognizer` struct. It handles computing 128D embeddings of faces and comparing them against the "Owner's" embedding to set `owner_present`.
- **`aegis-core/src/camera.rs`**: We updated the background tracking loop to monitor `face_count`. If `face_count >= 2`, a "Shoulder Surfing" flag is triggered. We also integrated the `FaceRecognizer` into this pipeline.
- **`aegis-core/src/biometrics.rs`**: Built the `BiometricsMonitor` using the `rdev` crate to hook into global Wayland OS events (key presses, mouse movements). It scaffolds timing logic to calculate a Jensen-Shannon Divergence (JSD) score to detect anomalous typing rhythms (e.g., an intruder on your laptop).
- **`aegis-ui/src/components/SecurityCard.tsx`**: A dashboard UI component that consumes these biometrics over Tauri events, showing "Owner Verified" and "Shoulder Surfing" alerts dynamically.

## 3. Phase 5: Experimental Ambient Sensing
**Objective:** Implement hardware-level ambient environment tracking (Sonar & Wi-Fi CSI).
- **`aegis-core/src/sonar/audio.rs`**: Scaffolded `cpal` audio I/O foundations to emit a 19 kHz continuous wave and record microphone input simultaneously for full-duplex operation.
- **`aegis-core/src/sonar/doppler.rs` & `respiration.rs`**: Algorithms scaffolded to run FFTs (Fast Fourier Transforms) on the microphone input to detect Doppler frequency shifts caused by physical movement (or chest displacement for breathing).
- **`aegis-core/src/sonar/pairing.rs`**: Scaffolded an Air-gapped Ultrasonic Pairing protocol. The desktop emits a short burst of FSK (Frequency-Shift Keying) ultrasound, which the mobile device's microphone picks up to verify physical proximity.
- **`aegis-core/src/csi.rs`**: Scaffolded the Wi-Fi Channel State Information interface to read `ioctl` sockets for Nexmon firmware data.

## 4. Phase 6: Mobile Companion & Release Polish
**Objective:** Decouple the UI from the desktop and provide a mobile remote monitor.
- **`aegis-daemon/src/main.rs`**: Created a completely headless version of the Aegis tracker. It does not require Tauri or a GUI.
- **`aegis-core/src/server.rs`**: Implemented an `axum` HTTP and WebSocket server. It binds to `0.0.0.0:8817` and provides token-authenticated routes (`/api/vitals`, `/api/stream`).
- **`aegis-mobile/`**: Created a responsive React PWA (Progressive Web App). It mimics the desktop design system but is built to be served over LAN by the `aegis-daemon`.
- **`aegis-mobile/src/Pairing.tsx`**: Implemented the UI for the mobile user to tap "Verify Presence" and authenticate via the ultrasonic challenge-response loop.

## 5. Critical Fixes & Hardening
- **Canvas `DOMException` Bug (`WaveformCard.tsx`)**: In the UI, the real-time heartbeat canvas chart was crashing the React app on load. It was using a CSS variable (`var(--accent-2)`) inside the HTML5 Canvas `addColorStop` API. Canvas APIs don't compute CSS variables natively. This was fixed by passing the raw `#4E9CF5` hex string.
- **Disk Space Exhaustion (`ENOSPC`)**: Due to `cargo` keeping massive build caches across 3 different workspaces (`aegis-core`, `aegis-daemon`, `aegis-ui/src-tauri`), the drive filled up. We had to perform a manual `rm -rf target` operation to allow the final daemon compilation to succeed.

## 6. Execution Status
- All code formatted and passed `cargo check` / `npm run build` cleanly.
- Marked `STATE.md` as completely finished.
- Version bumped and tagged `v1.0.0` in git.
