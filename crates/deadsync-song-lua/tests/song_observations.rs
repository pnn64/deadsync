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

fn compile_pair(
    sources: &[&str],
    configure: impl FnOnce(&mut SongLuaCompileContext),
) -> Result<Vec<Output>, String> {
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
    rust
}
fn compare(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Vec<Output> {
    let output = compile_pair(sources, configure).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_observations.rs::layer_count",
        &output.len(),
    );
    for (index, output) in output.iter().enumerate() {
        compare_output(output, index);
    }
    output
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
        info,
        boolean_writes,
        speed_writes,
        noteskin_writes,
        state_calls,
        overlay_writes
    );
}

fn assert_field(actual: &impl std::fmt::Debug, index: usize, field: &str) {
    reference_test::assert_debug(
        "song/tests/rust_song_observations.rs::fields",
        &(index, field, actual),
    );
}

#[test]
fn startup_option_setters_and_partial_speed_failures_match() {
    let out = compare(
        &[r#"
        return Def.Actor{InitCommand=function(self)
            local ps=GAMESTATE:GetPlayerState(PLAYER_1)
            for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
                local po=ps:GetPlayerOptions(level)
                po:Mirror(true,false);po:Mirror(false,true)
                po:XMod(2,.4,true);po:TimeSpacing(0,.2)
                pcall(function()po:CMod(-1)end)
                pcall(function()po:MMod(333,-1)end)
                pcall(function()po:ScrollSpeed(3,-1)end)
                po:ScrollBPM(444,.3);po:MaxScrollBPM(555,.6)
            end
        end}
    "#],
        |_| {},
    );
    assert!(!out[0].boolean_writes.is_empty());
    assert!(out[0].speed_writes.iter().any(|w| w.failed));
}

#[test]
fn accepted_rejected_and_parsed_noteskin_writes_match() {
    let out = compare(
        &[r#"
        NOTESKIN.DoesNoteSkinExist=function(self,name)return name=='accepted'end
        return Def.Actor{InitCommand=function(self)
            local ps=GAMESTATE:GetPlayerState(PLAYER_1)
            for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
                local po=ps:GetPlayerOptions(level)
                po:NoteSkin('accepted');po:NoteSkin('rejected',true)
                po:FromString('50% Mini, accepted')
                ps:SetPlayerOptions(level,'25% Mini, accepted')
            end
        end}
    "#],
        |_| {},
    );
    assert!(!out[0].noteskin_writes.is_empty());
}

#[test]
fn nested_manager_calls_and_reload_history_match() {
    let out = compare(
        &[r#"
        visits=(visits or 0)+1
        return Def.ActorFrame{
            OnCommand=function(self)
                MESSAGEMAN:Broadcast('Outer')
                if visits==1 then SCREENMAN:SetNewScreen('ScreenGameplay')end
            end,
            OuterMessageCommand=function(self)MESSAGEMAN:Broadcast('Inner')end,
            InnerMessageCommand=function(self)GAMESTATE:SetCurrentSteps(PLAYER_1,nil)end,
        }
    "#],
        |_| {},
    );
    assert_eq!(out[0].startup_reloads, 1);
    assert!(out[0].state_calls.len() >= 4);
}

#[test]
fn chronological_direct_queued_update_and_message_writes_match() {
    let out = compare(
        &[r#"
        local started=false
        return Def.ActorFrame{Name='observed',
            OnCommand=function(self)self:SetUpdateFunction(function(self)
                local beat=GAMESTATE:GetSongBeat()
                self:x(beat*12):x(beat*14):diffusealpha(.8):visible(true)
                if not started then
                    started=true
                    self:linear(.03):y(7):queuecommand('Later')
                    MESSAGEMAN:Broadcast('Touch',{value=8})
                end
                self:playcommand('Update')
            end)end,
            UpdateCommand=function(self)self:z(9):draworder(2)end,
            LaterCommand=function(self)self:z(11):playcommand('Update')end,
            TouchMessageCommand=function(self,p)if p then self:z(p.value):basezoomx(.5)end end,
        }
    "#],
        |_| {},
    );
    assert!(!out[0].overlay_writes.is_empty());
    assert!(!out[0].state_calls.is_empty());
}

#[test]
fn chronological_option_setters_and_gc_match() {
    let out = compare(
        &[r#"
        return Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
            po:Mirror(true,true);po:XMod(2,.7);po:FromString('25% Mini')
            collectgarbage()
        end)end}
    "#],
        |_| {},
    );
    assert!(out[0].boolean_writes.len() > 1);
    assert!(out[0].noteskin_writes.len() > 1);
}

#[test]
fn startup_queue_capture_excludes_noteskin_discovery_writes() {
    let out = compare(
        &[r#"
        NOTESKIN.DoesNoteSkinExist=function()return true end
        return Def.Actor{OnCommand=function(self)self:sleep(.03):queuecommand('Later')end,
            LaterCommand=function(self)
                local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
                po:Mirror(true);po:XMod(3);po:NoteSkin('queued')
                MESSAGEMAN:Broadcast('Queued')
            end}
    "#],
        |_| {},
    );
    assert!(!out[0].boolean_writes.is_empty());
}

#[test]
fn multilayer_screen_and_player_write_indices_match() {
    let out = compare(
        &[
            r#"
        return Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local s=SCREENMAN:GetTopScreen(); s:x(3)
            s:GetChild('PlayerP1'):y(4)
            self:rotationz(5)
        end)end}
    "#,
            r#"
        return Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function(self)
            self:zoom(.7):basealpha(.4)
        end)end}
    "#,
        ],
        |c| c.background_layer_count = 1,
    );
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|o| !o.overlay_writes.is_empty()));
}

