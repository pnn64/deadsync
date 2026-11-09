use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaMessageEvent,
    rust_engine::{RustSongRuntime, SongLayerPlan, SongUpdateOutput, test_support},
};
use std::sync::Arc;

fn compare(source: &str, messages: Vec<SongLuaMessageEvent>, expect_error: bool) -> Option<SongUpdateOutput> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "chronological compiler");
    context.music_length_seconds = 0.2;
    context.compile_clock_inputs = (0..=12).map(|i| [i as f64 / 60., i as f64 / 30.]).collect();
    let native = test_support::legacy_song_compilation_vm();
    let activation = test_support::legacy_activate_song_layers(&native, &[&path], 0, &context).unwrap();
    let mut rust = RustSongRuntime::new(&context).unwrap();
    rust.begin_song_compilation().unwrap();
    let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
    rust.load_gameplay_theme(screen, &selected).unwrap();
    let root = rust.initialize_song_layers(screen, &SongLayerPlan::new(&[&path], 0, &context).unwrap()).unwrap().unwrap();
    let rust_activation = rust.activate_song_layers(screen, &context).unwrap().unwrap();
    test_support::legacy_discover_song_startup_commands(&native, &activation, false).unwrap();
    rust.discover_song_startup_commands(screen, &rust_activation).unwrap();
    let mut native_overlays = test_support::legacy_read_song_overlay_kinds::<(), ()>(&native, activation.root.clone(), &context, |_,_,_| Ok(Arc::from([])), |_| None).unwrap();
    let mut overlays = rust.read_song_overlay_actors::<(), ()>(root, &context, |_,_,_| Ok(Arc::from([])), |_| None, |_| {}).unwrap();
    let mut native_tracked = test_support::legacy_read_song_tracked_actors(&native).unwrap();
    let mut tracked = rust.read_song_tracked_actors(screen).unwrap().unwrap();
    let mut native_messages = messages.clone();
    let mut rust_messages = messages;
    test_support::legacy_prepare_update_startup(&mut native_overlays, &activation, &context, &mut native_messages);
    test_support::prepare_update_startup(&mut rust, &mut overlays, &rust_activation, &context, &mut rust_messages).unwrap();
    assert_eq!(native_messages, rust_messages);
    let (mut a_sound,mut b_sound,mut a_judgment,mut b_judgment,mut a_sprite,mut b_sprite)=(Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new());
    let a = test_support::legacy_compile_update_functions(&native,&activation,&context,&mut native_overlays,&mut native_tracked,&native_messages,&mut a_sound,&mut a_judgment,&mut a_sprite);
    let b = rust.compile_song_update_functions(screen,root,&context,&mut overlays,&mut tracked,&rust_messages,&rust_activation,&mut b_sound,&mut b_judgment,&mut b_sprite);
    assert_eq!(a.is_err(),expect_error,"native error: {:?}",a.as_ref().err());
    assert_eq!(b.is_err(),expect_error,"Rust error: {:?}",b.as_ref().err());
    if expect_error { return None; }
    let (a,b)=(a.unwrap(),b.unwrap());
    assert_eq!(a.eases,b.eases,"player/mod windows");
    assert_eq!(a.overlay_eases,b.overlay_eases,"overlay eases");
    assert_eq!(a.overlay_tracks,b.overlay_tracks,"renderer tracks");
    assert_eq!(a.column_transforms,b.column_transforms,"column transforms");
    assert_eq!(a.column_splines,b.column_splines,"column splines");
    assert_eq!(a.stateful_messages,b.stateful_messages,"message provenance");
    assert_eq!(a.broadcasts,b.broadcasts,"broadcast order");
    assert_eq!(a_sound,b_sound,"sound drains");
    assert_eq!(a_judgment,b_judgment,"judgment drains");
    assert_eq!(a_sprite.len(),b_sprite.len());
    assert_eq!(native_overlays.len(),overlays.actors.len(),"runtime wrappers");
    for (i,(a,b)) in native_overlays.iter().zip(&overlays.actors).enumerate() {
        assert_eq!(a.actor.name,b.actor.name,"actor {i}");
        assert_eq!(a.actor.parent_index,b.actor.parent_index,"parent {i}");
        assert_eq!(a.actor.initial_state,b.actor.initial_state,"initial state {i}");
        assert_eq!(a.actor.message_commands,b.actor.message_commands,"commands {i}");
    }
    assert_eq!(native_tracked.len(),tracked.actors.len());
    for (i,(a,b)) in native_tracked.iter().zip(&tracked.actors).enumerate() {assert_eq!(a.actor,b.actor,"tracked actor {i}");}
    let a: String = native.load("return table.concat(events or {},'|')").eval().unwrap();
    let Value::String(id) = rust.vm_mut().execute(&compile_named(b"return table.concat(events or {},'|')",b"=chronological events").unwrap()).unwrap()[0] else {panic!()};
    assert_eq!(a.as_bytes(),rust.vm_mut().string_bytes(id).unwrap(),"script state/order");
    rust.release_song_overlay_actors(&mut overlays).unwrap();
    rust.release_song_tracked_actors(&mut tracked).unwrap();
    rust.release_actor_scene(screen).unwrap();
    rust.vm_mut().collect_garbage().unwrap();
    drop(rust);
    Some(b)
}

#[test]
fn complete_driver_captures_parent_child_callbacks_and_getter_feedback() {
    let output = compare(r#"
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
    "#,Vec::new(),false).unwrap();
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
                p:GetChild('NoteField'):GetColumnActors()[1]:rotationz(b*9)
            end)
        end}
    "#,Vec::new(),false).unwrap();
    assert!(!output.eases.is_empty());
    assert!(!output.column_transforms.is_empty());
}

#[test]
fn complete_driver_appends_runtime_wrappers_and_remaps_their_tracks() {
    let output=compare(r#"
        events={};saved={}
        return Def.Quad{Name='wrapped',InitCommand=function(self)
            saved.actor=self
            self:SetUpdateFunction(function(self,dt)
                if dt>0 and not self.outer then
                    self.outer=self:AddWrapperState();self.outer:linear(.08):x(12)
                    self.inner=self:AddWrapperState();self.inner:rotationz(5)
                end
            end)
        end}
    "#,Vec::new(),false).unwrap();
    assert!(!output.overlay_tracks.is_empty());
}

#[test]
fn complete_driver_captures_notefield_wrappers_motion_and_hold_frames() {
    compare(r#"
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
    "#,Vec::new(),false).unwrap();
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
    compare("events={};return Def.Quad{InitCommand=function(self)self:x(4):zoom(1.5)end}",Vec::new(),false).unwrap();
}

#[test]
fn complete_driver_reports_frame_metadata_errors() {
    compare(r#"
        events={}
        return Def.Quad{InitCommand=function(self)self:SetUpdateFunction(function(self,dt)
            if dt>0 then self.__songlua_player_index={};self:diffusealpha(.3) end
        end)end}
    "#,Vec::new(),true);
}
