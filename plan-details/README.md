# Aegis — Execution Playbook

This folder is the **single source of truth for finishing the project**. It is written so that any developer — or a lightweight AI model — can execute it step by step without guessing.

## Files

| File | What it covers |
|---|---|
| `STATE.md` | Current state of the codebase, verified metrics, key discoveries, build/test commands. **Read first, always.** |
| `CONCEPTS.md` | Domain primer (POS, Welch, SNR, harmonics, trackers…). **Read before touching signal code.** |
| `TESTING.md` | The full testing manual: every test layer, bench CLI reference, regression gates, smoke checklists |
| `EXPERIMENTS.md` | How to decide "which way is better": A/B protocol, decision rules, and the experiment backlog E1–E12 |
| `TROUBLESHOOTING.md` | Failure playbook: build/test/camera/UI/model/git/perf trees + when and how to ask the user |
| `P1-remaining.md` | Finish Phase 1: performance polish, backend lifecycle (start/stop/tray/settings) |
| `P1-ui.md` | The premium UI overhaul (full design system + component-by-component spec) |
| `P2.md` | Vision comfort overlay, window/context tracking, phone detection |
| `P3.md` | Posture correction, emotion detection, smart music engine |
| `P4.md` | Face recognition security, keystroke/mouse biometrics |
| `P5.md` | Ultrasonic sonar, Wi-Fi CSI, air-gapped pairing |
| `P6.md` | Mobile companion + final release polish |

## The execution loop (follow this exactly)

0. First session only: read `STATE.md`, `CONCEPTS.md`, `TESTING.md` fully; skim `TROUBLESHOOTING.md` and `EXPERIMENTS.md` so you know they exist. When anything breaks → `TROUBLESHOOTING.md`. When choosing between designs/tunings → `EXPERIMENTS.md` protocol.
1. Open `STATE.md`. Find the **Next up** pointer.
2. Open the referenced phase file. Find the first unchecked `[ ]` step.
3. Read the WHOLE step before touching code: Goal → Files → Instructions → Verify → If it fails.
4. Implement exactly what the step says. Do not improvise beyond the step's "allowed variations".
5. Run the step's **Verify** block. Every command's expected output is stated. If output matches → check the box `[x]`, add one line to the changelog in `STATE.md`.
6. Commit with the step's commit message template and push:
   ```bash
   git add -A && git commit -m "<template>" && git push origin main
   ```
7. Go to 2. When a phase file has no unchecked steps, update **Next up** in `STATE.md` to the next phase file and update the top-level `plan.md` phase status.

## Hard rules

- **Never commit** if `cargo build` has warnings/errors or `cargo test -p aegis-core` fails. Fix first.
- **Never weaken a Verify block** to make it pass. If it genuinely can't pass, follow the step's "If it fails" branch; if that dead-ends, write the blocker into `STATE.md → Blockers` and move to the next independent step.
- **Commit messages**: plain, imperative, no attribution trailers of any kind (no `Co-Authored-By`, no tool footers).
- **Do not push test media**: `tests/*.avi` is gitignored (it contains the user's face). Keep it that way.
- **Rust quality bar**: zero `cargo build` warnings across the workspace at every commit.
- **When a step needs `sudo` or an interactive login**: stop and ask the user to run the exact command; do not attempt workarounds.
- Timestamps are wall-clock everywhere in the rPPG engine. Never reintroduce frame-rate assumptions (the webcam claims 30 fps but delivers ~16.6).

## Testing discipline (memorize)

```bash
# 1. Unit/accuracy tests — synthetic signals with KNOWN ground truth. Must always pass.
cargo test --release -p aegis-core

# 2. Video regression bench — stability/robustness on the recorded fixture.
scripts/bench.sh --label <short-name>
#    Compare the SUMMARY line against the table in STATE.md. Stability metrics
#    (std10, maxJump, coverage) must not regress. MAE on this video is NOT
#    absolute truth (lossy codec ceiling — see STATE.md → Key discoveries).

# 3. Live app smoke test (needs a webcam + display):
cd aegis-ui && npm run tauri dev
```
