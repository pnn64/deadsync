use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaMessageEvent,
    {SongLayerPlan, SongRuntime, SongUpdateOutput, test_support},
};
use std::sync::Arc;

fn compare(
    source: &str,
    messages: Vec<SongLuaMessageEvent>,
    expect_error: bool,
) -> Option<SongUpdateOutput> {
    compare_frames(source, messages, expect_error, 12)
}
fn compare_frames(
    source: &str,
    messages: Vec<SongLuaMessageEvent>,
    expect_error: bool,
    last_frame: usize,
) -> Option<SongUpdateOutput> {
    compare_context(source, messages, expect_error, last_frame, |_| {})
}
fn compare_context(
    source: &str,
    messages: Vec<SongLuaMessageEvent>,
    expect_error: bool,
    last_frame: usize,
    configure: impl FnOnce(&mut SongLuaCompileContext),
) -> Option<SongUpdateOutput> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "chronological compiler");
    context.music_length_seconds = 0.2;
    context.compile_clock_inputs = (0..=last_frame)
        .map(|i| {
            [
                i as f64 / 60.,
                i as f64 / 30.,
                if i == 0 {
                    0.
                } else {
                    f64::from(1.0_f32 / 60.0)
                },
            ]
        })
        .collect();
    configure(&mut context);
    let mut rust = SongRuntime::new(&context).unwrap();
    rust.begin_song_compilation().unwrap();
    let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
    rust.load_gameplay_theme(screen, &selected).unwrap();
    let root = rust
        .initialize_song_layers(screen, &SongLayerPlan::new(&[&path], 0, &context).unwrap())
        .unwrap()
        .unwrap();
    let rust_activation = rust
        .activate_song_layers(screen, &context)
        .unwrap()
        .unwrap();
    rust.discover_song_startup_commands(screen, &rust_activation)
        .unwrap();
    let mut overlays = rust
        .read_song_overlay_actors::<(), ()>(
            root,
            &context,
            |_, _, _| Ok(Arc::from([])),
            |_| None,
            |_| {},
        )
        .unwrap();
    let mut tracked = rust.read_song_tracked_actors(screen).unwrap().unwrap();
    let mut rust_messages = messages;
    test_support::prepare_update_startup(
        &mut rust,
        &mut overlays,
        &rust_activation,
        &context,
        &mut rust_messages,
    )
    .unwrap();
    reference_test::assert_source_debug(
        "song/tests/rust_song_updates.rs::messages",
        source,
        &rust_messages,
    );
    let (mut b_sound, mut b_judgment, mut b_sprite) = (Vec::new(), Vec::new(), Vec::new());
    let b = rust.compile_song_update_functions(
        screen,
        root,
        &context,
        &mut overlays,
        &mut tracked,
        &rust_messages,
        &rust_activation,
        &mut b_sound,
        &mut b_judgment,
        &mut b_sprite,
    );
    assert_eq!(
        b.is_err(),
        expect_error,
        "Rust error: {:?}",
        b.as_ref().err()
    );
    if expect_error {
        return None;
    }
    let b = b.unwrap();
    reference_test::assert_source_debug(
        "song/tests/rust_song_updates.rs::output",
        source,
        &(
            &b.eases,
            &b.overlay_eases,
            &b.overlay_tracks,
            &b.column_transforms,
            &b.column_splines,
            &b.stateful_messages,
            &b.broadcasts,
            &b_sound,
            &b_judgment,
            b_sprite.len(),
        ),
    );
    reference_test::assert_source_debug(
        "song/tests/rust_song_updates.rs::overlays",
        source,
        &overlays
            .actors
            .iter()
            .map(|a| {
                (
                    &a.actor.name,
                    a.actor.parent_index,
                    &a.actor.initial_state,
                    &a.actor.message_commands,
                )
            })
            .collect::<Vec<_>>(),
    );
    reference_test::assert_source_debug(
        "song/tests/rust_song_updates.rs::tracked",
        source,
        &tracked.actors.iter().map(|a| &a.actor).collect::<Vec<_>>(),
    );
    let Value::String(id) = rust
        .vm_mut()
        .execute(
            &compile_named(
                b"return table.concat(events or {},'|')",
                b"=chronological events",
            )
            .unwrap(),
        )
        .unwrap()[0]
    else {
        panic!()
    };
    reference_test::assert_source_debug(
        "song/tests/rust_song_updates.rs::events",
        source,
        &rust.vm_mut().string_bytes(id).unwrap(),
    );
    rust.release_song_overlay_actors(&mut overlays).unwrap();
    rust.release_song_tracked_actors(&mut tracked).unwrap();
    rust.release_actor_scene(screen).unwrap();
    rust.vm_mut()
        .execute(&compile_named(b"collectgarbage();collectgarbage()", b"=driver cleanup").unwrap())
        .unwrap();
    drop(rust);
    Some(b)
}

