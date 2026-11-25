use deadsync_assets::song_lua::{compile_song_lua, compile_song_lua_layers};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaEaseTarget, SongLuaLiveFrame, SongLuaOverlayKind,
};
use std::{fs, path::Path};

#[test]
fn scheduled_xml_runs_inline_lua_and_legacy_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.xml");
    fs::write(
        &path,
        r##"
<ActorFrame><children>
  <Layer Type="Quad" Name="legacy" OnCommand="x,+13;diffuse,#804020;zoom,1.5;sleep,3" />
  <Layer Type="Quad" Name="inline" OnCommand="%function(self)
    if 1 < 2 and 2 &lt; 3 then self:x(29):sleep(3) end
  end" />
</children></ActorFrame>
"##,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "XML actors");
    context.music_length_seconds = 4.0;
    let compiled = compile_song_lua(&path, &context).unwrap();
    for (name, x) in [("legacy", 13.0), ("inline", 29.0)] {
        let actor = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some(name))
            .unwrap();
        assert!(matches!(actor.kind, SongLuaOverlayKind::Quad));
        assert_eq!(actor.initial_state.x, x);
    }
}

#[test]
fn baked_game_commands_apply_current_modifiers_on_the_command_frame() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    fs::write(
        &path,
        r#"
return Def.ActorFrame {
  OnCommand=function(self) self:sleep(0.25):queuecommand('SetMods') end,
  SetModsCommand=function()
    GAMESTATE:ApplyGameCommand('mod,*0.1 stealth', PLAYER_1)
  end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "Immediate modifiers");
    context.music_length_seconds = 1.0;
    let baked = compile_song_lua(&path, &context).unwrap();
    let stealth: Vec<_> = baked
        .eases
        .iter()
        .filter(|window| matches!(&window.target, SongLuaEaseTarget::Mod(key) if key == "stealth"))
        .collect();
    assert_eq!(stealth.len(), 1);
    let window = stealth[0];
    assert_eq!(window.player, Some(1));
    assert_eq!([window.from, window.to], [100.0, 100.0]);
    assert_eq!(window.approach_speed, None);
    assert!((0.25..0.29).contains(&window.start), "{window:?}");
}

#[test]
#[ignore = "requires the retained Eros and Apollo song assets and full playback"]
fn eros_and_apollo_runs_baked_and_runtime() {
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = tempfile::tempdir().unwrap();
    deadsync_assets::init_paths(deadsync_config::dirs::AssetPaths {
        search_roots: vec![data.path().to_path_buf(), bundle.clone()],
        graphic_roots: vec![bundle.join("assets/graphics")],
        texture_roots: [data.path().join("assets"), bundle.join("assets")],
        noteskin_roots: vec![bundle.join("assets/noteskins")],
        noteskin_pack_roots: vec![bundle.join("assets/noteskins")],
        noteskin_cache: data.path().join("noteskins"),
        banner_cache: data.path().join("banners"),
        cdtitle_cache: data.path().join("cdtitles"),
    })
    .unwrap();
    let directory = std::env::var_os("DEADSYNC_EROS_FIXTURE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/itgmania-song-lua-micro/eros-and-apollo/inputs")
        });
    let duration =
        deadlib_audio_decode::file_length_seconds(&directory.join("eros.ogg"), Default::default())
            .unwrap();
    let song = deadsync_simfile::song::parse_song_meta_file(
        &directory.join("eros.sm"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        -0.008,
        |_| duration,
    )
    .unwrap();
    let native: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.parent().unwrap().join("manifest.json")).unwrap(),
    )
    .unwrap();
    let foreground = native["native_foreground_loads"].as_array().unwrap();
    assert_eq!(song.foreground_lua_changes.len(), foreground.len());
    for (change, expected) in song.foreground_lua_changes.iter().zip(foreground) {
        let path = change.path.strip_prefix(&directory).unwrap();
        assert_eq!(change.name, expected["name"].as_str().unwrap());
        assert_eq!(
            f64::from(change.start_beat),
            expected["start_beat"].as_f64().unwrap()
        );
        assert_eq!(
            path.to_string_lossy().replace('\\', "/"),
            expected["path"].as_str().unwrap()
        );
    }
    assert_eq!(song.background_lua_changes.len(), 1);
    let mut context = SongLuaCompileContext::new(&directory, &song.title);
    context.media_probes = deadsync_assets::song_lua::MEDIA_PROBES;
    context.players[1].enabled = false;
    context.players[0].speedmod = deadsync_song_lua::SongLuaSpeedMod::X(2.0);
    context.players[0].noteskin_name = "cel".into();
    context.screen_width = 854.0;
    context.display_width = 854.0;
    context.song_timing_bpms = vec![(0.0, 121.483)];
    context.song_display_bpms = [121.483; 2];
    context.song_timing = song.song_timing.clone();
    context.global_offset_seconds = -0.008;
    context.music_length_seconds = duration;
    context.video_renderers = "opengl".into();
    context.song_steps = song
        .charts
        .iter()
        .map(deadsync_song_lua::SongLuaStep::from)
        .collect();
    context.player_step_indices[0] = Some(0);
    let mode = std::env::var("DEADSYNC_EROS_MODE").ok();
    assert!(matches!(mode.as_deref(), None | Some("baked" | "runtime")));
    for live in [true, false] {
        if mode
            .as_deref()
            .is_some_and(|mode| mode != if live { "runtime" } else { "baked" })
        {
            continue;
        }
        context.live = live;
        context.defer_initial_live_update = live;
        let compile = |paths: &[&Path], primary, context: &SongLuaCompileContext| {
            let output = compile_song_lua_layers(paths, primary, context);
            match &output {
                Ok(layers) => assert_eq!(layers.len(), foreground.len() + 1),
                Err(error) => eprintln!("Eros and Apollo live={live}: {error}"),
            }
            output
        };
        let mut prepared = deadsync_song_lua::playback::prepare_song_lua(
            &song,
            &context,
            compile_song_lua,
            compile,
        );
        assert!(
            prepared.compile_succeeded(),
            "legacy chart preparation live={live}"
        );
        if live {
            let mut scene = prepared
                .take_live_scene()
                .unwrap()
                .expect("legacy runtime scene");
            let timing = context.song_timing.as_ref().unwrap();
            let mut failed_frames = Vec::new();
            for frame in 0..=(f64::from(duration) * 60.).ceil() as u32 {
                let seconds = f64::from(frame) / 60.;
                let beat = f64::from(timing.get_song_position(seconds as f32).beat);
                let frame = scene
                    .update(SongLuaLiveFrame {
                        beat,
                        music_seconds: seconds,
                        screen_seconds: seconds,
                        delta_seconds: 1. / 60.,
                        ..Default::default()
                    })
                    .unwrap();
                if !frame.errors.is_empty() && failed_frames.len() < 10 {
                    failed_frames.push((beat, frame.errors));
                }
            }
            assert!(failed_frames.is_empty(), "{failed_frames:?}");
            let cursor = scene.runtime_mut().vm_mut().get_global(b"curmod").unwrap();
            let calls = native["native_modifier_calls"].as_f64().unwrap();
            assert_eq!(cursor, deadlib_lua::Value::Number(calls + 1.0));
        }
    }
}
