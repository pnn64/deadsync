use super::*;

#[test]
fn config_default_enables_custom_sounds() {
    let cfg = Config::default();
    assert!(
        cfg.custom_sounds_enabled,
        "custom_sounds_enabled should default to true so the bundled folders are active out of the box"
    );
}

#[test]
fn config_default_disables_machine_nice_sound() {
    assert!(!Config::default().machine_nice_sound);
}

#[test]
fn config_default_hides_mouse_cursor() {
    assert!(Config::default().hide_mouse_cursor);
}
