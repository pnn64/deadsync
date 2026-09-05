use deadsync_song_lua::{
    SongLuaOverlayState, SongLuaOverlayStateDelta, apply_overlay_delta, overlay_state_lerp,
};

#[test]
fn partial_deltas_preserve_baselines_and_animation_epoch() {
    let initial = SongLuaOverlayState {
        x: 20.0,
        y: 30.0,
        size: Some([40.0, 60.0]),
        sprite_animation_epoch: Some(5.0),
        ..SongLuaOverlayState::default()
    };
    let delta = SongLuaOverlayStateDelta {
        x: Some(100.0),
        sprite_state_index: Some(2),
        texture_filtering: Some(false),
        depth_test: Some(true),
        ..SongLuaOverlayStateDelta::default()
    };
    let mut middle = initial;
    overlay_state_lerp(&mut middle, &delta, 0.5);
    assert_eq!(middle.x, 60.0);
    assert_eq!(middle.y, 30.0);
    assert_eq!(middle.size, Some([40.0, 60.0]));
    assert_eq!(middle.sprite_animation_epoch, Some(5.0));
    assert_eq!(middle.sprite_state_index, initial.sprite_state_index);
    assert!(middle.texture_filtering);
    assert!(!middle.depth_test);

    // State deltas leave playback events and animation-clock bookkeeping to
    // their callers, including when applying a completed tween.
    let mut completed = middle;
    apply_overlay_delta(&mut completed, &delta);
    assert_eq!(completed.x, 100.0);
    assert_eq!(completed.y, 30.0);
    assert_eq!(completed.sprite_state_index, Some(2));
    assert_eq!(completed.sprite_animation_epoch, Some(5.0));
    assert!(!completed.texture_filtering);
    assert!(completed.depth_test);
}

#[test]
fn interpolation_keeps_overshoot_and_switches_terminal_flags() {
    let from = SongLuaOverlayState {
        x: 20.0,
        y: 30.0,
        size: Some([40.0, 60.0]),
        ..SongLuaOverlayState::default()
    };
    let delta = SongLuaOverlayStateDelta {
        x: Some(100.0),
        size: Some([80.0, 100.0]),
        fov: Some(60.0),
        texture_filtering: Some(false),
        depth_test: Some(true),
        ..SongLuaOverlayStateDelta::default()
    };
    let mut middle = from;
    overlay_state_lerp(&mut middle, &delta, 0.5);
    assert_eq!(middle.size, Some([60.0, 80.0]));
    assert_eq!(middle.fov, None);
    assert_eq!(middle.y, 30.0);
    assert!(middle.texture_filtering);
    assert!(!middle.depth_test);
    for t in [1.0 - f32::EPSILON, 1.0, 1.25] {
        let mut state = from;
        overlay_state_lerp(&mut state, &delta, t);
        assert!(!state.texture_filtering, "t={t}");
        assert!(state.depth_test, "t={t}");
        assert_eq!(state.y, 30.0);
    }
    let mut overshoot = from;
    overlay_state_lerp(&mut overshoot, &delta, 1.25);
    assert_eq!(overshoot.x, 120.0);
    assert_eq!(overshoot.size, Some([90.0, 110.0]));
}