#[test]
fn option_observation_follows_republished_global_owner_identity() {
    let out = compare(
        &[r#"
        NOTESKIN.DoesNoteSkinExist=function()return true end
        return Def.Actor{InitCommand=function(self)
            local ps=GAMESTATE:GetPlayerState(PLAYER_1)
            local po=ps:GetPlayerOptions('ModsLevel_Preferred')
            local previous=__songlua_player_options_1
            __songlua_player_options_1=po
            po:Mirror(true);po:XMod(2);po:NoteSkin('preferred')
            previous:Mirror(true);previous:XMod(3);previous:NoteSkin('stage')
            __songlua_player_options_1=previous
        end}
    "#],
        |_| {},
    );
    assert_eq!(out[0].boolean_writes.len(), 1);
    assert_eq!(out[0].speed_writes.len(), 1);
    assert_eq!(out[0].noteskin_writes.len(), 1);
}

#[test]
fn current_steps_observation_uses_authored_metadata_after_notifications() {
    let out = compare(
        &[r#"
        return Def.ActorFrame{InitCommand=function(self)
            local steps=GAMESTATE:GetCurrentSong():GetAllSteps()
            steps[2].GetDifficulty=function()return 'Difficulty_Edit'end
            GAMESTATE:SetCurrentSteps(PLAYER_1,steps[2])
            GAMESTATE:SetCurrentSteps(PLAYER_1,steps[2])
        end,CurrentStepsP1ChangedMessageCommand=function(self)
            MESSAGEMAN:Broadcast('Selected')
        end}
    "#],
        |c| {
            c.song_steps = [song::SongLuaDifficulty::Easy, song::SongLuaDifficulty::Hard]
                .into_iter()
                .enumerate()
                .map(|(i, difficulty)| song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty,
                    description: format!("chart-{i}"),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 8,
                    music_file: "same.ogg".into(),
                    possible_grade_points: 0,
                })
                .collect();
            c.player_step_indices = [Some(0), Some(0)];
        },
    );
    assert!(
        out[0]
            .state_calls
            .iter()
            .any(|c| c.operation == "GameState.SetCurrentSteps")
    );
}

#[test]
fn original_non_frame_update_fixtures_remain_rejected() {
    for source in [
        r#"
        return Def.Actor{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
            po:Mirror(true,true);po:XMod(2,.7);po:FromString('25% Mini')
            collectgarbage()
        end)end}
    "#,
        r#"
        return Def.Actor{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local s=SCREENMAN:GetTopScreen(); s:x(3)
            s:GetChild('PlayerP1'):y(4)
            self:rotationz(5)
        end)end}
    "#,
        r#"
        return Def.Quad{OnCommand=function(self)self:SetUpdateFunction(function(self)
            self:zoom(.7):basealpha(.4)
        end)end}
    "#,
    ] {
        let rust = compile_pair(&[source], |_| {});

        assert!(rust.unwrap_err().contains("SetUpdateFunction"));
    }
}

