use deadsync_song_lua::*;

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile(path: &std::path::Path, context: &SongLuaCompileContext) -> Compiled {
    compile_song_lua::<(), (), _>(
        path,
        context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap()
}

#[test]
fn baked_child_queues_match_live_after_helper_gaps_and_sparse_declarations() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/song-lua");
    for layout in ["dense", "helper", "sparse", "children"] {
        let dir = tempfile::tempdir().unwrap();
        for name in ["baked-child-declarations.lua", "baked-child-helper.lua"] {
            std::fs::copy(fixtures.join(name), dir.path().join(name)).unwrap();
        }
        let path = dir.path().join("default.lua");
        std::fs::write(
            &path,
            format!("return LoadActor('baked-child-declarations.lua', '{layout}')"),
        )
        .unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "Child queues");
        context.music_length_seconds = 2.;
        context.song_display_bpms = [60.; 2];
        let baked = compile(&path, &context);
        assert_eq!(baked.info, SongLuaCompileInfo::default(), "{layout}");
        assert!(baked.live_scene.is_none(), "must test actual baked output");
        let target = baked
            .overlays
            .iter()
            .position(|a| a.name.as_deref() == Some("Target"))
            .unwrap();
        let track = baked
            .overlay_updates
            .iter()
            .find(|track| {
                track.overlay_index == target && track.target == SongLuaOverlayUpdateTarget::X
            })
            .expect("queued Init command must keep driving effects through the song");
        assert!(
            track
                .samples
                .iter()
                .any(|s| matches!(s.value, SongLuaOverlayUpdateValue::F32(x) if x > 150.)),
            "{layout}: effects must reach the end of the song"
        );

        context.live = true;
        let mut live = compile(&path, &context);
        let local = live
            .overlays
            .iter()
            .position(|a| a.name.as_deref() == Some("Target"))
            .unwrap();
        let index = live.live_actor_indices()[local];
        let mut scene = live.live_scene.take().unwrap();
        for frame in 0_usize..=120 {
            let seconds = frame as f64 / 60.;
            let prior = frame.saturating_sub(1) as f64 / 60.;
            let output = scene
                .update(SongLuaLiveFrame {
                    beat: seconds,
                    music_seconds: seconds,
                    screen_seconds: seconds,
                    delta_seconds: f64::from(seconds as f32 - prior as f32),
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{layout}: {:?}", output.errors);
            let baked_x = track
                .samples
                .iter()
                .rev()
                .find(|s| s.time <= seconds as f32)
                .map_or(baked.overlays[target].initial_state.x, |s| match s.value {
                    SongLuaOverlayUpdateValue::F32(x) => x,
                    _ => panic!("x sample"),
                });
            assert!(
                (baked_x - scene.states()[index].x).abs() < 0.001,
                "{layout} at {seconds}: baked={baked_x}, live={}",
                scene.states()[index].x
            );
        }
        scene.end().unwrap();
    }
}
