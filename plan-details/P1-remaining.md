# Phase 1 — Remaining (performance + production backend)

**Prerequisite:** read `STATE.md`. The rPPG engine is done and benchmarked. This file finishes the *backend*: making start/stop, the system tray, and settings production-grade, plus a performance pass. UI is a separate file (`P1-ui.md`).

Work top to bottom. Each step is self-contained.

---

## Step 1 — Graceful start/stop lifecycle `[x]`

**Goal:** The camera loop can be cleanly started AND stopped, and cannot be double-started. Today `start_tracking` spawns a thread that runs forever with no way to stop it or release the camera.

**Files:** `aegis-core/src/camera.rs`, `aegis-ui/src-tauri/src/lib.rs`

**Instructions:**
1. In `camera.rs`, change `start_camera_loop` to accept a stop signal:
   ```rust
   use std::sync::atomic::{AtomicBool, Ordering};
   use std::sync::Arc;
   // signature:
   pub fn start_camera_loop(sender: mpsc::Sender<VitalStats>, stop: Arc<AtomicBool>) -> Result<()>
   ```
2. In the main capture loop, at the top of each iteration:
   ```rust
   if stop.load(Ordering::Relaxed) { log_msg!(log_file, "Stop requested."); break; }
   ```
   Also break the detection thread: send it the stop flag too, or drop `detect_tx` when the loop exits (dropping the sender ends its `while let Ok(..)`). Simplest: `break` the main loop, then after the loop `drop(detect_tx)` happens automatically as it goes out of scope — confirm the detection thread exits.
3. After the loop, explicitly release: `drop(camera);` and log "Camera released."
   **⚠ Call sites — the signature change breaks TWO existing callers; update both or the workspace won't compile:**
   - `aegis-ui/src-tauri/src/lib.rs` → `start_camera_loop(tx, stop_flag)` (from AppState, see below)
   - `aegis-daemon/src/main.rs` → `start_camera_loop(tx, Arc::new(AtomicBool::new(false)))` (daemon runs until killed; a real flag arrives with the P6 server)
4. In `lib.rs`, hold shared state so the command handlers can coordinate:
   ```rust
   use std::sync::atomic::AtomicBool;
   use std::sync::Arc;
   struct AppState { running: Arc<AtomicBool> }
   ```
   Register it with `.manage(AppState { running: Arc::new(AtomicBool::new(false)) })` in the builder.
5. Rewrite `start_tracking` to:
   - Read `AppState`. If `running` is already `true`, return `"Tracking"` (idempotent, no second camera).
   - Set `running = true`, create a fresh `Arc<AtomicBool>` stop flag stored so stop can flip it (store the stop flag in `AppState` behind a `Mutex<Option<Arc<AtomicBool>>>`).
   - Pass the stop flag into `start_camera_loop`.
6. Add a `stop_tracking` command: flip the stop flag to `true`, set `running=false`, return `"Stopped"`. Register it in `generate_handler!`.

**Verify:**
```bash
cargo build --release 2>&1 | grep -cE "error|warning"   # expect 0
```
Then a manual smoke (needs webcam+display): `cd aegis-ui && npm run tauri dev`, click Initialize → Stop → Initialize again. Camera LED should turn off on Stop and the app should not crash or freeze on restart. If no display is available this session, note "manual smoke deferred" in STATE.md and rely on the build passing.

**If it fails:**
- Detection thread hangs on exit → it's blocked on `detect_rx.recv()`; ensure `detect_tx` is dropped (don't clone it into anything long-lived) or add a stop check via `recv_timeout`.
- Camera won't reopen after stop → the previous `Camera` wasn't dropped; ensure the thread fully exits before `start_tracking` returns success on restart (guard with the `running` flag).

**Commit:** `Add graceful start/stop lifecycle with camera release and double-start guard`

---

## Step 2 — Performance pass on the hot loop `[x]`

