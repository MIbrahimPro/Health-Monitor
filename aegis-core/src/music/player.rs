pub struct SmartPlayer {
    // Scaffolded for Phase 3 Step 5.
    // Would hold rodio OutputStream and Sinks.
}

impl SmartPlayer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn play_tag(&mut self, _tag: &str) {
        println!("SmartPlayer: playing tag {}", _tag);
    }
}
