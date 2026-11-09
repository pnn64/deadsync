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
    let dir = tempfile::tempdir().unwrap();
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
    let native = song::compile_song_lua_layers_with_default_host(
        &paths,
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    );
    let rust = song::rust_engine::compile_prepared_song_lua_layers(
        &paths,
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    );
    assert_eq!(
        native.is_err(),
        rust.is_err(),
        "native: {:?}; Rust: {:?}",
        native.as_ref().err(),
        rust.as_ref().err()
    );
    let (a, b) = (native.unwrap(), rust.unwrap());
    assert_eq!(a.len(), b.len());
    for (i, (a, b)) in a.iter().zip(&b).enumerate() {
        compare_output(a, b, i);
    }
    b
}
fn compare_output(a: &Output, b: &Output, index: usize) {
    assert!(a.live_scene.is_none() && b.live_scene.is_none());
    macro_rules! same {($($field:ident),+)=>{$(assert_field(&format!("{:?}",a.$field),&format!("{:?}",b.$field),index,stringify!($field));)+}}
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

fn assert_field(a: &str, b: &str, index: usize, field: &str) {
    if a == b {
        return;
    }
    let a = a.chars().collect::<Vec<_>>();
    let b = b.chars().collect::<Vec<_>>();
    let offset = a
        .iter()
        .zip(&b)
        .position(|(a, b)| a != b)
        .unwrap_or(a.len().min(b.len()));
    let snippet = |s: &[char]| {
        s[offset.saturating_sub(100)..(offset + 200).min(s.len())]
            .iter()
            .collect::<String>()
    };
    panic!(
        "layer {index}: {field}, first difference at {offset}\nnative: {}\nRust: {}",
        snippet(&a),
        snippet(&b)
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
            TouchMessageCommand=function(self,p)self:z(p.value):basezoomx(.5)end,
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
        return Def.Actor{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
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
                local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
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
        ],
        |c| c.background_layer_count = 1,
    );
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|o| !o.overlay_writes.is_empty()));
}