#[test]
fn original_init_screen_request_remains_discarded() {
    let out = compare(
        &[r#"
        visits=(visits or 0)+1
        return Def.ActorFrame{
            InitCommand=function(self)
                MESSAGEMAN:Broadcast('Outer')
                if visits==1 then SCREENMAN:SetNewScreen('ScreenGameplay')end
            end,
            OuterMessageCommand=function(self)MESSAGEMAN:Broadcast('Inner')end,
            InnerMessageCommand=function(self)GAMESTATE:SetCurrentSteps(PLAYER_1,nil)end,
        }
    "#],
        |_| {},
    );
    assert_eq!(out[0].startup_reloads, 0);
}

#[test]
fn original_stage_queue_is_not_the_published_song_option_owner() {
    let out = compare(
        &[r#"
        NOTESKIN.DoesNoteSkinExist=function()return true end
        return Def.Actor{OnCommand=function(self)self:sleep(.03):queuecommand('Later')end,
            LaterCommand=function(self)
                local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
                po:Mirror(true);po:XMod(3);po:NoteSkin('queued')
                MESSAGEMAN:Broadcast('Queued')
            end}
    "#],
        |_| {},
    );
    assert!(out[0].boolean_writes.is_empty());
}

#[test]
fn original_nil_message_probe_preserves_error_cause_and_observations() {
    let rust = compile_pair(
        &[r#"
        local started=false
        return Def.ActorFrame{Name='observed',
            OnCommand=function(self)self:SetUpdateFunction(function(self)
                local beat=GAMESTATE:GetSongBeat()
                self:x(beat*12):x(beat*14):diffusealpha(.8):visible(true)
                if not started then
                    started=true
                    self:linear(.03):y(7):queuecommand('Later')
                    MESSAGEMAN:Broadcast('Touch',{value=8})
                end
                self:playcommand('Update')
            end)end,
            UpdateCommand=function(self)self:z(9):draworder(2)end,
            LaterCommand=function(self)self:z(11):playcommand('Update')end,
            TouchMessageCommand=function(self,p)self:z(p.value):basezoomx(.5)end,
        }
    "#],
        |_| {},
    );
    let mut b = rust.unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_observations.rs::layer_count",
        &b.len(),
    );
    for (index, b) in b.iter_mut().enumerate() {
        for diagnostics in [&mut b.info.skipped_message_command_captures] {
            assert_eq!(diagnostics.len(), 1);
            let cause = "attempt to index local 'p' (a nil value)";
            assert!(
                diagnostics[0].contains("TouchMessageCommand") && diagnostics[0].contains(cause)
            );
            // Only the engine-specific rendering is normalized in this control.
            diagnostics[0] = format!("TouchMessageCommand: {cause}");
        }
        compare_output(b, index);
    }
}

#[test]
fn queued_update_command_keeps_last_same_frame_immediate_setter() {
    let out = compare(
        &[r#"
        local queued=false
        return Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function(self)
            self:visible(false):basezoomx(.2)
            if not queued then queued=true;self:sleep(.02):queuecommand('Later')end
        end)end,LaterCommand=function(self)self:playcommand('Update')end,
        UpdateCommand=function(self)self:visible(false):visible(true):basezoomx(.3):basezoomx(.6)end}
    "#],
        |_| {},
    );
    assert!(!out[0].overlay_writes.is_empty());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
