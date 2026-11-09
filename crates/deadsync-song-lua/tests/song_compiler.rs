use deadsync_song_lua::{
    self as song, CompiledSongLua, SongLuaCompileContext, SongLuaOverlayActor, SongLuaOverlayKind,
};
use std::{path::Path, sync::Arc};
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Output = CompiledSongLua<SongLuaOverlayActor<Kind>>;
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}

fn compare(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Vec<Output> {
    compare_with(sources, configure, |output, index| {
        compare_output(output, index)
    })
}

fn compare_with(
    sources: &[&str],
    configure: impl FnOnce(&mut SongLuaCompileContext),
    check: impl Fn(&mut Output, usize),
) -> Vec<Output> {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let mut paths = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let path = dir.path().join(format!("layer{i}.lua"));
        std::fs::write(&path, source).unwrap();
        paths.push(path);
    }
    image::RgbaImage::new(2, 2)
        .save(dir.path().join("art.png"))
        .unwrap();
    std::fs::write(dir.path().join("sound.ogg"), []).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "complete compiler");
    context.players[1].enabled = false;
    context.music_length_seconds = 0.2;
    context.compile_clock_inputs = (0..=12)
        .map(|i| {
            [
                i as f64 / 60.,
                i as f64 / 60.,
                if i == 0 { 0. } else { f64::from(1.0_f32 / 60.) },
            ]
        })
        .collect();
    configure(&mut context);
    let paths = paths.iter().map(|p| p.as_path()).collect::<Vec<_>>();

    let rust = song::compile_prepared_song_lua_layers(
        &paths,
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    );
    let mut b = rust.unwrap();
    reference_test::assert_debug("song/tests/rust_song_compiler.rs::layer_count", &b.len());
    for (index, output) in b.iter_mut().enumerate() {
        check(output, index);
    }
    b
}

fn compare_output(b: &Output, index: usize) {
    assert!(b.live_scene.is_none());
    macro_rules! same {($($field:ident),+)=>{$(assert_field(&b.$field,index,stringify!($field));)+}}
    same!(
        live_indices,
        loaded_steps,
        loaded_style,
        startup_reloads,
        startup,
        entry_path,
        screen_width,
        screen_height,
        beat_mods,
        time_mods,
        eases,
        messages,
        sound_paths,
        overlays,
        draw_frames,
        screen_overlay_index,
        overlay_eases,
        overlay_update_unit,
        overlay_updates,
        stateful_message_captures,
        player_actors,
        song_foreground,
        screen_layers,
        hidden_players,
        note_hides,
        column_offsets,
        column_splines,
        column_spline_origin,
        info
    );
    // Every originally compared output field is checked against fixed reference data.
}

fn assert_field(actual: &impl std::fmt::Debug, index: usize, field: &str) {
    reference_test::assert_debug(
        "song/tests/rust_song_compiler.rs::fields",
        &(index, field, actual),
    );
}