#[test]
fn complete_driver_captures_parent_child_callbacks_and_getter_feedback() {
    let output = compare(
        r#"
        events={};saved={}
        return Def.ActorFrame{
            InitCommand=function(self) self:SetUpdateFunction(function(self,dt)
                events[#events+1]=string.format('p:%.4f',dt)
                saved.first:x(saved.first:GetX()+1)
            end) end,
            Def.Quad{Name='first',InitCommand=function(self) saved.first=self;self:x(2) end},
            Def.ActorFrame{Name='child',InitCommand=function(self)
                self:SetUpdateFunction(function(self,dt) saved.first:y(saved.first:GetX()*2) end)
            end}}
    "#,
        Vec::new(),
        false,
    )
    .unwrap();
    assert!(!output.overlay_tracks.is_empty());
}

#[test]
fn complete_driver_keeps_startup_queues_cancellation_and_recurrence() {
    compare(r#"
        events={};saved={}
        return Def.ActorFrame{
            Def.ActorFrame{Name='first',InitCommand=function(self) saved.first=self end,
                OnCommand=function(self) self:linear(.04):x(20):sleep(.02):queuecommand('Tick') end,
                TickCommand=function(self) events[#events+1]='tick';self:addx(3):sleep(.02):queuecommand('Tick') end},
            Def.ActorFrame{Name='control',InitCommand=function(self)
                self:SetUpdateFunction(function(self,dt)
                    local beat=GAMESTATE:GetSongBeat();if beat>.2 and not self.done then
                        self.done=true;saved.first:stoptweening():x(7)
                    end
                end)
            end}}
    "#,Vec::new(),false).unwrap();
}

#[test]
fn complete_driver_bakes_player_options_and_column_rotation() {
    let output=compare(r#"
        events={}
        return Def.ActorFrame{InitCommand=function(self)
            local p=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            local options=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
            self:SetUpdateFunction(function(self,dt)
                local b=GAMESTATE:GetSongBeat();p:x(b*10):rotationz(b*5):zoomx(1+b)
                options:Drunk(b);options:CMod(100+b*20)
                local handler=p:GetChild('NoteField'):GetColumnActors()[1]:GetRotHandler();handler:SetSplineMode('NoteColumnSplineMode_Offset');local spline=handler:GetSpline();spline:SetSize(1);spline:SetPoint(1,{0,0,b*9})
            end)
        end}
    "#,Vec::new(),false).unwrap();
    assert!(!output.eases.is_empty());
    assert!(!output.column_transforms.is_empty());
}

#[test]
fn complete_driver_appends_runtime_wrappers_and_remaps_their_tracks() {
    let output = compare(
        r#"
        events={};saved={}
        return Def.ActorFrame{Name='wrapped',Def.Quad{},InitCommand=function(self)
            saved.actor=self
            self:SetUpdateFunction(function(self,dt)
                if dt>0 and not self.outer then
                    self.outer=self:AddWrapperState();self.outer:linear(.08):x(12)
                    self.inner=self:AddWrapperState();self.inner:rotationz(5)
                end
            end)
        end}
    "#,
        Vec::new(),
        false,
    )
    .unwrap();
    assert!(!output.overlay_tracks.is_empty());
}

#[test]
fn complete_driver_captures_notefield_wrappers_motion_and_hold_frames() {
    compare(
        r#"
        events={}
        return Def.ActorFrame{InitCommand=function(self)
            local field=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
            self:SetUpdateFunction(function(self,dt)
                if dt>0 and not self.wrapper then
                    self.wrapper=field:AddWrapperState();self.wrapper:linear(.08):y(30)
                    self.wrapper:spin():effectmagnitude(0,0,90):effectclock('beat')
                end
            end)
        end}
    "#,
        Vec::new(),
        false,
    )
    .unwrap();
}

#[test]
fn complete_driver_preserves_message_replay_and_broadcast_provenance() {
    compare(r#"
        events={};saved={}
        return Def.ActorFrame{
            InitCommand=function(self)self:SetUpdateFunction(function(self,dt)
                if dt>0 and not self.sent then self.sent=true;MESSAGEMAN:Broadcast('Pulse') end
            end)end,
            Def.Quad{Name='receiver',InitCommand=function(self)saved.actor=self end,
                PulseMessageCommand=function(self)events[#events+1]='pulse';self:linear(.03):x(9):diffusealpha(.5)end}}
    "#,vec![SongLuaMessageEvent{beat:0.1,message:"Pulse".into(),persists:true}],false).unwrap();
}

#[test]
fn complete_driver_preserves_static_tree_output() {
    compare(
        "events={};return Def.Quad{InitCommand=function(self)self:x(4):zoom(1.5)end}",
        Vec::new(),
        false,
    )
    .unwrap();
}

#[test]
fn complete_driver_reports_frame_metadata_errors() {
    compare(
        r#"
        events={}
        return Def.ActorFrame{Def.Quad{},InitCommand=function(self)broken=self;self:SetUpdateFunction(function(self,dt)
            if dt>0 then self.__songlua_player_index={};self:diffusealpha(.3) end
        end)end}
    "#,
        Vec::new(),
        true,
    );
}

#[test]
fn complete_driver_keeps_field_wrapper_cursor_reads_after_authored_gc() {
    compare_frames(
        r#"
        events={}
        return Def.ActorFrame{InitCommand=function(self)
            local field=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
            local first=field:AddWrapperState()
            local outer=field:AddWrapperState()
            local replacement=field:AddWrapperState();replacement:y(77)
            field.__songlua_wrappers={first,outer}
            outer.__songlua_state_spin=setmetatable({}, {__index=function(_,key)
                if key=='baked' then
                    events[#events+1]='cursor';field.__songlua_wrappers[1]=replacement
                    collectgarbage();return false
                end
            end})
            self:SetUpdateFunction(function()end)
        end}
    "#,
        Vec::new(),
        false,
        0,
    )
    .unwrap();
}

#[test]
fn complete_driver_uses_exact_song_timing_origin_rate_and_frame_clocks() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let out = compare_context(
        r#"
        events={}
        return Def.ActorFrame{Name='clock',Def.Quad{},InitCommand=function(self)
            timed_actor=self
            local player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            self:SetUpdateFunction(function(self,dt)
                local beat=GAMESTATE:GetSongBeat()
                events[#events+1]=string.format('%.8f/%.8f/%.8f/%.8f',beat,
                    GAMESTATE:GetSongBPS(),GAMESTATE:GetSongPosition():GetMusicSeconds(),dt)
                self:x(beat*3):aux(dt);player:x(beat*7)
                collectgarbage()
            end)
        end}
    "#,
        Vec::new(),
        false,
        8,
        |context| {
            context.song_music_rate = 1.5;
            context.song_timing = Some(TimingData::from_segments(
                1.25,
                0.,
                &TimingSegments {
                    bpms: vec![(0., 150.), (0.2, 180.)],
                    ..Default::default()
                },
                &[],
            ));
            let timing = context.song_timing.as_ref().unwrap();
            let origin = timing.get_time_for_beat_exact(0.);
            for (i, frame) in context.compile_clock_inputs.iter_mut().enumerate() {
                let music = f64::from(origin) + i as f64 / 60. * f64::from(context.song_music_rate);
                frame[0] = music;
                frame[1] = f64::from(timing.get_song_position(music as f32).beat);
            }
        },
    )
    .unwrap();
    assert!(!out.overlay_tracks.is_empty());
    assert!(
        out.eases
            .iter()
            .all(|window| window.unit == deadsync_song_lua::SongLuaTimeUnit::Second)
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
