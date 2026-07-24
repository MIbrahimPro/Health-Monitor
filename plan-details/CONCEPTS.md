# Aegis — Concepts Primer (read before touching signal code)

Short, precise definitions of every domain concept used in this codebase, so implementation follows math, not vibes. Formulas use per-window arrays; `mean()`/`std()` are over the window.

## rPPG (remote photoplethysmography)
Each heartbeat pushes blood into facial skin; hemoglobin absorbs light, so skin reflectance dips rhythmically by ~0.5–1 % — strongest in green. A camera averaging thousands of skin pixels can recover this tiny oscillation. Everything else (motion, lighting, sensor noise, compression) is bigger than the signal; the whole pipeline exists to reject those.

## Spatial averaging & why patches
Averaging N independent-noise pixels reduces noise ∝ 1/√N (why we sample at 640×480: 4× pixels = 2× SNR). We use a **3×3 patch grid** instead of one mean because the pulse is *coherent* across skin regions while noise/artifacts are patch-local — computing a spectrum per patch and fusing them (weighted by each patch's SNR) reinforces the common pulse peak and averages away patch noise.

## Skin mask
Per-pixel gate before averaging: `max(R,G,B) > 10 AND R ≥ G AND R ≥ B`. Skin under most lighting is red-dominant; hair/shadows are near-black; white walls/windows have R≈G≈B or B-dominant. Fallback to unmasked mean when the mask starves (dark rooms) — a biased mean beats no signal.

## Temporal normalization
Divide each channel by its window mean: `Cn = C / mean(C)`. Removes dependence on absolute brightness and skin tone; what remains is fractional variation (the pulse is a fraction, ~0.005).

## POS (Plane-Orthogonal-to-Skin, Wang et al. 2016)
Project normalized RGB onto two axes chosen to cancel intensity changes (motion/lighting move R,G,B together; pulse moves them differently):
```
S1 = Gn − Bn
S2 = −2·Rn + Gn + Bn
h  = S1 + α·S2,   α = std(S1)/std(S2)
```
α adapts the mix so residual intensity noise cancels. Computed over a short window (1.6 s) because normalization/α are only locally valid.

## Overlap-add (and the "alpha flutter" it fixes)
Naively taking one output sample per sliding window makes the output amplitude wobble as α changes window-to-window (historic bug: this "flutter" modulated the signal and wrecked the FFT). Instead, for every window position compute the whole windowed `h`, subtract its mean, and **add** it into an output buffer at its time position. Each output sample accumulates contributions from ~L overlapping windows → smooth, flutter-free pulse. Incremental version: on each new frame, process just the newest window and add into the trailing L samples.

## CHROM (de Haan 2013) — for context
Earlier cousin of POS: `X = 3Rn − 2Gn`, `Y = 1.5Rn + Gn − 1.5Bn`, `S = X − (std X/std Y)·Y`. The pre-overhaul code used `X` alone (no α) to dodge flutter — which is why it was noise-prone. POS+overlap-add supersedes it here.

## Uniform resampling
FFTs assume evenly spaced samples; the camera delivers jittery ~16.6 fps timestamps. We linearly interpolate the pulse onto a fixed 20 Hz grid using wall-clock timestamps before any spectral step. **Never index by frame count as if it were time.**

## Biquad filters / Butterworth bandpass
A biquad is a 2nd-order IIR filter `y[n] = b0x[n]+b1x[n−1]+b2x[n−2] − a1y[n−1] − a2y[n−2]` (RBJ cookbook coefficients). We cascade a high-pass at 0.7 Hz and a low-pass at 3.0 Hz (Butterworth Q=1/√2 = maximally flat) to keep only 42–180 BPM. IIR filters have a startup transient; we pre-charge state with a reflected copy of the signal's start ("reflect padding"). The offline reference runs forward-then-backward (zero phase); live code runs forward only.

## PSD, Welch's method
Power Spectral Density = signal power per frequency. Welch: split the window into 50 %-overlapping segments, Hann-window each (reduces spectral leakage), FFT (we zero-pad 4× for finer frequency grid — interpolation, not new information), average the squared magnitudes → much less noisy spectrum than one big FFT. Frequency step `df = fs/nfft`; BPM = Hz × 60.

## Harmonics, subharmonics, and the two classic traps
A periodic pulse at f₀ shows spectral peaks at f₀, 2f₀ (harmonic)…
- **Subharmonic lock** (old bug "reads 40–50"): picking f₀/2 when detrending notched out f₀. Guard: `no_subharmonic_lock_at_84` test.
- **Breathing-harmonic lock**: breathing (≈0.27 Hz, non-sinusoidal head/body motion) has harmonics at 2×,3×,4× that land INSIDE the HR band (3×16 br/min = 48 "BPM"). Guard: estimate respiration, penalize candidates within 0.05 Hz of k·f_resp (k≥2), and reward candidates whose OWN 2nd harmonic exists (a real pulse has one).

## de Haan SNR
`SNR = 10·log10( P(f₀±0.1 Hz ∪ 2f₀±0.2 Hz) / P(rest of 0.7–3 Hz band) )` in dB. Our quality metric: −10 = garbage, −3 ≈ usable, 0+ = good, +5 = excellent. Confidence mapping: `sigmoid((SNR+2)/1.5)`.

## Tracking (slew-limited, confidence-gated)
Raw per-window peak picks jump; the tracker smooths: measurement confidence gates whether we update at all (<0.2 hold), update rate α scales with confidence (0.10–0.50), and each 0.5 s tick moves at most ±4 BPM (≈8 BPM/s — faster than physiology needs, slow enough to kill glitches). Longer windows warm-seed from confident shorter ones.

## EMA (exponential moving average)
`s ← λ·s + (1−λ)·x`. Used for the face box (λ=0.9 per frame → box glides, ROI stable) and various confidences. Larger λ = smoother/laggier.

## Respiration estimation
Breathing moves the head/torso ⇒ low-frequency oscillation in ROI luminance. Band-limit 0.13–0.55 Hz (8–33 br/min), Welch, peak = breaths/min; confidence = peak's share of band power. Also feeds the breathing-harmonic penalty above.

## Concepts arriving in later phases
- **Cosine similarity** (P4): `a·b/(|a||b|)` between L2-normalized face embeddings; same person ≳0.55 with our no-alignment crop.
- **Jensen-Shannon divergence** (P4): symmetric, bounded [0,1] distance between probability histograms; our typing-rhythm anomaly score.
- **Goertzel algorithm** (P5): O(N) single-frequency DFT power — ideal for FSK demodulation at two known tones.
- **Doppler sidebands** (P5): motion toward/away from a 19 kHz carrier shifts reflections by ±Δf ∝ velocity; room motion appears as energy 15–120 Hz away from the carrier.
- **FSK** (P5): encode bits as two frequencies (18.2/19.4 kHz), 50 ms/bit, preamble for sync, CRC16 for integrity.
- **Letterbox + NMS** (P2 phone detection): resize-with-padding to the model's square input; Non-Max Suppression drops overlapping duplicate boxes (keep highest score, remove IoU>0.45 neighbors).
