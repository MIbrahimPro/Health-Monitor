# Phase 1 — Premium UI Overhaul

**Goal:** Replace the proof-of-concept dashboard with a breathtaking, production-grade UI: refined dark palette, modern typography, smooth gradients, micro-animations, purposeful states. All local assets (no CDN — Tauri apps must work offline).

**Files:** everything under `aegis-ui/src/`, plus `aegis-ui/package.json`, `aegis-ui/index.html`.

Work top to bottom. After every step: the app must still build (`cd aegis-ui && npm run build`) with zero TypeScript errors.

---

## Design system (the law — do not deviate)

### Palette (CSS variables, define in `:root`)

```css
--bg-0: #070A0F;          /* app background — near-black blue */
--bg-1: #0C111A;          /* panel background */
--bg-2: #121A26;          /* raised card */
--stroke: rgba(148, 163, 184, 0.08);   /* hairline borders */
--stroke-strong: rgba(148, 163, 184, 0.16);
--text-hi: #E6EDF7;       /* primary text */
--text-mid: #8A97A8;      /* secondary text */
--text-low: #4E5A6B;      /* labels/hints */
--accent: #2DE0A5;        /* vital green-teal — the hero color */
--accent-2: #4E9CF5;      /* cool blue for secondary data */
--accent-warm: #F5A34E;   /* respiration amber */
--danger: #F0546C;        /* alerts / high HR */
--grad-hero: linear-gradient(135deg, #2DE0A5 0%, #4E9CF5 100%);
--glow-accent: 0 0 24px rgba(45, 224, 165, 0.35);
```
Rules: backgrounds only from `--bg-*`; never pure #000/#fff; text only the three text tokens; the gradient is used sparingly (hero number, primary button, waveform stroke). HR color logic: <60 → `--accent-2`, 60–100 → `--accent`, >100 → `--danger`.

### Typography

- UI text: **Inter** (weights 400/500/600/800). Numbers (BPM, stats): **JetBrains Mono** (weight 700) with `font-variant-numeric: tabular-nums`.
- Install locally: `npm i @fontsource/inter @fontsource/jetbrains-mono`, then in `main.tsx`:
  `import "@fontsource/inter/400.css";` (repeat for 500/600/800) and `import "@fontsource/jetbrains-mono/700.css";`
- Scale: hero BPM 72px/800 mono; card values 28px/700 mono; card labels 11px/600 Inter uppercase `letter-spacing:0.12em` color `--text-low`; body 13px/400.

### Motion (micro-animations)

- All transitions 180–260 ms, `cubic-bezier(0.22, 1, 0.36, 1)` (ease-out-quint feel).
- Numbers animate on change: CSS `transition` cannot animate text — implement a `useAnimatedNumber(value, 400ms)` hook (requestAnimationFrame lerp) and render the interpolated value rounded.
- Card entrance: fade+rise (`opacity 0→1`, `translateY(8px)→0`) staggered 60 ms via `animation-delay`.
- Heartbeat: the ♥ icon (inline SVG) scales 1→1.12→1 keyframe, duration = `60/bpm` seconds, only when a BPM is locked.
- Status dot pulses (existing pattern is fine, restyle to tokens).
- Respect `@media (prefers-reduced-motion: reduce)`: disable heartbeat/entrance animations.

---

## Step 1 — Foundation: tokens, fonts, layout shell `[x]`

**Instructions:**
1. `npm i @fontsource/inter @fontsource/jetbrains-mono` in `aegis-ui/`.
2. Rewrite `App.css` from scratch: CSS reset block, `:root` tokens above, base `body { background: var(--bg-0); color: var(--text-hi); font-family: 'Inter', system-ui, sans-serif; }`.
3. App shell grid:
   ```
   ┌ header (56px): logo-mark "AEGIS" + status chip ─── controls (Start/Stop) ┐
   ├ main grid: 12-col, 16px gap, 20px padding                                │
   │  hero HR card (span 5)   │ resp card (span 3) │ quality card (span 4)    │
   │  waveform card (span 8, min 220px)            │ camera card (span 4)     │
   └──────────────────────────────────────────────────────────────────────────┘
   ```
   Use CSS Grid (`grid-template-columns: repeat(12, 1fr)`), cards = `background: var(--bg-1); border: 1px solid var(--stroke); border-radius: 14px; padding: 20px;`.
4. Keep all existing React state/logic working — this step is layout+style only.

**Verify:** `npm run build` → 0 errors. `npm run tauri dev` visual check: fonts loaded (inspect devtools network — NO external requests), grid as sketched.
**If it fails:** fontsource import path errors → check exact package export names in `node_modules/@fontsource/inter/`.
**Commit:** `UI foundation: design tokens, local Inter/JetBrains Mono, 12-col dashboard shell`

## Step 2 — Hero heart-rate card `[ ]`

