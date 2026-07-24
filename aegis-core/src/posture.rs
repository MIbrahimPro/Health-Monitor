use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostureState {
    Good,
    TooClose,
    Slouching,
}

impl PostureState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PostureState::Good => "Good",
            PostureState::TooClose => "TooClose",
            PostureState::Slouching => "Slouching",
        }
    }
}

pub struct PostureMonitor {
    h0: f32,
    cy0: f32,
    calibration_samples: Vec<(f32, f32)>,
    calibrated: bool,
    
    // Ring buffers for sustained conditions (at 1 fps roughly, we store last 35 seconds)
    too_close_history: VecDeque<bool>,
    slouch_history: VecDeque<bool>,
    
    current_state: PostureState,
}

impl PostureMonitor {
    pub fn new() -> Self {
        Self {
            h0: 0.0,
            cy0: 0.0,
            calibration_samples: Vec::with_capacity(30),
            calibrated: false,
            too_close_history: VecDeque::with_capacity(35),
            slouch_history: VecDeque::with_capacity(35),
            current_state: PostureState::Good,
        }
    }

    pub fn recalibrate(&mut self) {
        self.calibrated = false;
        self.calibration_samples.clear();
        self.current_state = PostureState::Good;
        self.too_close_history.clear();
        self.slouch_history.clear();
    }

    pub fn process_frame(&mut self, face_h: f32, face_cy: f32, frame_h: f32) -> PostureState {
        if !self.calibrated {
            self.calibration_samples.push((face_h, face_cy));
            if self.calibration_samples.len() >= 30 {
                // Compute median
                let mut heights: Vec<f32> = self.calibration_samples.iter().map(|s| s.0).collect();
                let mut cys: Vec<f32> = self.calibration_samples.iter().map(|s| s.1).collect();
                
                heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                cys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                
                self.h0 = heights[heights.len() / 2];
                self.cy0 = cys[cys.len() / 2];
                self.calibrated = true;
            }
            return PostureState::Good;
        }

        let dist_ratio = face_h / self.h0;
        let drop_ratio = (face_cy - self.cy0) / frame_h;

        let is_too_close = dist_ratio > 1.28;
        let is_slouching = drop_ratio > 0.06 && dist_ratio > 1.10;

        // Push to history
        self.too_close_history.push_back(is_too_close);
        if self.too_close_history.len() > 25 {
            self.too_close_history.pop_front();
        }

        self.slouch_history.push_back(is_slouching);
        if self.slouch_history.len() > 35 {
            self.slouch_history.pop_front();
        }

        // Evaluate sustained conditions
        // TooClose: >= 20 of last 25
        let too_close_count = self.too_close_history.iter().filter(|&&x| x).count();
        // Slouching: >= 30 of last 35
        let slouch_count = self.slouch_history.iter().filter(|&&x| x).count();

        if slouch_count >= 30 {
            self.current_state = PostureState::Slouching;
        } else if too_close_count >= 20 {
            self.current_state = PostureState::TooClose;
        } else {
            // Need to recover if we were bad.
            // Let's say we need < 5 true in the window to recover to Good.
            if self.current_state == PostureState::Slouching && slouch_count < 5 {
                self.current_state = PostureState::Good;
            } else if self.current_state == PostureState::TooClose && too_close_count < 5 {
                self.current_state = PostureState::Good;
            } else if self.current_state != PostureState::Good && slouch_count < 5 && too_close_count < 5 {
                self.current_state = PostureState::Good;
            }
        }

        self.current_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posture_state_machine() {
        let mut monitor = PostureMonitor::new();
        
        // Calibration (30 frames)
        for _ in 0..30 {
            assert_eq!(monitor.process_frame(100.0, 200.0, 1080.0), PostureState::Good);
        }
        assert!(monitor.calibrated);
        assert_eq!(monitor.h0, 100.0);
        assert_eq!(monitor.cy0, 200.0);

        // Stay good
        for _ in 0..30 {
            assert_eq!(monitor.process_frame(100.0, 200.0, 1080.0), PostureState::Good);
        }

        // Lean in (dist_ratio = 1.3 > 1.28)
        for i in 0..25 {
            let state = monitor.process_frame(130.0, 200.0, 1080.0);
            if i < 19 {
                assert_eq!(state, PostureState::Good);
            } else {
                assert_eq!(state, PostureState::TooClose);
            }
        }

        // Recover
        for i in 0..25 {
            let state = monitor.process_frame(100.0, 200.0, 1080.0);
            if i < 20 { // Need count < 5 to recover
                assert_eq!(state, PostureState::TooClose);
            } else {
                assert_eq!(state, PostureState::Good);
            }
        }
        
        // Slouch (drop_ratio > 0.06 => drop > 64.8px, dist > 1.10 => h > 110.0)
        for i in 0..35 {
            let state = monitor.process_frame(115.0, 270.0, 1080.0);
            if i < 29 {
                assert_eq!(state, PostureState::Good);
            } else {
                assert_eq!(state, PostureState::Slouching);
            }
        }
    }
}
