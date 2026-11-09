use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveScene,
    rust_engine::{
        RustSongRuntime, SongLayerPlan, SongLiveScene, SongLiveSceneBindings, test_support,
    },
};
use std::{collections::HashMap, path::Path, sync::Arc};

fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn numeric(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    native: SongLuaLiveScene,
    rust: SongLiveScene,
    native_owners: HashMap<u64, String>,
    rust_owners: HashMap<u64, String>,
    _directory: tempfile::TempDir,
}
impl Pair {
    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        Self::build(sources, configure, true)
    }
    fn build(
        sources: &[&str],
        configure: impl FnOnce(&mut SongLuaCompileContext),
        require_root: bool,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(38u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(32000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(2u32.to_le_bytes());
        wav.extend(0i16.to_le_bytes());
        std::fs::write(directory.path().join("beep.wav"), wav).unwrap();
        let paths: Vec<_> = sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let p = directory.path().join(format!("layer-{i}.lua"));
                std::fs::write(&p, s).unwrap();
                p
            })
            .collect();
        let paths: Vec<_> = paths.iter().map(|p| p.as_path()).collect();
        let mut context = SongLuaCompileContext::new(directory.path(), "live layers");
        context.live = true;
        context.players[1].enabled = true;
        context.music_length_seconds = 2.;
        configure(&mut context);
        let mut outputs = song::compile_song_lua_layers_with_default_host(
            &paths,
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        let native = outputs[0].live_scene.take().unwrap();
        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_live_song_load();
        rust.begin_song_compilation().unwrap();
        let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen, &selected).unwrap();
        let plan = SongLayerPlan::new(&paths, 0, &context).unwrap();
        let root = rust.initialize_song_layers(screen, &plan).unwrap().unwrap();
        let activation = rust
            .activate_song_layers(screen, &selected)
            .unwrap()
            .unwrap();
        let mut overlays = rust
            .read_song_overlay_actors(root, &selected, slots, layer, |_| {})
            .unwrap();
        let mut tracked = rust.read_song_tracked_actors(screen).unwrap().unwrap();
        let mut actors: Vec<_> = overlays
            .actors
            .iter()
            .map(|a| rust.song_overlay_actor_value(a).unwrap())
            .collect();
        actors.push(rust.actor_scene_root(screen).unwrap());
        let tracks = tracked
            .actors
            .iter()
            .map(|a| (a.target, rust.song_tracked_actor_value(a).unwrap()))
            .collect();
        let mut rust = SongLiveScene::new(
            rust,
            &selected,
            SongLiveSceneBindings {
                screen,
                root,
                actors,
                tracked: tracks,
                entry_paths: paths.iter().map(|p| p.to_path_buf()).collect(),
                primary_index: 0,
                startup_broadcasts: activation.broadcasts,
            },
        )
        .unwrap();
        rust.runtime_mut()
            .release_song_overlay_actors(&mut overlays)
            .unwrap();
        rust.runtime_mut()
            .release_song_tracked_actors(&mut tracked)
            .unwrap();
        let mut pair = Self {
            native,
            rust,
            native_owners: HashMap::new(),
            rust_owners: HashMap::new(),
            _directory: directory,
        };
        for name in ["ROOT", "Q", "SA", "SB"] {
            let value: mlua::Value = test_support::legacy_live_scene_lua(&pair.native)
                .globals()
                .get(name)
                .unwrap();
            if let mlua::Value::Table(actor) = value {
                pair.native_owners
                    .insert(actor.to_pointer() as u64, name.to_owned());
            }
            let vm = pair.rust.runtime_mut().vm_mut();
            let value = vm.get_global(name.as_bytes()).unwrap();
            if let Value::Table(_) = value {
                pair.rust_owners
                    .insert(vm.object_pointer(value).unwrap() as u64, name.to_owned());
            }
        }
        let native = pair.native.take_startup_output();
        let rust = pair.rust.take_startup_output();
        pair.output(native, rust);
        if require_root {
            pair.run("assert(ROOT~=nil and Q~=nil)");
        }
        pair.run("S=STATSMAN:GetCurStageStats(); P={S:GetPlayerStageStats(0),S:GetPlayerStageStats(1)}; EVENTS={}");
        pair.compare(SNAPSHOT);
        pair
    }
    fn compare(&mut self, source: &str) {
        let expected: mlua::MultiValue = test_support::legacy_live_scene_lua(&self.native)
            .load(source)
            .set_name("@/live-frame-check.lua")
            .call(())
            .unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|v| match v {
                mlua::Value::Nil => Scalar::Nil,
                mlua::Value::Boolean(v) => Scalar::Boolean(v),
                mlua::Value::Integer(v) => numeric(v as f64),
                mlua::Value::Number(v) => numeric(v),
                mlua::Value::String(v) => Scalar::String(v.as_bytes().to_vec()),
                v => panic!("scalar expected: {v:?}"),
            })
            .collect();
        let values = self
            .rust
            .runtime_mut()
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/live-frame-check.lua").unwrap())
            .unwrap()
            .to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(v) => Scalar::Boolean(v),
                Value::Number(v) => numeric(v),
                Value::String(id) => Scalar::String(
                    self.rust
                        .runtime_mut()
                        .vm_mut()
                        .string_bytes(id)
                        .unwrap()
                        .to_vec(),
                ),
                v => panic!("scalar expected: {v:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "{source}");
    }
    fn run(&mut self, source: &str) {
        self.compare(source)
    }
    fn prepare(&mut self, frame: &SongLuaLiveFrame) {
        self.native.prepare_gameplay_frame(frame.clone()).unwrap();
        self.rust.prepare_gameplay_frame(frame.clone()).unwrap();
        self.compare(SNAPSHOT);
    }
    fn update(&mut self, frame: &SongLuaLiveFrame) {
        let native = self.native.update(frame.clone()).unwrap();
        let rust = self.rust.update(frame.clone()).unwrap();
        self.output(native, rust);
        self.compare(SNAPSHOT);
        self.states();
    }
    fn draw(&mut self) {
        let native = self.native.draw().unwrap();
        let rust = self.rust.draw().unwrap();
        self.output(native, rust);
        self.compare(SNAPSHOT);
        self.states();
    }
    fn states(&mut self) {
        assert_eq!(
            self.rust.outro_off_dispatched(),
            self.native.outro_off_dispatched()
        );
        assert_eq!(self.rust.failed_outro(), self.native.failed_outro());
        assert_eq!(
            self.rust.states(),
            self.native.states(),
            "overlay current render states"
        );
        let target = |(t, s)| {
            use song::SongLuaTrackedActorTarget as T;
            let t = match t {
                T::Player(i) => (0, i),
                T::PlayerJudgment(i) => (1, i),
                T::PlayerCombo(i) => (2, i),
                T::SongForeground => (3, 0),
                T::ScreenLayer(i) => (4, i),
            };
            (t, s)
        };
        let expected: Vec<_> = self.native.tracked_states().map(target).collect();
        let actual: Vec<_> = self.rust.tracked_states().map(target).collect();
        assert_eq!(actual, expected, "tracked current render states");
    }
    fn output(&mut self, native: song::SongLuaLiveOutput, rust: song::SongLuaLiveOutput) {
        assert_eq!(
            format!("{:?}", rust.request),
            format!("{:?}", native.request)
        );
        assert_eq!(
            format!("{:?}", rust.source_requests),
            format!("{:?}", native.source_requests)
        );
        assert_eq!(rust.broadcasts, native.broadcasts);
        assert_eq!(rust.sounds, native.sounds);
        assert_eq!(rust.player_writes, native.player_writes);
        assert_eq!(
            rust.errors.len(),
            native.errors.len(),
            "native {:?}; Rust {:?}",
            native.errors,
            rust.errors
        );
        assert_eq!(rust.coverage_gaps, native.coverage_gaps);
        let source = |v: Vec<song::SongLuaLiveBroadcast>| {
            v.into_iter()
                .map(|v| (v.beat, v.music_seconds, v.name, v.params))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            source(rust.source_broadcasts),
            source(native.source_broadcasts)
        );
        assert_eq!(rust.draw_frames, native.draw_frames);
        let sounds = |events: Vec<song::SongLuaActorSound>, owners: &HashMap<u64, String>| {
            events
                .into_iter()
                .map(|event| match event {
                    song::SongLuaActorSound::Play { owner, path } => {
                        format!("{} play {}", owners[&owner], path.display())
                    }
                    song::SongLuaActorSound::Pause { owner, paused } => {
                        format!("{} pause {paused}", owners[&owner])
                    }
                    song::SongLuaActorSound::Stop { owner } => format!("{} stop", owners[&owner]),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sounds(rust.actor_sounds, &self.rust_owners),
            sounds(native.actor_sounds, &self.native_owners)
        );
    }
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.rust.end().unwrap();
        self.native.end().unwrap();
    }
}

