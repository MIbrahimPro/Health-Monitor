use std::path::PathBuf;

pub struct LibraryScanner {
    music_dir: PathBuf,
}

impl LibraryScanner {
    pub fn new(music_dir: PathBuf) -> Self {
        Self { music_dir }
    }

    pub fn scan(&self) {
        // Scaffolded for Phase 3 Step 4.
        // Would normally:
        // 1. Walk music_dir
        // 2. Decode with symphonia
        // 3. Call analyze_samples
        // 4. Store in SQLite
        println!("Scanning music directory: {:?}", self.music_dir);
    }
}
