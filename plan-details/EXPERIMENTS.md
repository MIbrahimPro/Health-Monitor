# Aegis — Experimentation Protocol & Backlog

How to decide "which way is better" without fooling yourself. Follow this whenever tuning the signal pipeline (and adapt the same discipline for UI/perf choices).

---

## The protocol (always)

1. **One change per run.** Never compare runs that differ in two things.
2. **Name labels** `<thing>_<variant>` (e.g. `grid_2x2`, `mask_strict`). Run:
   ```bash
   cargo test --release -p aegis-core        # must stay green BEFORE benching
   scripts/bench.sh --label <thing>_<variant>
   ```
3. **Record** each SUMMARY line in a scratch table (in your working notes or a `tests/results/EXPLOG.md`, gitignored) with the exact code diff summary.
4. **Decision rule — metric priority on the current lossy fixture:**
   1. Synthetic tests all green (hard gate — a variant that breaks one is dead).
   2. `std10` lower is better (stability).
   3. `cov10` higher (≥90 % required).
   4. `prodSNR` higher.
   5. `maxJump10` lower.
   6. MAE only breaks ties (the fixture's reference is soft — see STATE.md).
   A variant must win on (2)–(4) combined WITHOUT regressing any hard gate. Improvements under ~5 % are noise on a single fixture — call those ties and prefer the simpler code.
5. **Champion bookkeeping:** if a variant wins, commit it, add its SUMMARY to the STATE.md metrics table, and it becomes the new baseline. If it loses, `git checkout -- <files>` and log one line in EXPLOG so nobody retries it blind.
6. **Offline-first iteration (cheap):** for extraction/geometry questions, do a single instrumented pass (`--dump-rois`) and analyze the CSV with numpy (python3, numpy installed; no scipy) before touching Rust. Only port the winning design to production code. Prior art: this is exactly how full-res sampling and the breathing-harmonic discovery were made — see `scripts/` history and agents.md.

## Statistical honesty on one fixture

- One 180 s video = one sample. Differences <5 % in std/SNR are not conclusions.
- Chunked evidence beats whole-file evidence: when comparing signal quality, also compare per-30 s-chunk peak consistency (a variant whose chunk peaks cluster tighter is genuinely better even at equal whole-file SNR).
- Never tune a constant to make THIS video's number pretty if it has no physical story. Every constant change needs a one-line physical justification in the commit message (e.g. "eyes blink → exclude eye band from ROI").

---

## Experiment backlog (ordered by expected value; each is optional — run when touching that area)

### E1 — Patch grid size: 3×3 (champion) vs 2×2 vs 4×4
Hypothesis: more patches = better fusion until patches get too few pixels (<~1500 px each at 640×480 ROI). Change `GRID_X/GRID_Y` in `pipeline.rs` (one commit-less edit per run). Decide by protocol; expect 4×4 to win slightly in good light and lose in dark (patch starvation → `None` patches). Also record `analyze ms/frame` (cost grows ~linearly with patches).

### E2 — ROI geometry: current upper-60 % vs forehead+cheeks composite
The `--dump-rois` data already showed (dark fixture): no candidate clearly beat upper60, and forehead (above the rustface box) is hair-risky. Re-run this experiment ONLY on the future lossless bright fixture — in good light forehead+cheeks should win (eyes/brows excluded). Implementation if it wins: sample two boxes derived from the face box ((0.22,−0.24,0.56,0.26) and the two cheek patches — exact fractions in `bench_rppg.rs::RoiDump`) instead of one, patches split across them.

### E3 — Skin mask strictness
Variants: current loose (`max>10 && R≥G && R≥B`) vs strict (`R>G+4 && R>B+4 && max>20 && R<250`) vs adaptive (threshold from ROI median). Offline first via `--dump-rois` (both masks already dumped). On the dark fixture strict lost pixels without SNR gain; retest on bright fixture where strict should reject brows/glasses better.

### E4 — POS window length: 1.6 s vs 1.2 s vs 2.4 s (`POS_WINDOW_SECS`)
Wang et al. chose 1.6 s at 20–30 fps. At our 16.6 fps, 1.6 s = 27 samples (fine). Longer = smoother but slower to react and more drift-sensitive inside the window. Expect 1.6 to hold; run only if bored or the lossless fixture disagrees.

### E5 — Tracker dynamics (`alpha`, slew ±4 BPM/tick, conf gates 0.2/0.3)
Trade: responsiveness to real HR changes (exercise recovery) vs jitter. The fixture can't test real HR swings — synthesize one: extend `synthetic.rs` with a ramp test (72→110 over 20 s, engine must follow within X s; pick X from the slew math: 38 BPM at ~4 BPM/s ≈ 10 s + filter latency). Add as a permanent test if you touch tracker constants.

### E6 — Welch segment length for the 10 s window (currently 10 s = 1 segment)
Variant: 6 s segments (3 half-overlapped) → smoother PSD, worse resolution (Δf 0.17 Hz = 10 BPM… too coarse — likely bad). Verify the intuition once: expect loss; keep single segment.

### E7 — Breathing-harmonic penalty strength (×0.5, tolerance 0.05 Hz)
Sweep 0.3/0.5/0.7 with the `rejects_breathing_harmonic_lock` test AND a new inverse test: true HR EXACTLY on a breathing harmonic (HR 64, breathing 16 → 4×16=64) must still report 64 (harmonic bonus + prior must outweigh the penalty). Add that inverse test before sweeping — it's the guard that the penalty isn't too strong.

### E8 — Respiration source: luminance (current) vs face-box y-position vs both fused
Face-box cy (already EMA-tracked in `FrameAnalyzer`) directly measures head bob. Offline: bench CSV has no cy — add a debug column first. Decide by which agrees more consistently with the fixture's 16.5 br/min across chunks (and later, with the sonar cross-check of P5 Step 3).

### E9 — Detection cadence: every 10 frames (0.6 s) vs 5 vs 15
Cost: ~28 ms/call on one core. Faster cadence = quicker face reacquisition, marginal ROI tracking gain (EMA dominates). Measure reacquisition latency after the cover-uncover smoke test + `cov10`. Only worth running if users report tracking lag.

### E10 — Live YUYV vs fixture quality (sanity, once P1-remaining Step 6 exists)
Same person, same lighting, ~same minute: record lossless fixture AND run live; compare live `snr_db` (UI) with bench `prodSNR` on the recording. Expect live ≥ recording (no codec). If live is WORSE, something in the live path corrupts frames (YUYV decode, buffer reuse bug) — investigate immediately; this comparison is the canary for live-path bugs.

### E11 — (Perf) preview JPEG quality/cadence: q55 every 5th frame (current) vs q70 every 3rd
UI smoothness vs CPU+IPC size. Measure: payload bytes/s (log base64 length × rate) and UI feel. Decide by feel at <1 MB/s budget.

### E12 — (UI) waveform renderer: canvas 2D (planned) vs WebGL
Only if the canvas renderer can't hold 60 fps on the target machine (measure with devtools). Canvas is simpler; don't preemptively WebGL.

---

## Graduation rule

An experiment that produces a winner also produces:
1. the champion SUMMARY line in STATE.md's table,
2. a synthetic test if it guarded a behavior (E5, E7 especially),
3. one line in agents.md's session log,
4. deletion of its dead code paths (no zombie flags).
