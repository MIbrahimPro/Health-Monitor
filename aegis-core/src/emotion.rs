// Scaffolded for Phase 3 - Step 3: Facial emotion recognition

pub struct EmotionMonitor {
    // Scaffolded: would hold tract session here
}

impl EmotionMonitor {
    pub fn new() -> Self {
        Self {}
    }

    pub fn process_frame(&mut self, _frame_data: &[u8], _width: u32, _height: u32, _face_x: u32, _face_y: u32, _face_w: u32, _face_h: u32) -> Option<String> {
        // TODO: Implement actual tract-onnx inference using emotion-ferplus-8.onnx.
        // Deferred because model file needs to be downloaded by user.
        Some("Neutral".to_string())
    }
}