#[test]
fn static_commands_startup_tweens_and_messages_match_complete_compiler() {
    let out = compare(
        &[r#"
        mod_actions={{0,'Ping'}}
        return Def.ActorFrame{Name='root',
            InitCommand=function(self)self:x(3)end,
            OnCommand=function(self)self:linear(.1):x(15):queuecommand('Later')end,
            LaterCommand=function(self)self:y(7)end,
            PingMessageCommand=function(self)self:z(9)end,
            Def.Quad{Name='child',InitCommand=function(self)self:zoom(.5):diffusealpha(.8)end},
        }
    "#],
        |_| {},
    );
    assert!(
        out[0]
            .overlays
            .iter()
            .any(|r| r.name.as_deref() == Some("root"))
    );
    assert!(!out[0].messages.is_empty());
}

#[test]
fn chronological_background_and_screen_output_match_complete_compiler() {
    let out = compare(
        &[r#"
        return Def.ActorFrame{Name='moving',OnCommand=function(self)
            self:SetUpdateFunction(function(self)
                local beat=GAMESTATE:GetSongBeat();self:x(beat*20)
                SCREENMAN:GetTopScreen():x(beat*3);collectgarbage()
            end)
        end}
    "#],
        |context| context.background_layer_count = 1,
    );
    assert!(!out[0].overlay_updates.is_empty());
    assert!(out[0].screen_overlay_index.is_some());
}

#[test]
fn separate_layers_own_messages_wrappers_and_parent_indices() {
    let out = compare(
        &[
            "return Def.ActorFrame{Name='background',OnCommand=function(self)self:x(11);self:AddWrapperState():y(7)end,Def.Quad{Name='back'}}",
            "return Def.ActorFrame{Name='foreground',OnCommand=function(self)self:y(13):linear(.1):y(25)end,Def.Quad{Name='front'}}",
        ],
        |context| {
            context.background_layer_count = 1;
            context.foreground_names = vec!["layer1.lua".into()];
            context.layer_start_beats = vec![0., 0.05];
        },
    );
    assert_eq!(out.len(), 2);
    assert!(
        out[0]
            .overlays
            .iter()
            .any(|r| r.name.as_deref() == Some("background"))
    );
    assert!(
        out[1]
            .overlays
            .iter()
            .any(|r| r.name.as_deref() == Some("foreground"))
    );
}

#[test]
fn startup_reload_retains_globals_rng_and_outgoing_end() {
    let out = compare(
        &[r#"
        n=(n or 0)+1
        return Def.ActorFrame{Name=tostring(n)..':'..tostring(ended)..':'..tostring(math.random(1,1000)),
            OnCommand=function(self)if n==1 then SCREENMAN:SetNewScreen('ScreenGameplay')end end,
            EndCommand=function(self)ended=(ended or 0)+1;math.random()end,
            Def.Quad{Name='leaf'},
        }
    "#],
        |_| {},
    );
    assert_eq!(out[0].startup_reloads, 1);
}

#[test]
fn resources_proxy_targets_custom_draw_and_sound_output_match() {
    let out = compare(
        &[r#"
        local art=Def.Sprite{Name='art',Texture='art.png'}
        local quad=Def.Quad{Name='quad'}
        return Def.ActorFrame{Name='root',
            InitCommand=function(self)self:SetDrawFunction(function(self)quad:Draw();art:Draw()end)end,
            OnCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
            quad,art,
            Def.ActorProxy{Name='proxy',InitCommand=function(self)self:SetTarget(quad)end},
            Def.BitmapText{Name='text',Font='Common Normal',Text='hello'},
        }
    "#],
        |_| {},
    );
    assert!(!out[0].draw_frames.is_empty());
    assert!(!out[0].sound_paths.is_empty());
}

#[test]
fn scalar_entries_preserve_global_schedule_without_actor_output() {
    compare(&["mod_actions={{.05,'OnlyMessage'}};return 12"], |_| {});
}

#[test]
fn empty_preparation_returns_before_entry_validation() {
    let mut context = SongLuaCompileContext::new(".", "empty");
    context.background_layer_count = 3;
    assert!(
        song::compile_prepared_song_lua_layers::<(), ()>(
            &[],
            99,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn unsupported_startup_screen_request_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("default.lua");
    std::fs::write(&path,"return Def.ActorFrame{OnCommand=function(self)SCREENMAN:SetNewScreen('ScreenSelectMusic')end}").unwrap();
    let context = SongLuaCompileContext::new(dir.path(), "unsupported startup");

    let b = song::compile_prepared_song_lua_layers::<(), ()>(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap_err();
    reference_test::assert_debug("song/tests/rust_song_compiler.rs::startup_error", &b);
}

#[test]
fn startup_capture_excludes_reload_construction_init_and_end_broadcasts() {
    let out = compare(
        &[r#"
        n=(n or 0)+1
        if n>1 then MESSAGEMAN:Broadcast('ReloadConstruction')end
        return Def.ActorFrame{Name='root',
            InitCommand=function(self)MESSAGEMAN:Broadcast('AuthoredInit')end,
            OnCommand=function(self)
                if n==1 then SCREENMAN:SetNewScreen('ScreenGameplay')
                else
                    MESSAGEMAN:Broadcast('FinalOn')
                    self:SetUpdateFunction(function(self,dt)
                        if dt==0 then MESSAGEMAN:Broadcast('FinalZero')end
                    end)
                end
            end,
            EndCommand=function(self)MESSAGEMAN:Broadcast('OutgoingEnd')end,
        }
    "#],
        |_| {},
    );
    assert_eq!(
        out[0]
            .messages
            .iter()
            .filter(|m| m.message == "FinalZero")
            .count(),
        1
    );
    assert!(out[0].messages.iter().all(|m| !matches!(
        m.message.as_str(),
        "ReloadConstruction" | "AuthoredInit" | "OutgoingEnd" | "FinalOn"
    )));
}

#[test]
fn queued_sounds_and_message_sounds_use_real_dispatch_frames() {
    let out = compare(
        &[r#"
        return Def.ActorFrame{Name='root',
            OnCommand=function(self)
                SOUND:PlayOnce('sound.ogg');self:sleep(.05):queuecommand('Later')
            end,
            LaterCommand=function(self)MESSAGEMAN:Broadcast('Ping')end,
            PingMessageCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
        }
    "#],
        |_| {},
    );
    assert!(
        out[0]
            .messages
            .iter()
            .any(|m| m.message.starts_with("__songlua_sound_call_") && m.beat > 0.)
    );
}

#[test]
fn reload_media_boundary_preserves_only_final_on_and_zero_update_sounds() {
    let out = compare(
        &[r#"
        n=(n or 0)+1
        SOUND:PlayOnce('sound.ogg')
        return Def.ActorFrame{Name='root',
            InitCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
            OnCommand=function(self)
                if n==1 then SCREENMAN:SetNewScreen('ScreenGameplay')
                else
                    SOUND:PlayOnce('sound.ogg')
                    self:SetUpdateFunction(function(self,dt)
                        if dt==0 then SOUND:PlayOnce('sound.ogg')end
                    end)
                end
            end,
            EndCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
        }
    "#],
        |context| context.initial_lua_gc_stopped = true,
    );
    assert_eq!(out[0].startup_reloads, 1);
    assert_eq!(
        out[0]
            .messages
            .iter()
            .filter(|m| m.message.starts_with("__songlua_sound_call_"))
            .count(),
        2
    );
}

#[test]
fn framebuffer_and_deferred_sprite_kinds_match_complete_compiler() {
    let out = compare(
        &[r#"
        local capture=Def.ActorFrameTexture{Name='capture',
            InitCommand=function(self)self:SetWidth(16):SetHeight(8):SetTextureName('compiled-aft'):EnableAlphaBuffer(true):Create()end,
            Def.Quad{Name='inside'},
        }
        return Def.ActorFrame{capture,
            Def.Sprite{Name='captured',InitCommand=function(self)self:SetTexture(capture:GetTexture())end},
            Def.Sprite{Name='late',OnCommand=function(self)self:Load('art.png')end},
        }
    "#],
        |_| {},
    );
    assert!(
        out[0]
            .overlays
            .iter()
            .any(|r| matches!(r.kind, Kind::ActorFrameTexture { .. }))
    );
    assert!(
        out[0]
            .overlays
            .iter()
            .any(|r| matches!(r.kind, Kind::AftSprite { .. }))
    );
    assert!(
        out[0]
            .overlays
            .iter()
            .any(|r| r.name.as_deref() == Some("late") && matches!(r.kind, Kind::Sprite { .. }))
    );
}

fn compare_existing_chronological_fixture(source: &str) {
    compare_with(
        &[source],
        |context| {
            context.music_length_seconds = 4.;
            context.song_display_bpms = [60.; 2];
            context.song_timing_bpms = vec![(0., 60.)];
            context.compile_clock_inputs.clear();
        },
        |rust, index| {
            // Broadcast allocation order can interleave independent actors'
            // tracks differently. Compare complete tracks per actor, keeping
            // target order, every sample and all other output fields exact.

            rust.overlay_updates.sort_by_key(|t| t.overlay_index);

            for capture in &mut rust.stateful_message_captures {
                capture.writes.sort_by_key(|w| w.overlay_index);
            }
            compare_output(rust, index);
        },
    );
}

#[test]
fn existing_chronological_message_advance_matches_complete_compiler() {
    compare_existing_chronological_fixture(include_str!(
        "../../../tests/fixtures/song-lua/phase-update.lua"
    ));
}

#[test]
fn existing_chronological_tween_retarget_matches_complete_compiler() {
    compare_existing_chronological_fixture(include_str!(
        "../../../tests/fixtures/song-lua/retarget-update.lua"
    ));
}

#[test]
fn existing_chronological_spin_tween_matches_complete_compiler() {
    compare_existing_chronological_fixture(include_str!(
        "../../../tests/fixtures/song-lua/spin-update.lua"
    ));
}

#[test]
fn large_actor_scene_retains_message_functions_and_replay_getters_across_gc() {
    let output = compare(
        &[r#"
        local root = Def.ActorFrame {
            OnCommand=function(self)
                collectgarbage('collect')
                MESSAGEMAN:Broadcast('LargeScene')
            end,
        }
        for i=1,300 do
            root[#root+1] = Def.Quad {
                Name='large-'..i,
                InitCommand=function(self) self:x(i):zoomto(2,2) end,
                OnCommand=function(self) self:sleep(0.05):queuecommand('Later') end,
                LaterCommand=function(self) self:x(i+1) end,
                LargeSceneMessageCommand=function(self) self:aux(i) end,
            }
        end
        return root
        "#],
        |_| {},
    );
    assert_eq!(
        output[0]
            .overlays
            .iter()
            .filter(|a| a.name.as_deref().is_some_and(|n| n.starts_with("large-")))
            .count(),
        300
    );
}

// These are the existing failing application microfixtures. Comparing their
// complete prepared payloads separates adapter differences from composition.
fn compare_existing_microfixture(path: &str, seconds: f32, background: bool) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .canonicalize()
        .unwrap();
    reference_test::register_path(path.parent().unwrap());
    reference_test::register_path(Path::new(&song::file_path_string(path.parent().unwrap())));
    let mut context = SongLuaCompileContext::new(path.parent().unwrap(), "existing microfixture");
    context.music_length_seconds = seconds;
    context.song_display_bpms = [60.; 2];
    context.song_timing_bpms = vec![(0., 60.)];
    context.background_layer_count = usize::from(background);
    context.screen_width = 854.;
    context.screen_height = 480.;
    context.display_width = 854.;
    context.display_height = 480.;
    context.display_aspect_ratio = 854. / 480.;
    context.video_renderers = "opengl,software".into();
    for (i, player) in context.players.iter_mut().enumerate() {
        player.enabled = true;
        player.screen_x = ((if i == 0 { 0.85 } else { 2.15 }) / 3. * 854_f32).floor();
        player.screen_y = 240.;
        player.speedmod = song::SongLuaSpeedMod::X(1.);
    }

    let mut rust = song::compile_prepared_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap();
    reference_test::assert_debug("song/tests/rust_song_compiler.rs::layer_count", &rust.len());
    for (i, rust) in rust.iter_mut().enumerate() {
        // Independent actors' records can interleave by allocation identity.
        // Keep each actor's target order, values and every sample unchanged.

        rust.overlay_updates
            .sort_by_key(|track| track.overlay_index);

        for capture in &mut rust.stateful_message_captures {
            capture.writes.sort_by_key(|write| write.overlay_index);
        }
        compare_output(rust, i);
    }
}

macro_rules! existing_microfixture {
    ($name:ident, $path:literal, $seconds:literal, $background:literal) => {
        #[test]
        fn $name() {
            compare_existing_microfixture($path, $seconds, $background);
        }
    };
}
existing_microfixture!(
    existing_callback_wag_prepared_payload,
    "tests/fixtures/song-lua/callback-wag/message-wag-repeat.lua",
    6.,
    true
);
existing_microfixture!(
    existing_deferred_effects_prepared_payload,
    "tests/fixtures/song-lua/deferred-effects.lua",
    7.,
    false
);
existing_microfixture!(
    existing_late_colors_prepared_payload,
    "tests/fixtures/song-lua/late-colors.lua",
    4.,
    false
);
existing_microfixture!(
    existing_late_pulse_prepared_payload,
    "tests/fixtures/song-lua/late-pulse.lua",
    7.,
    false
);
existing_microfixture!(
    existing_motion_body_prepared_payload,
    "tests/fixtures/song-lua/motion-body.lua",
    2.,
    true
);
existing_microfixture!(
    existing_message_backlog_prepared_payload,
    "tests/fixtures/song-lua/message-wag-stop.lua",
    4.,
    true
);
existing_microfixture!(
    existing_nested_start_prepared_payload,
    "crates/deadsync-song-lua/tests/fixtures/nested-start.lua",
    4.,
    true
);
existing_microfixture!(
    existing_queued_state_prepared_payload,
    "crates/deadsync-song-lua/tests/fixtures/queued-state.lua",
    4.,
    true
);
existing_microfixture!(
    existing_queued_blink_prepared_payload,
    "tests/fixtures/song-lua/queued-blink.lua",
    4.,
    false
);

// Keep the original application setter-history assertion enabled. This
// separate control checks every production payload field after the fixture's
// final explicit XMod(4), independent of intermediate conflicting pairs writes.
existing_microfixture!(
    existing_conflicting_speed_setters_final_payload,
    "tests/fixtures/song-lua/native-table-order.lua",
    2.,
    false
);

#[test]
fn existing_background_fit_helpers_match_complete_compiler() {
    for width in [640., 854.] {
        compare(
            &[r#"
            local root=Def.ActorFrame{}
            for _,mode in ipairs{'CoverDistort','CoverPreserve','FitInside','FitInsideAvoidLetter','FitInsideAvoidPillar'} do
                for _,center in ipairs{false,true} do
                    root[#root+1]=Def.Sprite{Texture='art.png',OnCommand=function(self)
                        PREFSMAN:SetPreference('BackgroundFitMode','BackgroundFitMode_'..mode)
                        self:xy(17,29):halign(0):valign(1)
                        if center then assert(self:scale_or_crop_background()==self)
                        else assert(self:scale_or_crop_background_no_move()==self) end
                    end}
                end
            end
            return root
        "#],
            |context| context.screen_width = width,
        );
    }
}

#[test]
fn copied_theme_texture_keeps_metadata_after_reading_default_textures() {
    compare(
        &[r#"
        local quad,sprite,graph,target,loaded
        return Def.ActorFrame{
            OnCommand=function()
                local white=assert(quad:GetTexture())
                assert(sprite:GetTexture()==nil and target:GetTexture()==nil)
                assert(graph:GetTexture():GetPath()=='')
                local image=assert(loaded:GetTexture())
                graph:SetTexture(image)
                collectgarbage('collect')
                local copied=assert(graph:GetTexture())
                assert(copied:GetPath()==image:GetPath())
                assert(copied:GetTextureWidth()==image:GetTextureWidth())
                assert(copied:GetTextureHeight()==image:GetTextureHeight())
                graph:SetTexture(white)
                assert(graph:GetTexture():GetPath()=='' and graph:GetTexture():GetTextureWidth()==1)
            end,
            Def.Quad{InitCommand=function(self)quad=self end},
            Def.Sprite{InitCommand=function(self)sprite=self end},
            Def.ActorMultiVertex{InitCommand=function(self)graph=self end},
            Def.ActorFrameTexture{InitCommand=function(self)target=self end},
            Def.Sprite{Texture='__songlua_theme_path/G/_Judgments/Love 2x6.png',
                InitCommand=function(self)loaded=self end},
        }
    "#],
        |_| {},
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
