fn compile_sampling_fixture() -> TestCompiledSongLua {
    let (root, entry) = song_lua_fixture("sampling_buffers.lua");
    let mut context = SongLuaCompileContext::new(root, "Sampling buffers");
    context.music_length_seconds = 2.0;
    test_compile_song_lua(&entry, &context).unwrap()
}

#[test]
fn sampled_states_preserve_static_actors_tween_endpoints_and_stopped_updates() {
    let compiled = compile_sampling_fixture();
    for i in 1..=128 {
        let name = format!("Static{i}");
        let (index, actor) = compiled.overlays.iter().enumerate()
            .find(|(_, actor)| actor.name.as_deref() == Some(&name)).unwrap();
        assert_eq!(actor.initial_state.x, i as f32);
        assert_eq!(actor.initial_state.y, -(i as f32));
        assert!(!compiled.overlay_updates.iter().any(|track| track.overlay_index == index));
    }
    for (name, target, expected, end) in [
        ("Tween", SongLuaOverlayUpdateTarget::X, 120.0, 1.75),
        ("Counter", SongLuaOverlayUpdateTarget::Y, 31.0, 0.5),
    ] {
        let index = compiled.overlays.iter().position(|actor| actor.name.as_deref() == Some(name)).unwrap();
        let track = compiled.overlay_updates.iter()
            .find(|track| track.overlay_index == index && track.target == target).unwrap();
        let last = track.samples.last().unwrap();
        assert_eq!(last.value, SongLuaOverlayUpdateValue::F32(expected));
        assert!((last.beat - end).abs() < 0.02, "{name}: {}", last.beat);
    }
    assert!(compiled.messages.iter().any(|event| event.message == "Pulse" && event.beat == 0.75));
    assert!(compiled.messages.iter().any(|event| event.message == "Pulse" && event.beat == 1.25));
}

#[test]
#[ignore = "manual release benchmark; includes complete Lua VM/host setup and teardown"]
fn sampling_compile_bench() {
    crate::perf::measure_sampled("compile_128_static_3_dynamic", 3, 1, compile_sampling_fixture);
}

#[test]
#[ignore = "capture complete before/after compiler output using DEADSYNC_PERF_SNAPSHOT"]
fn sampling_compile_snapshot() {
    let mut compiled = compile_sampling_fixture();
    // HashMap iteration can assign tracks in a different order across processes.
    compiled.overlay_updates.sort_by_key(|track| (track.overlay_index, track.target));
    compiled.entry_path = PathBuf::from("sampling_buffers.lua");
    fs::write(std::env::var_os("DEADSYNC_PERF_SNAPSHOT").expect("snapshot path"),
        format!("{compiled:#?}")).unwrap();
}
