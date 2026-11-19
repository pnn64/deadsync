use super::*;

#[test]
fn baked_movie_sampler_retains_native_pause_and_resume_clock() {
    crate::tests::init_paths();
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/itgmania-song-lua-micro/sprite-movie-clock-native.json",
    ))
    .unwrap();
    let case = native["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "queued-animation-command")
        .unwrap();
    let updates = case["updates"].as_array().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("movie.mpg");
    let mut bytes = b"YUV4MPEG2 W2 H2 F30:1 Ip A1:1 C420jpeg\nFRAME\n".to_vec();
    bytes.extend([128; 6]);
    std::fs::write(&video, bytes).unwrap();
    let entry = dir.path().join("default.lua");
    let mut controls = String::new();
    for (index, update) in updates.iter().enumerate() {
        if let Some(queued) = update.get("queued_animate") {
            let after = queued["after"].as_f64().unwrap_or(0.0);
            let command = if queued["value"].as_bool().unwrap() {
                "Resume"
            } else {
                "Pause"
            };
            controls.push_str(&format!(
                "if frame=={} then movie:sleep({after}):queuecommand('{command}') end\n",
                index + 1,
            ));
        }
    }
    std::fs::write(
        &entry,
        format!(
            r#"
local movie
local frame=0
return Def.ActorFrame{{
    -- ActorFrame updates children before its own callback. The control sibling
    -- must precede Movie to match the oracle's controls before Sprite::Update.
    Def.ActorFrame{{OnCommand=function(self) self:SetUpdateFunction(function()
        frame=frame+1
        {controls}
    end) end}},
    Def.Sprite{{Name='Movie', Texture='movie.mpg',
        InitCommand=function(self) movie=self end,
        OnCommand=function(self) self:animate(false):visible(false) end,
        ResumeCommand=function(self) self:animate(true) end,
        PauseCommand=function(self) self:animate(false) end,
    }},
}}
"#
        ),
    )
    .unwrap();
    let timing = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..Default::default()
        },
        &[],
    );
    let mut context = crate::SongLuaCompileContext::new(dir.path(), "baked movie clock");
    context.music_length_seconds = 2.0;
    context.players[1].enabled = false;
    context.compile_clock_inputs = updates
        .iter()
        .map(|update| {
            let seconds = update["elapsed"].as_f64().unwrap();
            [seconds, seconds * 2.0, update["delta"].as_f64().unwrap()]
        })
        .collect();
    let compiled = deadsync_assets::song_lua::compile_song_lua(&entry, &context).unwrap();
    assert!(compiled.live_scene.is_none());
    let tracks = crate::gameplay::build_song_lua_overlay_update_tracks(&compiled, &timing, 0.0);
    assert!(
        tracks
            .iter()
            .any(|track| track.target == crate::SongLuaOverlayUpdateTarget::MoviePositionSeconds)
    );
    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        &timing,
        0.0,
    );
    let events = crate::gameplay::build_song_lua_overlay_message_events_with_seconds(
        &compiled,
        &message_seconds,
    );
    let eases = crate::gameplay::build_song_lua_overlay_ease_windows_with_events(
        &compiled, &timing, 0.0, &events,
    );
    let mut overlays = compiled.overlays;
    overlays.push(SongLuaOverlayActor {
        kind: SongLuaOverlayKind::UpdateTracks {
            tracks,
            draw_frames: Arc::from([]),
        },
        name: None,
        parent_index: None,
        initial_state: Default::default(),
        message_commands: Vec::new(),
    });
    let layer = deadsync_gameplay::build_song_lua_visual_layer_runtime(
        0.0,
        compiled.screen_width,
        compiled.screen_height,
        overlays,
        eases,
        events,
        compiled.song_foreground,
        Vec::new(),
    );
    let mut sampler = Layer::new(&layer.overlays);
    let mut expected = 0.0_f32;
    for (index, update) in updates.iter().enumerate() {
        for event in update["events"].as_array().unwrap() {
            assert_eq!(event["method"], "update");
            expected += event["value"].as_f64().unwrap() as f32;
        }
        let now = update["elapsed"].as_f64().unwrap() as f32;
        let mut positions = Vec::new();
        sampler.sample(
            now,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            &mut |path, seconds, looping| positions.push((path.to_owned(), seconds, looping)),
        );
        assert_eq!(positions.len(), 1, "native update {index}");
        assert_eq!(positions[0].0, video);
        assert!(
            (positions[0].1 - expected).abs() < 0.00001,
            "native update {index} at {now}: sampled {} expected {expected}",
            positions[0].1
        );
        assert!(positions[0].2);
    }
}

#[test]
fn baked_movie_advances_without_authored_update_callbacks() {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("movie.mpg");
    let mut bytes = b"YUV4MPEG2 W2 H2 F30:1 Ip A1:1 C420jpeg\nFRAME\n".to_vec();
    bytes.extend([128; 6]);
    std::fs::write(&video, bytes).unwrap();
    let entry = dir.path().join("default.lua");
    std::fs::write(&entry, "return Def.Sprite{Texture='movie.mpg'}").unwrap();
    let mut context = crate::SongLuaCompileContext::new(dir.path(), "movie without callbacks");
    context.music_length_seconds = 1.0;
    context.players[1].enabled = false;
    context.compile_clock_inputs = vec![[0.0, 0.0, 0.0], [0.25, 0.5, 0.25], [0.5, 1.0, 0.25]];
    let compiled = deadsync_assets::song_lua::compile_song_lua(&entry, &context).unwrap();
    let movie = compiled
        .overlay_updates
        .iter()
        .find(|track| track.target == crate::SongLuaOverlayUpdateTarget::MoviePositionSeconds)
        .expect("Sprite::Update must run without a Lua update function");
    let last = movie.samples.last().unwrap();
    assert_eq!(last.value, crate::SongLuaOverlayUpdateValue::F32(0.5));
}