**Goal:** Reduce per-frame allocation and CPU. Current offline throughput ~230 fps (plenty), but the live path allocates a fresh grayscale Vec every detection and a preview Vec every 5th frame. Target: no avoidable per-frame heap allocation in the steady state.

**Files:** `aegis-core/src/camera.rs`, `aegis-core/src/pipeline.rs`

**Instructions:**
1. Measure first. Run `scripts/bench.sh --label perf_before` and record the "analyze: X ms/frame" and "offline fps" from the output into STATE.md.
2. In `camera.rs`, hoist reusable buffers outside the loop and reuse them:
   - `gray_full` buffer for `rgb_to_gray` — add `rgb_to_gray_into(&buf, w, h, &mut gray)` variant in pipeline.rs that clears+extends a passed `&mut Vec<u8>` instead of allocating.
   - `downscale_gray_into` similarly.
   - preview buffer reused across frames.
3. `skin_patch_grid` already does a single pass — leave it. Do NOT micro-optimize the math; correctness first.
4. Keep detection every 10th frame and preview every 5th.

**Verify:**
```bash
cargo test --release -p aegis-core 2>&1 | grep "result:"   # all ok
scripts/bench.sh --label perf_after
```
Compare `perf_after` SUMMARY to `perf_before`: `std10`, `cov10`, `resp` must be unchanged (±2 %); "analyze ms/frame" should be ≤ before. **Accuracy metrics must not move** — this is a pure refactor.

**If it fails:** if any metric shifts, you changed behavior, not just allocation. Revert and reapply one buffer at a time, re-benching each.

**Commit:** `Optimize live hot loop: reuse gray/downscale/preview buffers (zero steady-state alloc)`

---

## Step 3 — Settings persistence (JSON config) `[x]`

**Goal:** Per `plan.md` 1.1: store module toggles + preferences in a local JSON config file, loaded on start, written on change.

**Files:** new `aegis-core/src/config.rs`, wire in `aegis-ui/src-tauri/src/lib.rs`

**Instructions:**
1. Add `serde = { version="1", features=["derive"] }` and `serde_json = "1"` to `aegis-core/Cargo.toml` (already present in UI crate; add to core).
2. Create `config.rs`:
   ```rust
   use serde::{Deserialize, Serialize};
   #[derive(Serialize, Deserialize, Clone)]
   #[serde(default)]
   pub struct Config {
       pub camera_module: bool,
       pub show_vitals: bool,
       pub overlay_module: bool,     // used in P2
       pub context_module: bool,     // used in P2
       // add fields per phase as needed; #[serde(default)] tolerates old files
   }
   impl Default for Config { fn default() -> Self { Self {
       camera_module: true, show_vitals: true, overlay_module: false, context_module: false,
   }}}
   ```
   Add `pub fn config_path() -> PathBuf` → `dirs`-style: use `std::env::var("XDG_CONFIG_HOME")` or `~/.config/aegis/config.json`. Add `load()` (returns Default if missing/corrupt) and `save(&self)`.
3. In `lib.rs` add commands `get_config() -> Config` and `set_config(cfg: Config)` (saves to disk, updates managed state). Register both.
4. `start_tracking` should respect `camera_module` (refuse if disabled, return a clear message).

**Verify:**
```bash
cargo build --release 2>&1 | grep -cE "error|warning"   # 0
# unit test round-trip:
cargo test --release -p aegis-core config 2>&1 | grep "result:"
```
Add a test in `aegis-core/tests/config.rs` that saves a non-default config to a temp path and loads it back equal. Also corrupt-file test: write `"{ garbage"`, `load()` returns `Default`.

**If it fails:** permission denied writing config → ensure the parent dir is created (`create_dir_all`) before write.

**Commit:** `Add JSON settings persistence with module toggles and safe defaults`

---

## Step 4 — System tray `[ ]`

**Goal:** Per `plan.md` 1.1 / `phase1_plan.md` 2.1: app lives in the system tray; clicking the tray icon shows/hides the window; a menu offers Show / Hide / Quit.

