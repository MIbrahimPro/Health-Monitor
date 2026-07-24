use std::f32::consts::PI;

pub struct AudioFeatures {
    pub bpm: f32,
    pub energy: f32,
    pub brightness: f32,
    pub tag: String,
}

pub fn analyze_samples(samples: &[f32], sample_rate: u32) -> AudioFeatures {
    // 1. Energy: RMS
    let mut sum_sq = 0.0;
    for &s in samples {
        sum_sq += s * s;
    }
    let rms = if samples.is_empty() { 0.0 } else { (sum_sq / samples.len() as f32).sqrt() };
    
    // 2. Brightness: Very rough proxy using zero-crossing rate instead of spectral centroid to save STFT overhead.
    let mut zcr = 0;
    for i in 1..samples.len() {
        if (samples[i] > 0.0 && samples[i - 1] < 0.0) || (samples[i] < 0.0 && samples[i - 1] > 0.0) {
            zcr += 1;
        }
    }
    let zcr_rate = if samples.is_empty() { 0.0 } else { zcr as f32 / samples.len() as f32 };
    // Normalize zcr_rate somewhat to 0..1 (max possible ZCR is 1.0, typical music is <0.2)
    let brightness = (zcr_rate * 5.0).clamp(0.0, 1.0);

    // 3. Tempo: Time-domain envelope autocorrelation
    // Extract envelope using simple low-pass of absolute values
    let mut env = Vec::with_capacity(samples.len() / 256);
    let mut acc = 0.0;
    for (i, &s) in samples.iter().enumerate() {
        acc = acc * 0.99 + s.abs() * 0.01;
        if i % 256 == 0 {
            env.push(acc);
        }
    }

    let env_sr = sample_rate as f32 / 256.0;
    let min_lag = (env_sr * 60.0 / 180.0) as usize; // 180 BPM
    let max_lag = (env_sr * 60.0 / 60.0) as usize;  // 60 BPM

    let mut best_lag = min_lag;
    let mut max_auto = 0.0;

    for lag in min_lag..=max_lag {
        let mut auto = 0.0;
        for i in 0..env.len().saturating_sub(lag) {
            auto += env[i] * env[i + lag];
        }
        if auto > max_auto {
            max_auto = auto;
            best_lag = lag;
        }
    }

    let bpm = if best_lag > 0 {
        (env_sr * 60.0) / best_lag as f32
    } else {
        120.0
    };

    // 4. Tagging
    let tag = if bpm < 90.0 && rms < 0.3 {
        "Calm"
    } else if bpm >= 90.0 && bpm <= 120.0 && brightness < 0.5 {
        "Focus"
    } else if bpm > 120.0 && rms > 0.5 {
        "Energetic"
    } else if bpm < 100.0 && brightness < 0.3 {
        "Sad"
    } else if bpm > 115.0 && brightness > 0.55 {
        "Motivational"
    } else {
        "Focus"
    };

    AudioFeatures {
        bpm,
        energy: rms.clamp(0.0, 1.0),
        brightness,
        tag: tag.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_estimation() {
        // Generate a 120 BPM click track
        let sr = 44100;
        let duration = 5; // seconds
        let mut samples = vec![0.0f32; sr * duration];
        
        // 120 BPM = 2 beats per second = 1 beat every 0.5s = 1 beat every 22050 samples
        let beat_len = sr / 2;
        
        for i in 0..(sr * duration) {
            if i % beat_len < 1000 {
                samples[i] = 1.0;
            }
        }
        
        let features = analyze_samples(&samples, sr as u32);
        println!("BPM: {}", features.bpm);
        assert!((features.bpm - 120.0).abs() < 5.0);
    }
}