**Instructions:**
1. Component `HeroCard`: giant animated BPM (10 s tracker as primary), heartbeat ♥ SVG beating at the real rate, label "HEART RATE", sub-row showing 30 s and 60 s values as small mono chips ("30s · 72", "60s · 71").
2. Value uses `useAnimatedNumber`. Color per HR rule. Add subtle radial glow behind the number: `background: radial-gradient(closest-side, rgba(45,224,165,.12), transparent)`.
3. When no lock yet: show `--` in `--text-low` + shimmer sweep animation across the number (CSS gradient sweep, 1.8 s loop) + "Calibrating…" hint with the existing warmup logic.

**Verify:** dev run — number glides between values (no hard jumps), heart beats at displayed rate, calibration shimmer before first lock.
**Commit:** `Hero heart-rate card: animated value, live heartbeat, calibration shimmer`

## Step 3 — Respiration + signal quality cards `[ ]`

**Instructions:**
1. `RespCard`: animated breaths/min from `resp_bpm` (amber accent), tiny sine-wave SVG that undulates at the respiration rate (CSS `animation-duration = 60/resp` s), `--` state when null.
2. `QualityCard`: circular progress ring (SVG `stroke-dasharray`) 0–100 from `quality`, center shows the number; below it a mono row `SNR −3.2 dB · 16.4 FPS`. Ring color: quality <35 `--danger`, 35–65 `--accent-warm`, >65 `--accent`. Ring animates via `stroke-dashoffset` transition 400 ms.
3. Extend the `pulse-update` listener types with `resp_bpm`, `quality`, `snr_db` (already in payload).

**Verify:** dev run with face in view ≥30 s: respiration shows a plausible 10–20, ring fills as signal stabilizes.
**Commit:** `Respiration and signal-quality cards with animated ring and live SNR`

## Step 4 — Cinematic waveform `[ ]`

**Instructions:** rewrite the oscilloscope canvas renderer:
1. Render loop via `requestAnimationFrame` (not per-event), reading from `pulseHistoryRef`; devicePixelRatio-aware canvas sizing (`canvas.width = clientWidth * dpr`).
2. Style: background transparent (card provides `--bg-2`); faint dot-grid (2px dots every 24px, `rgba(148,163,184,0.06)`); waveform stroked with a horizontal gradient `--accent → --accent-2`, `lineWidth 2.5`, glow via `shadowBlur 12` in accent; leading edge = brighter 8px "comet head" dot.
3. Amplitude normalization over the visible window (existing min/max logic ok, add 10 % padding); scroll direction right→left with newest at right edge.
4. Under the canvas: left mono caption `rPPG · POS fused`, right caption live `snr_db` dB.

**Verify:** dev run — smooth ≥30 fps scroll (check no jank with devtools performance), crisp on HiDPI.
**If it fails:** canvas blurry → dpr scaling missing; jank → you're re-allocating gradients per frame, hoist them.
**Commit:** `Cinematic waveform: rAF renderer, HiDPI, gradient stroke with comet head`

## Step 5 — Camera card + status states `[ ]`

**Instructions:**
1. Camera card: the JPEG feed with `border-radius: 10px`, over it a top-right status chip: `● LIVE` (accent) / `NO FACE` (danger) / `OFF` (text-low). When `!face_found`, overlay a centered pill "Position your face in view" with slight backdrop blur (`backdrop-filter: blur(4px)`).
2. Global states: idle (start button prominent, cards show `--`), starting (button spinner), running, error (message from invoke rendered in a danger toast bar). Start button uses `--grad-hero` background, dark text, hover lift `translateY(-1px)` + glow; becomes an outlined "Stop" while running (wire to `stop_tracking` from P1-remaining Step 1).
3. Delete all leftover inline styles from App.tsx — everything through classes.

**Verify:** dev run through the full lifecycle: idle → start → no-face → face → locked → stop → restart. Every state visually distinct; zero inline styles left (`grep -n "style={{" src/App.tsx` → only the dynamic color/width bindings allowed, ideally none).
**Commit:** `Camera card status system, start/stop control states, remove inline styles`

## Step 6 — Polish pass `[ ]`

**Instructions:**
1. Entrance stagger on first mount; `prefers-reduced-motion` guards.
2. Empty-data shimmer skeletons on cards for the first 2 s.
3. Window chrome: in `tauri.conf.json` set `"title": "Aegis — Vitals"`, min size 980×640.
4. Sweep for contrast: every text/token pairing ≥ 4.5:1 (check `--text-mid` on `--bg-1`).
5. `index.html`: set `<html lang="en">`, dark `color-scheme`, app title.

**Verify:** `npm run build` 0 errors; screenshot the five states; reduced-motion honored (test by toggling OS setting or devtools emulation).
**Commit:** `UI polish: entrance stagger, skeletons, reduced-motion, window chrome`

---

## Definition of done

- [ ] Steps 1–6 checked, `npm run build` clean, lifecycle smoke test passes.
- [ ] No external network requests at runtime (devtools network tab empty besides local assets).
- [ ] STATE.md Next up → `P2.md`; changelog + `plan.md` Phase 1 marked ✅ complete.
