use super::*;
use deadsync_config::defaults::{
    DEFAULT_CUSTOM_SOUNDS_ENABLED, DEFAULT_HIDE_MOUSE_CURSOR, DEFAULT_MACHINE_NICE_SOUND,
};

#[test]
fn config_default_enables_custom_sounds() {
    assert!(
        Config::default().custom_sounds_enabled == DEFAULT_CUSTOM_SOUNDS_ENABLED,
        "custom_sounds_enabled should default to true so the bundled folders are active out of the box"
    );
}

#[test]
fn config_default_disables_machine_nice_sound() {
    assert_eq!(
        Config::default().machine_nice_sound,
        DEFAULT_MACHINE_NICE_SOUND
    );
}

#[test]
fn config_default_hides_mouse_cursor() {
    assert_eq!(
        Config::default().hide_mouse_cursor,
        DEFAULT_HIDE_MOUSE_CURSOR
    );
}