const SNAPSHOT: &str = r#"
local out={};local n=0;local function add(v)n=n+1;out[n]=v==nil and 'nil' or v end
local r=__songlua_compile_song_runtime
for _,k in ipairs{'__songlua_song_beat','__songlua_music_seconds','__songlua_uptime_seconds'}do add(r[k])end
add(table.concat(EVENTS or {},';'))
for i=0,1 do local ps=GAMESTATE:GetPlayerState(i);add(ps:GetHealthState());add(ps:GetPlayerOptions('ModsLevel_Current'):Mini())end
return unpack(out,1,n)
"#;
const SOURCE: &str = r#"
EVENTS=EVENTS or {};local tick=0
return Def.ActorFrame{Name='Root',InitCommand=function(self)ROOT=self end,
OnCommand=function(self)
 self:SetUpdateFunction(function(self,d)tick=tick+1;EVENTS[#EVENTS+1]='update:'..tick;Q:x(Q:GetX()+d)end)
 SCREENMAN:GetTopScreen():AddInputCallback(function(e)EVENTS[#EVENTS+1]='input:'..e.GameButton;Q:visible(not Q:GetVisible());return false end)
end,
OffCommand=function(self)EVENTS[#EVENTS+1]='off' end,
EndCommand=function(self)EVENTS[#EVENTS+1]='end' end,
Def.Quad{Name='Q',InitCommand=function(self)Q=self;self:setsize(8,12)end}}
"#;
fn frame(t: f64) -> SongLuaLiveFrame {
    let mut f = SongLuaLiveFrame {
        beat: t * 2.,
        music_seconds: t,
        screen_seconds: t,
        delta_seconds: 0.1,
        ..Default::default()
    };
    f.players[0].life = 0.5;
    f.players[1].life = 0.5;
    f
}

#[test]
fn retained_scene_update_prepare_draw_end_and_owned_render_states_match() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("collectgarbage('collect');collectgarbage('collect')");
    p.draw();
    let f = frame(0.1);
    p.prepare(&f);
    p.prepare(&frame(0.2));
    p.run("collectgarbage('collect');collectgarbage('collect')");
    p.update(&f);
    p.draw();
    p.run("collectgarbage('collect');collectgarbage('collect')");
    p.update(&frame(0.2));
    p.draw();
    let n = p.native.end().unwrap();
    let r = p.rust.end().unwrap();
    p.output(n, r);
    p.compare(SNAPSHOT);
    p.run("collectgarbage('collect');collectgarbage('collect')");
    assert!(p.rust.update(frame(0.3)).is_err());
    assert!(p.native.update(frame(0.3)).is_err());
    p.draw();
}
#[test]
fn authored_startup_and_runtime_tween_queues_keep_current_render_poses() {
    let s = SOURCE.replace(
        "OnCommand=function(self)",
        "OnCommand=function(self) self:linear(1):x(100)",
    );
    let mut p = Pair::new(&[&s], |_| {});
    for t in [0.1, 0.2, 0.5, 1., 1.5] {
        p.update(&frame(t));
    }
}
#[test]
fn scene_outro_clears_attacks_and_orders_success_and_all_failed_off() {
    for failed in [false, true] {
        let mut p = Pair::new(&[SOURCE], |_| {});
        for (i, outro) in [false, true, true, true, true].into_iter().enumerate() {
            let mut f = frame(i as f64 * 0.1);
            f.gameplay_outro = outro;
            for player in &mut f.players {
                player.failed = failed;
            }
            p.update(&f);
        }
    }
}
#[test]
fn prepared_background_failure_retries_the_host_phase_without_a_stale_prepared_mark() {
    let s = SOURCE.replace(
        "OnCommand=function(self)",
        "OnCommand=function(self) self:sleep(0.2)",
    );
    let mut p = Pair::new(&[&s], |c| {
        c.background_layer_count = 1;
        c.layer_start_beats = vec![0.];
    });
    p.run("ROOT.__songlua_state_text_distortion={}");
    let f = frame(0.1);
    assert!(p.native.prepare_gameplay_frame(f.clone()).is_err());
    assert!(p.rust.prepare_gameplay_frame(f.clone()).is_err());
    p.compare(SNAPSHOT);
    p.run("ROOT.__songlua_state_text_distortion=nil");
    p.prepare(&f);
    p.update(&f);
}
#[test]
fn live_outputs_preserve_explicit_nil_tables_nested_messages_and_requests() {
    let s=SOURCE.replace("Q:x(Q:GetX()+d)","Q:x(Q:GetX()+d);MESSAGEMAN:Broadcast('Nested',{tick=tick,list={1,2},flag=true});if d>0 then SCREENMAN:SetNewScreen('ScreenGameplay');SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_GoToNextScreen')end ");
    let mut p = Pair::new(&[&s], |_| {});
    p.run("ROOT:addcommand('NestedMessage',function(self,params)params.tick=99;MESSAGEMAN:Broadcast('Inner');MESSAGEMAN:Broadcast('ExplicitNil',nil)end)");
    p.update(&frame(0.1));
    p.update(&frame(0.2));
}
#[test]
fn health_callbacks_write_the_selected_statistics_and_freeze_poses_before_health() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("ROOT:addcommand('HealthStateChangedMessage',function(self,params)Q:x(77);SCREENMAN:GetTopScreen():GetChild('PlayerP2'):SetLife(0.9)end)");
    let mut f = frame(0.1);
    f.players[0].life = 0.1;
    p.update(&f);
    p.draw();
}

#[test]
fn input_and_custom_draw_update_states_and_drain_in_execution_order() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("ROOT:SetDrawFunction(function(self)Q:x(Q:GetX()+1):Draw()end)");
    p.update(&frame(0.1));
    p.draw();
    let event = song::SongLuaInputEvent {
        player: Some(0),
        controller: Some(0),
        button: "Up",
        game_button: "Up",
        kind: song::SongLuaInputType::FirstPress,
        device: "Keyboard",
        device_button: "Up",
        level: 1.,
        z: 0,
        down: true,
        ago: 0.,
        is_joystick: false,
        is_mouse: false,
    };
    let (nh, n) = p.native.input(&event).unwrap();
    let (rh, r) = p.rust.input(&event).unwrap();
    assert_eq!(rh, nh);
    p.output(n, r);
    p.states();
    p.compare(SNAPSHOT);
    p.draw();
}
#[test]
fn malformed_raw_pose_fields_do_not_override_native_owned_live_tweens() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.update(&frame(0.1));
    p.run("Q.__songlua_state_x={};Q.__songlua_state_diffuse=false;Q.__songlua_state_basezoom='bad';ROOT:SetUpdateFunction(nil)");
    p.update(&frame(0.2));
}
#[test]
fn authored_statistics_selection_before_scene_construction_governs_live_writes() {
    let setup = r#"
    ORIGINAL=STATSMAN:GetCurStageStats();SELECTED={}
    for i=1,2 do local old=ORIGINAL:GetPlayerStageStats(i-1);local copy={};for k,v in pairs(old)do copy[k]=v end;SELECTED[i]=copy end
    local stage={};for k,v in pairs(ORIGINAL)do stage[k]=v end
    stage.GetPlayerStageStats=function(_,p)if p==0 or p=='PlayerNumber_P1'then return SELECTED[1]else return SELECTED[2]end end
    STATSMAN.GetCurStageStats=function()return stage end
    "#;
    let s = SOURCE.replace(
        "OnCommand=function(self)",
        &format!("OnCommand=function(self) {setup}"),
    );
    let mut p = Pair::new(&[&s], |_| {});
    p.run("ROOT:SetUpdateFunction(function()SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(0.75)end)");
    p.update(&frame(0.1));
    p.run("assert(SELECTED[1].__songlua_life==0.75);assert(ORIGINAL:GetPlayerStageStats(0).__songlua_life~=0.75)");
}
#[test]
fn malformed_live_source_keeps_the_existing_dummy_actor_fallback() {
    let mut p = Pair::build(
        &["return Def.ActorFrame{OnCommand=function() endend}"],
        |_| {},
        false,
    );
    p.run("assert(ROOT==nil and Q==nil)");
    p.update(&frame(0.1));
}
#[test]
fn original_startup_request_fixture_retains_the_native_reload_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("default.lua");
    let source = SOURCE.replace(
        "Q:x(Q:GetX()+d)",
        "Q:x(Q:GetX()+d);SCREENMAN:SetNewScreen('ScreenGameplay')",
    );
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "original live startup fixture");
    context.live = true;
    let result = song::compile_song_lua_layers_with_default_host(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    );
    let error = match result {
        Ok(_) => panic!("the original fixture must reject before creating a live scene"),
        Err(e) => e,
    };
    assert!(error.contains("128 startup gameplay reloads"));
}

