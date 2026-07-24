# Aegis — Troubleshooting Playbook

When something breaks, find the symptom here and follow the tree. If you exhaust a tree, write the dead end into `STATE.md → Blockers` (symptom, what you tried, exact error) and move to the next independent step — do not thrash.

---

## 1. Build failures

**`cargo build` errors after your edit** → read the FIRST error only (later ones cascade). Fix it, rebuild. If you can't fix it in 3 attempts: `git stash` your change, confirm clean build, re-apply in smaller pieces (`git stash pop`).

**Signature change broke other crates** → the workspace has 3 dependents of `aegis-core`: `aegis-daemon/src/main.rs`, `aegis-ui/src-tauri/src/lib.rs`, `aegis-core/src/bin/*.rs`, plus tests in `aegis-core/tests/`. Grep for the function name across all: `grep -rn "start_camera_loop\|process_frame\|process_patches" --include="*.rs" | grep -v target/`.

**Warnings present** → commit gate is zero warnings. `cargo build --release 2>&1 | grep warning` must be empty. Unused code you intend to use next step: don't commit half-wired code; finish the wiring in the same commit.

**Tauri build fails but core builds** → usually a feature flag (`tray-icon`) or `tauri.conf.json` schema issue. Check the exact `tauri` version: `grep '^version' -A1 Cargo.lock | grep -A1 '"tauri"'` and match its docs. WebKitGTK system deps missing errors (`javascriptcoregtk`, `libsoup`) → ask the user to install distro packages (needs sudo): `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf` (Debian/Ubuntu naming; adjust for the actual distro).

## 2. Test failures

**A synthetic test fails after an engine edit** → your edit changed signal behavior. Diagnose which stage: temporarily print inside the failing test (`cargo test --release -p aegis-core failing_name -- --nocapture`). Compare: does the engine produce ANY bpm (warmup broken?) vs a WRONG bpm (selection broken?) vs unstable (tracker broken?). Bisect your diff by reverting half of it. Never loosen the tolerance to pass.

**Tests pass but bench regresses** → the change hurts on realistic noise. Check WHERE via the CSV: plot/inspect `bpm10` over `t` (numpy quick-look):
```bash
python3 -c "
import csv,statistics
rows=list(csv.DictReader(open('tests/results/<label>.csv')))
v=[(float(r['t']),float(r['bpm10'])) for r in rows if r['bpm10']]
print('first',v[0],'last',v[-1]); print('std',statistics.pstdev([x[1] for x in v]))
import itertools
for k,g in itertools.groupby(v,key=lambda x:int(x[0]//30)): 
    g=list(g); print(f'{k*30}-{k*30+30}s mean',round(sum(x[1] for x in g)/len(g),1))
"
```
If a specific time chunk breaks (e.g. after 150 s — subject moved), your change is motion-fragile.

**Bench dies with "only N frames decoded"** → ffmpeg/pipe issue: run `ffmpeg -v error -i tests/test_video.avi -f null -` to check the file; check disk space; check you're at repo root (model path resolution is cwd-relative).

## 3. Camera problems (live app)

