use std::path::PathBuf;

pub struct FaceRecognizer {
    enrolled: bool,
    // Would hold tract session for mobilefacenet.onnx
}

impl FaceRecognizer {
    pub fn new() -> Self {
        // Scaffolded: pretend enrolled if config file exists, otherwise false.
        Self { enrolled: false }
    }

    pub fn enroll_owner(&mut self) -> bool {
        // Scaffolded: pretend to enroll
        self.enrolled = true;
        true
    }

    pub fn clear_owner(&mut self) {
        self.enrolled = false;
    }

    pub fn is_owner_present(&self, _frame_data: &[u8], _width: u32, _height: u32, _face_x: u32, _face_y: u32, _face_w: u32, _face_h: u32) -> Option<bool> {
        if !self.enrolled {
            return None;
        }
        // Scaffolded: always return true for now.
        // TODO: Implement mobilefacenet onnx model for embeddings and cosine similarity.
        Some(true)
    }
}