#[test]
fn immediate_draw_submissions_and_normal_draw_frames_keep_their_live_boundaries() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("Q:x(17):Draw(); ROOT:SetDrawFunction(function(self)Q:x(Q:GetX()+1):Draw()end)");
    p.update(&frame(0.1));
    p.draw();
    p.run("Q:Draw();Q:x(29):Draw()");
    p.draw();
    p.update(&frame(0.2));
    p.draw();
}

#[test]
fn source_broadcast_snapshots_keep_functions_actors_cycles_and_trace_limits() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run(r#"
        function sourcefn()return 1 end
        for _,v in ipairs{sourcefn,math.sin,coroutine.create(function()end),1/0}do
            assert(not pcall(MESSAGEMAN.Broadcast,MESSAGEMAN,'Invalid',v))
        end
        MESSAGEMAN:Broadcast('Closure',{value=sourcefn})
        MESSAGEMAN:Broadcast('HostFunction',{value=math.sin})
        MESSAGEMAN:Broadcast('Actor',Q)
        MESSAGEMAN:Broadcast('Dense',{1,2,'a\\b'})
        MESSAGEMAN:Broadcast('Mixed',{[true]=1,[3]=2,[-1]=3,name='ok'})
        MESSAGEMAN:Broadcast('Collision',{[true]=1,['true']=2})
        local cycle={};cycle.self=cycle;MESSAGEMAN:Broadcast('Cycle',cycle)
        local deep={};local next=deep;for i=1,6 do next.child={};next=next.child end;MESSAGEMAN:Broadcast('Depth',deep)
        local many={};for i=1,257 do many[i]=i end;MESSAGEMAN:Broadcast('Entries',many)
        MESSAGEMAN:Broadcast('Thread',{value=coroutine.create(function()end)})
        MESSAGEMAN:Broadcast('Infinity',{value=1/0})
    "#);
    p.update(&frame(0.1));
}

