use deadsync_song_lua::{
    SongLuaOverlayState, SongLuaOverlayStateDelta, apply_overlay_delta, overlay_state_lerp,
};

#[test]
fn compiled_depth_testing_and_writing_are_independent() {
    use deadsync_song_lua::{SongLuaCompileContext, SongLuaNoteskinResolver, compile_song_lua_with_default_host};
    let directory = std::env::temp_dir().join(format!("deadsync-depth-settings-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let entry = directory.join("default.lua");
    std::fs::write(&entry, r#"
return Def.ActorFrame{
    OnCommand=function(self) self:ztest(1) end,
    Def.Quad{Name='Sky'},
    Def.Quad{Name='Mask', OnCommand=function(self) self:zwrite(1) end},
    Def.Quad{Name='WriteOnly', OnCommand=function(self) self:zwrite(1):ztest(0) end},
    Def.Quad{Name='Fail', OnCommand=function(self) self:ztestmode('WriteOnFail') end},
    Def.ActorFrame{
        OnCommand=function(self) self:zbuffer(1) end,
        Def.Quad{Name='OverrideWrite', OnCommand=function(self) self:zwrite(0) end},
        Def.Quad{Name='OverrideTest', OnCommand=function(self) self:ztest(0) end},
        Def.Quad{Name='OverrideFail', OnCommand=function(self) self:ztestmode('WriteOnFail'):ztest(1) end},
    },
}
"#).unwrap();
    let mut context = SongLuaCompileContext::new(&directory, "Independent Depth");
    context.music_length_seconds = 4.0;
    let compiled = compile_song_lua_with_default_host::<(), (), _>(
        &entry, &context, SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(std::sync::Arc::from([])), |_| None, |_, _| None,
    ).unwrap();
    for (name, expected) in [
        ("Sky", (true, false, false)),
        ("Mask", (true, true, false)),
        ("WriteOnly", (false, true, false)),
        ("Fail", (true, false, true)),
        ("OverrideWrite", (true, false, false)),
        ("OverrideTest", (false, true, false)),
        ("OverrideFail", (true, true, false)),
    ] {
        let state = compiled.overlays.iter().find(|actor| actor.name.as_deref() == Some(name))
            .unwrap().initial_state;
        assert_eq!((state.depth_test, state.depth_write, state.depth_test_on_fail), expected, "{name}");
    }
}

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
        depth_write: Some(true),
        depth_test_on_fail: Some(true),
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
    assert!(!middle.depth_write);
    assert!(!middle.depth_test_on_fail);

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
    assert!(completed.depth_write);
    assert!(completed.depth_test_on_fail);
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
