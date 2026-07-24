use aegis_core::config::Config;
use std::env;
use std::fs;

#[test]
fn test_config_roundtrip_and_defaults() {
    let _ = fs::create_dir_all("tests/temp");
    env::set_var("XDG_CONFIG_HOME", "tests/temp");

    // Clean up
    let path = Config::config_path();
    let _ = fs::remove_file(&path);

    // Should load defaults
    let default_cfg = Config::load();
    assert_eq!(default_cfg.camera_module, true);
    assert_eq!(default_cfg.show_vitals, true);
    assert_eq!(default_cfg.overlay_module, false);

    // Modify and save
    let mut modified = default_cfg.clone();
    modified.camera_module = false;
    modified.overlay_module = true;
    modified.save().expect("Failed to save config");

    // Load and verify
    let loaded = Config::load();
    assert_eq!(loaded.camera_module, false);
    assert_eq!(loaded.overlay_module, true);

    // Corrupt file
    fs::write(&path, "{ garbage").unwrap();
    let recovered = Config::load();
    assert_eq!(recovered.camera_module, true); // Fell back to defaults
}
