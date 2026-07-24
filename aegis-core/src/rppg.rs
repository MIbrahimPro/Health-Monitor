use ndarray::prelude::*;
use rustfft::{FftPlanner, num_complex::Complex};
use std::collections::VecDeque;

/// The POS (Plane-Orthogonal-to-Skin) rPPG Algorithm implementation.
/// Uses actual timestamps for accurate BPM calculation regardless of variable FPS.
pub struct PosRppg {
    window_size: usize,
    buffer_r: VecDeque<f32>,
    buffer_g: VecDeque<f32>,
    buffer_b: VecDeque<f32>,
    buffer_times: VecDeque<f64>, // elapsed seconds since start

    // For BPM calculation
    pulse_buffer: VecDeque<f32>,
    pulse_times: VecDeque<f64>,

    bpm_10s_history: VecDeque<f32>,
    bpm_30s_history: VecDeque<f32>,
    bpm_60s_history: VecDeque<f32>,

    pub smoothed_bpm_10s: Option<f32>,
    pub smoothed_bpm_30s: Option<f32>,
    pub smoothed_bpm_60s: Option<f32>,
}

impl PosRppg {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer_r: VecDeque::with_capacity(window_size + 1),
            buffer_g: VecDeque::with_capacity(window_size + 1),
            buffer_b: VecDeque::with_capacity(window_size + 1),
            buffer_times: VecDeque::with_capacity(window_size + 1),
            pulse_buffer: VecDeque::with_capacity(1200), // ~120 seconds max at 10fps
            pulse_times: VecDeque::with_capacity(1200),
            bpm_10s_history: VecDeque::with_capacity(45),
            bpm_30s_history: VecDeque::with_capacity(45),
            bpm_60s_history: VecDeque::with_capacity(45),
            smoothed_bpm_10s: None,
            smoothed_bpm_30s: None,
            smoothed_bpm_60s: None,
        }
    }
    pub fn process_frame(&mut self, r: f32, g: f32, b: f32, elapsed_secs: f64) -> (f32, Option<f32>, Option<f32>, Option<f32>) {
        // Push into POS window
        if self.buffer_r.len() >= self.window_size {
            self.buffer_r.pop_front();
            self.buffer_g.pop_front();
            self.buffer_b.pop_front();
            self.buffer_times.pop_front();
        }
        self.buffer_r.push_back(r);
        self.buffer_g.push_back(g);
        self.buffer_b.push_back(b);
        self.buffer_times.push_back(elapsed_secs);

        if self.buffer_r.len() < self.window_size {
            return (0.0, None, None, None);
        }

        // POS algorithm
        let r_arr = Array::from_iter(self.buffer_r.iter().copied());
        let g_arr = Array::from_iter(self.buffer_g.iter().copied());
        let b_arr = Array::from_iter(self.buffer_b.iter().copied());

        let r_mean = r_arr.mean().unwrap_or(1.0).max(0.001);
        let g_mean = g_arr.mean().unwrap_or(1.0).max(0.001);
        let b_mean = b_arr.mean().unwrap_or(1.0).max(0.001);

        let r_n = &r_arr / r_mean;
        let g_n = &g_arr / g_mean;
        let b_n = &b_arr / b_mean;

        let x = 3.0 * &r_n - 2.0 * &g_n;
        let y = 1.5 * &r_n + &g_n - 1.5 * &b_n;

        // Use the primary chrominance channel (3R - 2G) which contains the strongest
        // pulsatile component. We skip the alpha * Y subtraction because in a sliding window
        // the alpha value fluctuates rapidly, creating massive low-frequency amplitude modulation
        // that destroys the FFT. Illumination drift is instead handled by our IIR high-pass filter.
        let h = x;

        let pulse_val = *h.last().unwrap_or(&0.0);

        // Push into FFT buffer
        self.pulse_buffer.push_back(pulse_val);
        self.pulse_times.push_back(elapsed_secs);

        // Retain up to 65 seconds
        while let Some(&t) = self.pulse_times.front() {
            if elapsed_secs - t > 65.0 {
                self.pulse_times.pop_front();
                self.pulse_buffer.pop_front();
            } else {
                break;
            }
        }

        // Calculate for each window
        let bpm10 = self.update_window(10.0, elapsed_secs);
        if let Some(b) = bpm10 { self.smoothed_bpm_10s = Some(b); }

        let bpm30 = self.update_window(30.0, elapsed_secs);
        if let Some(b) = bpm30 { self.smoothed_bpm_30s = Some(b); }

        let bpm60 = self.update_window(60.0, elapsed_secs);
        if let Some(b) = bpm60 { self.smoothed_bpm_60s = Some(b); }

        (pulse_val, self.smoothed_bpm_10s, self.smoothed_bpm_30s, self.smoothed_bpm_60s)
    }

    fn update_window(&mut self, window_secs: f64, current_t: f64) -> Option<f32> {
        let start_t = current_t - window_secs;
        
        let mut window_pulse = Vec::new();
        let mut window_times = Vec::new();

        for (i, &t) in self.pulse_times.iter().enumerate() {
            if t >= start_t {
                window_pulse.push(self.pulse_buffer[i]);
                window_times.push(t);
            }
        }

        if window_times.len() < 30 || (window_times.last().unwrap() - window_times.first().unwrap()) < window_secs * 0.8 {
            return None; // Not enough data
        }

        let raw_bpm = Self::calculate_bpm(&window_pulse, &window_times);

        if raw_bpm >= 40.0 && raw_bpm <= 180.0 {
            let history = if window_secs == 10.0 { &mut self.bpm_10s_history }
                          else if window_secs == 30.0 { &mut self.bpm_30s_history }
                          else { &mut self.bpm_60s_history };
                          
            let smoothed_bpm = if window_secs == 10.0 { self.smoothed_bpm_10s }
                               else if window_secs == 30.0 { self.smoothed_bpm_30s }
                               else { self.smoothed_bpm_60s };

            if history.len() >= 45 {
                history.pop_front();
            }
            history.push_back(raw_bpm);

            let mut sorted: Vec<f32> = history.iter().copied().collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let median_bpm = sorted[sorted.len() / 2];

            Some(match smoothed_bpm {
                Some(prev) => 0.95 * prev + 0.05 * median_bpm,
                None => median_bpm,
            })
        } else {
            None
        }
    }

    fn calculate_bpm(pulse_buffer: &[f32], pulse_times: &[f64]) -> f32 {
        // Resample evenly at 15 FPS to fix non-uniform camera framerate noise
        let target_fps = 15.0;
        let t_start = *pulse_times.first().unwrap();
        let t_end = *pulse_times.last().unwrap();
        
        let mut uniform_pulse = Vec::new();
        let dt = 1.0 / target_fps as f64;
        let mut current_t = t_start;

        let mut idx = 0;
        while current_t <= t_end && idx < pulse_times.len() - 1 {
            while idx < pulse_times.len() - 1 && pulse_times[idx + 1] < current_t {
                idx += 1;
            }
            if idx < pulse_times.len() - 1 {
                let t0 = pulse_times[idx];
                let t1 = pulse_times[idx + 1];
                let v0 = pulse_buffer[idx];
                let v1 = pulse_buffer[idx + 1];
                
                let weight = (current_t - t0) / (t1 - t0).max(0.0001);
                let v = v0 + (v1 - v0) * weight as f32;
                uniform_pulse.push(v);
            }
            current_t += dt;
        }

        let n = uniform_pulse.len();
        if n < 10 {
            return 0.0;
        }

        // 1. High-pass filter (cutoff ~0.5 Hz) to remove DC and slow drift
        // Using a 1st-order IIR filter: y[i] = alpha * (y[i-1] + x[i] - x[i-1])
        let fc = 0.5; // cutoff frequency in Hz
        let rc = 1.0 / (2.0 * std::f64::consts::PI * fc);
        let dt_filter = 1.0 / target_fps as f64;
        let alpha = (rc / (rc + dt_filter)) as f32;

        let mut detrended = Vec::with_capacity(n);
        if n > 0 {
            detrended.push(0.0);
            for i in 1..n {
                let y_prev = detrended[i - 1];
                let x_curr = uniform_pulse[i];
                let x_prev = uniform_pulse[i - 1];
                detrended.push(alpha * (y_prev + x_curr - x_prev));
            }
        }

        let n_padded = 2048; // Zero-pad for extreme frequency resolution
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n_padded);

        // Apply Hamming window and zero-mean on detrended signal
        let mean_pulse: f32 = detrended.iter().sum::<f32>() / n as f32;

        let mut buffer: Vec<Complex<f32>> = vec![Complex { re: 0.0, im: 0.0 }; n_padded];
        for i in 0..n {
            let val = detrended[i];
            let hamming = 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos();
            buffer[i] = Complex { re: (val - mean_pulse) * hamming, im: 0.0 };
        }

        fft.process(&mut buffer);

        // Find peak frequency in [0.75 Hz, 2.5 Hz] → [45 BPM, 150 BPM]
        let min_idx = (0.75 * n_padded as f32 / target_fps).ceil() as usize;
        let max_idx = (2.50 * n_padded as f32 / target_fps).floor() as usize;

        let mut max_amp = 0.0;
        let mut peak_idx = min_idx;

        for i in min_idx..=max_idx.min(n_padded / 2) {
            let amp = buffer[i].norm();
            if amp > max_amp {
                max_amp = amp;
                peak_idx = i;
            }
        }

        let peak_freq = peak_idx as f32 * target_fps / n_padded as f32;
        peak_freq * 60.0
    }
}
