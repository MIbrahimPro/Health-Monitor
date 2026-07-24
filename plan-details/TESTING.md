# Aegis — Testing & Validation Manual

Every claim about accuracy or performance in this project must come from one of the three test layers below. Never say "it works" from reading code.

---

## Layer 1 — Synthetic accuracy tests (ground truth known)

**File:** `aegis-core/tests/synthetic.rs` · **Run:** `cargo test --release -p aegis-core`
(`--release` matters: debug mode is ~20× slower and these process 45–90 s of samples.)

These synthesize skin-tone RGB streams at the webcam's real ~16.6 fps with a KNOWN pulse frequency, feed them through the actual engine (`PosRppg`), and assert the reported BPM. They are the only tests where the right answer is certain.

| Test | Scenario it guards | Pass criterion |
|---|---|---|
| `locks_72_bpm_clean` | Basic lock on a clean 72 BPM pulse | 10 s & 30 s windows within ±5 |
| `locks_60_bpm_with_drift` | Slow illumination drift (the classic detrend killer) | 10 s within ±5 of 60 |
| `locks_110_bpm_noisy` | Elevated HR under heavy noise (0.4 LSB) | 10 s within ±6 of 110 |
| `no_subharmonic_lock_at_84` | The historical "reads half" bug (84→42) | 10 s & 30 s within ±6 of 84 |
| `rejects_breathing_harmonic_lock` | Harmonic-rich 16 br/min breathing artifact 5× stronger than a 78 BPM pulse | 30 s within ±6 of 78 AND respiration 16±3 |

**Rules:**
- All 5 must pass at every commit. A red test is NEVER "flaky" here — the signals are deterministic (fixed LCG noise seed). A failure means the engine changed behavior.
- When you change engine constants (thresholds, penalties, windows), these tests are your first tripwire. If a change makes one fail, the change is wrong or the test tolerance genuinely needs a justified update — justify in the commit message, never silently loosen.
- **Adding a test** (do this for every new signal-domain bug found): copy the `run_synthetic` pattern; keep noise deterministic (LCG, no `rand` crate); pick tolerances with ≥2 BPM slack over observed result so tests aren't brittle; name it after the failure mode, not the number.

## Layer 2 — Video regression benchmark (stability truth, NOT accuracy truth)

**File:** `aegis-core/src/bin/bench_rppg.rs` · **Run:** `scripts/bench.sh --label <name>`

Streams `tests/test_video.avi` (640×480, 2985 frames = 180 s wall) through the EXACT production pipeline (`pipeline::FrameAnalyzer`) and grades it. Because the fixture is lossy mpeg4 recorded in the dark (see STATE.md → Key discoveries), its absolute HR is ambiguous — treat MAE with suspicion, treat stability metrics as law.

### CLI reference

| Flag | Default | Meaning |
|---|---|---|
| `--video PATH` | tests/test_video.avi | input video |
| `--wall-secs S` | 180 | real duration; fps = frames/S (fixes the lying 30 fps header) |
| `--container-fps` | — | trust the container's fps instead (do NOT use on the current fixture) |
| `--fps F` | — | manual fps override (wins over both) |
| `--label NAME` | run | tag for the CSV + SUMMARY line |
| `--csv PATH` | tests/results/NAME.csv | per-frame dump (t, rgb, pulse, bpm10/30/60, resp, quality) |
| `--detect-every N` | 10 | detection cadence in frames (mirror production = 10) |
| `--dump-rois PATH` | off | ALSO dump 5 candidate ROI geometries × 2 masks per frame (for extraction experiments — see EXPERIMENTS.md) |

### Reading the output

- `Pipeline run`: `face detected %` (should be 100 on the fixture), `detect ms/call` (~25–33), `analyze ms/frame` (≤0.5 target), `offline fps` (throughput incl. decode).
- `Reference`: full-recording POS+Welch estimate with harmonic-aware scoring; `REFERENCE RESP` ~16.5 on the fixture. On this fixture the reference itself is soft — top peaks are printed so a human can judge.
- `Production BPM timelines`: per window — `n`, `cov` (frames with output ÷ all frames), `MAE/RMSE/±5bpm` vs the reference (soft on this fixture), `std` (spread of the timeline), `maxJump` (largest step between consecutive outputs), `firstAt` (warmup latency).
- `SUMMARY <label> ...`: one greppable line — paste it into STATE.md's metrics table when it represents a committed change.

### Regression gates (current fixture — enforce at every engine-touching commit)

```
std10 ≤ 5.0      maxJump10 ≤ 2.0      cov10 ≥ 90%      firstAt10 ≤ 12s
analyze ≤ 0.5 ms/frame                offline fps ≥ 150
respiration coverage ≥ 80%, mean within 12–20
```
Current champion (label `patch_seeded`): std10 3.80, maxJump 1.36, cov10 95.5 %, analyze 0.38 ms.
A gate failure = do not commit; find the cause or revert.

## Layer 3 — Live smoke test (needs webcam + display + a human)

```bash
cd aegis-ui && npm run tauri dev
```
Checklist (2 minutes): app starts → Initialize → camera LED on → face box tracks smoothly (no snapping) → BPM appears ≤ 12 s → value plausible and steady (±3 over 30 s while sitting still) → respiration appears ≤ 30 s, plausible (8–20) → quality/SNR reasonable (dark room = lower) → waveform shows rhythmic pulses → cover the camera → "no face" state, uncover → recovers ≤ 5 s without restart. After P1-remaining: Stop → LED off → Initialize again works.

If any step needs the user (no display in session), write "deferred: <step>" in STATE.md rather than skipping silently.

## Layer 4 (future) — Lossless fixture with reference HR

Once P1-remaining Step 6 produces `tests/test_video_lossless.mkv` + a user-reported reference HR, MAE on that file becomes REAL accuracy. Then add gates: `prodPeak within ±5 of reference`, `mae30 ≤ 5`. Record the reference value and gates in STATE.md.

---

## Frontend checks

- `cd aegis-ui && npm run build` — zero TypeScript errors is a commit gate once UI work starts.
- No runtime network requests (devtools Network tab — only local assets). This is a privacy gate.

## Where results live

- `tests/results/*.csv` — per-frame dumps, gitignored, safe to delete anytime.
- Metric history — the table in `STATE.md` + `SUMMARY` lines recoverable by re-running any committed revision (`git checkout <sha> && scripts/bench.sh --label archaeology`).