#[test]
fn health_callback_failures_are_reported_once_and_preserve_the_live_scene() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("P[1].GetLife=function()error('health failure')end");
    p.update(&frame(0.1));
    p.update(&frame(0.2));
    p.draw();
}

#[test]
fn retained_sound_owners_preserve_startup_play_pause_and_teardown_order() {
    let source = SOURCE.replace(
        "Def.Quad{Name='Q'",
        r#"
        Def.Sound{Name='SA',File='beep.wav',InitCommand=function(self)SA=self end,
            OnCommand=function(self)self:play()end},
        Def.Sound{Name='SB',File='beep.wav',InitCommand=function(self)SB=self end},
        Def.Quad{Name='Q'"#,
    );
    let mut p = Pair::new(&[&source], |_| {});
    p.run("assert(SA~=SB);ROOT:SetUpdateFunction(function()SA:pause(true);SB:play();SA:stop();SB:pause(false)end)");
    p.update(&frame(0.1));
    let native = p.native.end().unwrap();
    let rust = p.rust.end().unwrap();
    p.output(native, rust);
    p.compare(SNAPSHOT);
}

#[test]
fn gameplay_outro_noted_after_actors_uses_the_same_off_boundary() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.update(&frame(0.1));
    p.native.note_gameplay_outro_after_actors();
    p.rust.note_gameplay_outro_after_actors();
    p.update(&frame(0.2));
    p.update(&frame(0.3));
    let entry = p._directory.path().join("layer-0.lua");
    assert!(p.rust.matches_entries(&[&entry], 0));
    assert!(p.native.matches_entries(&[&entry], 0));
    assert!(!p.rust.matches_entries(&[&entry], 1));
    assert!(!p.native.matches_entries(&[&entry], 1));
    let rust = p.rust.selected_context().unwrap();
    let native = p.native.selected_context().unwrap();
    assert_eq!(rust.style_name, native.style_name);
    assert_eq!(rust.player_step_indices, native.player_step_indices);
}

#[test]
fn a_rejected_broadcast_history_write_does_not_capture_a_source_event() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("local history=__songlua_broadcast_messages;__songlua_broadcast_messages=false;assert(not pcall(MESSAGEMAN.Broadcast,MESSAGEMAN,'Rejected',{a=1}));__songlua_broadcast_messages=history");
    p.update(&frame(0.1));
}