**Files:** `aegis-ui/src-tauri/src/lib.rs`, `aegis-ui/src-tauri/Cargo.toml`, `aegis-ui/src-tauri/tauri.conf.json`

**Instructions:**
1. Enable the tray feature: in `Cargo.toml` `tauri = { version = "2", features = ["tray-icon"] }`.
2. In `lib.rs` `run()`, build a tray icon with a menu (Show, Hide, Quit) using Tauri v2 `TrayIconBuilder` and `MenuBuilder`. Wire:
   - Left click on icon → toggle main window visibility.
   - "Quit" → `app.exit(0)`.
3. In `tauri.conf.json`, consider `"visible": false` at startup if the app should start hidden to tray (optional — default visible is fine; document the choice).
4. Reuse the existing `icons/32x32.png` for the tray icon.

**Verify:** `cargo build --release 2>&1 | grep -cE "error|warning"` → 0. Manual: tray icon appears; menu works; window toggles. If headless this session, note deferred smoke in STATE.md.

**If it fails:** tray API differs across Tauri 2 minor versions — check the exact `tauri` version in `Cargo.lock` and match the `TrayIconBuilder` API for that version (search the version's docs). Do not downgrade Tauri.

**Commit:** `Add system tray with show/hide/quit and click-to-toggle window`

---

## Step 5 — Wire quality/SNR/respiration into the payload end-to-end `[ ]`

**Goal:** Confirm the new engine outputs (`quality`, `snr_db`, `resp_bpm`) reach the frontend. (Backend already carries them; this step is the contract check + a daemon print.)

**Files:** `aegis-daemon/src/main.rs`, verify `aegis-ui/src-tauri/src/lib.rs`

**Instructions:**
1. Update `aegis-daemon/src/main.rs` to print the richer stats (it currently only prints `raw_pulse`). Include bpm_10s, resp_bpm, quality — useful for headless debugging.
2. Confirm `PulsePayload` in `lib.rs` includes `quality`, `snr_db`, `resp_bpm` (it does after the engine overhaul — just verify).

**Verify:**
```bash
cargo build --release 2>&1 | grep -cE "error|warning"   # 0
```

**Commit:** `Print full vitals in daemon; confirm quality/SNR/respiration payload contract`

---

## Step 6 — Better accuracy fixture (optional, needs the user) `[ ]`

**Goal:** The current fixture is lossy (SNR ceiling). Record a lossless clip with a known reference HR so absolute accuracy becomes measurable.

**Files:** `scripts/record_test_video.py`

**Instructions:**
1. Upgrade the recorder to write **lossless**: use `cv2.VideoWriter_fourcc(*'FFV1')` and a `.mkv` container (or dump raw PNG frames). Print the true achieved fps.
2. Ask the user (this needs them):
   - to run the recorder in good, steady lighting, and
   - to simultaneously measure their pulse (fingertip oximeter or count) and tell you the value.
3. Save as `tests/test_video_lossless.mkv` (gitignored) and record the reference HR in STATE.md.
4. Re-run `scripts/bench.sh --video tests/test_video_lossless.mkv --wall-secs <measured>` and compare `prodPeak`/`refBPM` to the reference HR. Now MAE is meaningful.

**Verify:** `refSNR` should be several dB higher than the lossy fixture; `prodPeak` within ±5 BPM of the user's reference.

**If it fails / user unavailable:** leave unchecked, note in STATE.md Blockers, proceed to `P1-ui.md`. Accuracy remains covered by synthetic tests.

**Commit:** `Upgrade fixture recorder to lossless FFV1 with true-fps reporting`

---

## Definition of done for this file

- [ ] Steps 1–5 checked, `cargo build` 0 warnings, `cargo test -p aegis-core` all green, video bench within regression rules.
- [ ] STATE.md **Next up** set to `P1-ui.md`, changelog updated, `plan.md` Phase 1 backend marked complete.
- [ ] Step 6 done or explicitly deferred in Blockers.
