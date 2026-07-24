pub enum Availability {
    Available,
    MissingDriver(String),
    UnsupportedHardware(String),
}

pub trait CsiSource {
    fn probe() -> Availability;
    fn start(&mut self);
}

pub struct NexmonSource {
    // Scaffolded for Phase 5 Step 4
}

impl CsiSource for NexmonSource {
    fn probe() -> Availability {
        // Honest unavailable state
        Availability::UnsupportedHardware("No nexmon-patched Broadcom NIC found".to_string())
    }

    fn start(&mut self) {
        // Would read UDP format here
    }
}