**"Camera init failed" in `logs/aegis_run_*.log`** →
1. Device exists? `ls -l /dev/video*`. Permissions? user in `video` group (`groups`). If not: user runs `sudo usermod -aG video $USER` + relogin.
2. Busy? `fuser /dev/video0` — another app (or a zombie aegis process: `pkill -f aegis`) holds it.
3. Format rejected? Some cams lack 640×480 YUYV. Probe: `v4l2-ctl --list-formats-ext -d /dev/video0` (package `v4l-utils`; ask user to install if missing). If 640×480 YUYV missing, use the nearest listed YUYV resolution in `camera.rs` — and note it in STATE.md. Avoid MJPEG (historic segfault — agents.md Bug #1).

**Camera LED on but "NO FACE" forever** → check the log for detection thread lines; verify `models/seeta_fd_frontal_v1.0.bin` exists and cwd is the repo/app dir (`find_face_model` tries `models/`, `../models/`, `../../models/`). Test detection offline: `scripts/bench.sh --label facecheck` — if faces detect at 100 % there, the model+detector are fine and the live gray/downscale path is suspect.

**FPS far below 16** → exposure-limited in the dark (normal: as low as ~8 in blackness) OR the capture thread is blocked — check that nothing heavy was added to the hot loop (JPEG every 5th, detection non-blocking `try_send` only).

**BPM wildly wrong live but bench is fine** → see EXPERIMENTS E10 canary procedure. Usually a units/timestamp bug (elapsed seconds vs frame count) or buffer-reuse bug introduced in the perf step.

## 4. rPPG output debugging tree ("engine outputs nothing / garbage")

Trace stage by stage — each has an observable:
1. **Samples reaching the engine?** `mean_rgb` column non-empty in bench CSV (or add a temp log in live). If empty: face box → ROI → mask starved (all patches < 25 px). Check ROI on the preview (green box sane?).
2. **Pulse forming?** CSV `pulse` column non-zero after ~2 s. If flat 0: POS window never reaches 8 samples → timestamps broken (gaps > 1.5 s resetting constantly — log `reset_signal` calls).
3. **Estimators running?** `bpm10` appears by ~9–12 s (needs 8 s span). If never: window span/min-sample guards failing → frame rate lower than you think (check `fps` in stats).
4. **Tracker gated?** quality stays ~0 and bpm null → every measurement below conf 0.3 (SNR < ~−3.3 dB). That's a signal-quality problem, not a code bug: more light, steadier subject, check mask/ROI.
5. **Wrong-but-stable value** → peak selection picking an artifact: check respiration estimate (is the value ≈ 2×, 3×, or 4× resp?), check the candidate scores by temporarily logging `cands` in `update_from_psd`.

## 5. Tauri / UI runtime issues

**`npm run tauri dev` fails to start** → `npm install` first; node ≥18 (have v24). Port clash (1420) → kill stale vite: `pkill -f vite`.
**Blank window** → devtools (right-click → inspect) console errors; TS build errors show in the terminal.
**Events not arriving in React** → verify the event name string `pulse-update` matches emit side; verify `start_tracking` invoked (status text). Add a temporary `console.log(payload)` in the listener; remove before commit.
**Wayland quirks (transparency/always-on-top/click-through)** → see P2 Step 1 "If it fails". Feature-gate by `XDG_SESSION_TYPE`, never hack the compositor.

## 6. Model files (ONNX)

Missing model → every model-dependent module must degrade to "Unavailable — file missing at models/<name>" (design rule). Ask the user to download (give exact filename + expected size). tract load errors (unsupported op) → try the older-opset alternative listed in the step; if none works, blocked-note and move on. NEVER vendor models into git if >10 MB without asking the user (repo bloat).

## 7. Git recovery

- Bad uncommitted experiment → `git checkout -- <files>` (surgical) or `git stash` (keep it retrievable).
- Bad commit not yet pushed → `git reset --soft HEAD~1`, fix, recommit.
- Bad commit already pushed → **do not force-push**; make a fix-forward commit (`Revert/Fix: ...`). History rewriting needs explicit user approval.
- Merge conflict on pull → shouldn't happen (single writer); if it does, `git pull --rebase` and resolve keeping the pushed remote truth for files you didn't change.

## 8. Performance regressions

`analyze ms/frame` above gate → find the new cost: comment-toggle recent additions and re-bench (bench isolates analyze vs detect vs decode timings for you). Common causes: per-frame allocation in the loop, accidental clone of the frame buffer, FFT plan re-creation (plans are cached in `FftPlanner` — keep one instance).

## 9. When to ask the user (and exactly how)

Ask (they answer quickly) when: a command needs sudo/install; a model/file must be downloaded; a physical action is needed (record video, hold up phone, second person); a decision is flagged `DECISION NEEDED` in a phase file; ground-truth values (their resting HR). Format: one short message — what you need, the exact command/file, what you'll do with it. Continue other steps while waiting; never idle-block.
