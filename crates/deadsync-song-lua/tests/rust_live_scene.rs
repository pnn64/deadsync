use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame,
    rust_engine::{RustSongRuntime, SongLayerPlan, SongLiveScene, SongLiveSceneBindings},
};
use std::{collections::HashMap, path::Path, sync::Arc};

fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
fn write_beep(directory: &Path) {
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
    std::fs::write(directory.join("beep.wav"), wav).unwrap();
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
    rust: SongLiveScene,
    rust_owners: HashMap<u64, String>,
    _directory: tempfile::TempDir,
}
fn rust_scene(context: &SongLuaCompileContext, paths: &[&Path]) -> SongLiveScene {
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
    rust
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
        reference_test::register_path(directory.path());
        write_beep(directory.path());
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

        let rust = rust_scene(&context, &paths);
        let mut pair = Self {
            rust,
            rust_owners: HashMap::new(),
            _directory: directory,
        };
        for name in ["ROOT", "Q", "SA", "SB"] {
            let mut runtime = pair.rust.runtime_mut();
            let vm = runtime.vm_mut();
            let value = vm.get_global(name.as_bytes()).unwrap();
            if let Value::Table(_) = value {
                pair.rust_owners
                    .insert(vm.object_pointer(value).unwrap() as u64, name.to_owned());
            }
        }

        let rust = pair.rust.take_startup_output();
        pair.output(rust);
        if require_root {
            pair.run("assert(ROOT~=nil and Q~=nil)");
        }
        pair.run("S=STATSMAN:GetCurStageStats(); P={S:GetPlayerStageStats(0),S:GetPlayerStageStats(1)}; EVENTS={}");
        pair.compare(SNAPSHOT);
        pair
    }
    fn compare(&mut self, source: &str) {
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
        reference_test::assert_debug("song/tests/rust_live_scene.rs::observations", &actual);
    }
    fn run(&mut self, source: &str) {
        self.compare(source)
    }
    fn prepare(&mut self, frame: &SongLuaLiveFrame) {
        self.rust.prepare_gameplay_frame(frame.clone()).unwrap();
        self.compare(SNAPSHOT);
    }
    fn update(&mut self, frame: &SongLuaLiveFrame) {
        let rust = self.rust.update(frame.clone()).unwrap();
        self.output(rust);
        self.compare(SNAPSHOT);
        self.states();
    }
    fn draw(&mut self) {
        let rust = self.rust.draw().unwrap();
        self.output(rust);
        self.compare(SNAPSHOT);
        self.states();
    }
    fn states(&mut self) {
        reference_test::assert_debug(
            "song/tests/rust_live_scene.rs::states",
            &(
                self.rust.outro_off_dispatched(),
                self.rust.failed_outro(),
                self.rust.states(),
                live_snapshot::tracked(self.rust.tracked_states()),
            ),
        );
    }
    fn output(&mut self, output: song::SongLuaLiveOutput) {
        reference_test::assert_debug(
            "song/tests/rust_live_scene.rs::output",
            &live_snapshot::scene_output(&output),
        );
        let sounds = output
            .actor_sounds
            .into_iter()
            .map(|event| match event {
                song::SongLuaActorSound::Play { owner, path } => {
                    format!("{} play {}", self.rust_owners[&owner], path.display())
                }
                song::SongLuaActorSound::Pause { owner, paused } => {
                    format!("{} pause {paused}", self.rust_owners[&owner])
                }
                song::SongLuaActorSound::Stop { owner } => {
                    format!("{} stop", self.rust_owners[&owner])
                }
            })
            .collect::<Vec<_>>();
        reference_test::assert_debug(
            "song/tests/rust_live_scene.rs::actor_sounds",
            &("sounds", &sounds),
        );
    }
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.rust.end().unwrap();
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

    let r = p.rust.end().unwrap();
    p.output(r);
    p.compare(SNAPSHOT);
    p.run("collectgarbage('collect');collectgarbage('collect')");
    assert!(p.rust.update(frame(0.3)).is_err());

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

    let (rh, r) = p.rust.input(&event).unwrap();
    reference_test::assert_debug("song/tests/rust_live_scene.rs::handled", &rh);
    p.output(r);
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
    reference_test::register_path(directory.path());
    let path = directory.path().join("default.lua");
    let source = SOURCE.replace(
        "Q:x(Q:GetX()+d)",
        "Q:x(Q:GetX()+d);SCREENMAN:SetNewScreen('ScreenGameplay')",
    );
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "original live startup fixture");
    context.live = true;
    let result = song::rust_engine::compile_song_lua_layers(
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

    let rust = p.rust.end().unwrap();
    p.output(rust);
    p.compare(SNAPSHOT);
}

#[test]
fn gameplay_outro_noted_after_actors_uses_the_same_off_boundary() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.update(&frame(0.1));

    p.rust.note_gameplay_outro_after_actors();
    p.update(&frame(0.2));
    p.update(&frame(0.3));
    let entry = p._directory.path().join("layer-0.lua");
    assert!(p.rust.matches_entries(&[&entry], 0));

    assert!(!p.rust.matches_entries(&[&entry], 1));

    let rust = p.rust.selected_context().unwrap();

    reference_test::assert_debug(
        "song/tests/rust_live_scene.rs::selected",
        &(&rust.style_name, &rust.player_step_indices),
    );
}

#[test]
fn a_rejected_broadcast_history_write_does_not_capture_a_source_event() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("local history=__songlua_broadcast_messages;__songlua_broadcast_messages=false;assert(not pcall(MESSAGEMAN.Broadcast,MESSAGEMAN,'Rejected',{a=1}));__songlua_broadcast_messages=history");
    p.update(&frame(0.1));
}

#[test]
fn ended_scene_handoff_preserves_saved_actors_without_duplicate_sound_stops() {
    let directory = tempfile::tempdir().unwrap();
    reference_test::register_path(directory.path());
    write_beep(directory.path());
    let path = directory.path().join("handoff.lua");
    let source = SOURCE.replace(
        "Def.Quad{Name='Q'",
        "Def.Sound{Name='SA',File='beep.wav',InitCommand=function(self)SA=self end},Def.Quad{Name='Q'",
    );
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "live handoff");
    context.live = true;
    let mut scene = rust_scene(&context, &[&path]);
    assert!(scene.take_startup_output().actor_sounds.is_empty());
    scene.update(frame(0.1)).unwrap();
    let mut runtime = scene.runtime_mut();
    let vm = runtime.vm_mut();
    let sound = vm.get_global(b"SA").unwrap();
    let owner = vm.object_pointer(sound).unwrap() as u64;
    drop(runtime);
    assert_eq!(
        scene.end().unwrap().actor_sounds,
        [song::SongLuaActorSound::Stop { owner }]
    );
    let owner = scene.into_runtime().unwrap();
    let mut runtime = owner.access();
    assert!(
        runtime.take_actor_sounds().is_empty(),
        "End already stopped this owner"
    );
    runtime
        .vm_mut()
        .execute(
            &compile_named(
                br#"
        SAVED=Q;ROOT=nil;Q=nil;SA=nil;collectgarbage('collect');collectgarbage('collect')
        assert(SAVED:GetName()=='Q');SAVED:x(17);assert(SAVED:GetX()==17)
        SAVED=nil;collectgarbage('collect');collectgarbage('collect')
    "#,
                b"@/live-handoff-check.lua",
            )
            .unwrap(),
        )
        .unwrap();
    assert!(runtime.take_actor_sounds().is_empty());
}

#[test]
fn a_sound_without_a_file_keeps_the_existing_compiled_owner_filter() {
    let source = SOURCE.replace(
        "Def.Quad{Name='Q'",
        "Def.Sound{Name='SA',InitCommand=function(self)SA=self end},Def.Quad{Name='Q'",
    );
    let mut pair = Pair::new(&[&source], |_| {});
    let rust = pair.rust.end().unwrap();
    assert!(
        rust.actor_sounds.is_empty(),
        "the fileless Sound has no compiled visual owner"
    );
    pair.output(rust);
}

#[test]
fn replay_option_deltas_are_independent_consumed_once_and_survive_a_prepared_frame() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run(
        r#"
for _,pn in ipairs({PLAYER_1,PLAYER_2}) do
 local ps=GAMESTATE:GetPlayerState(pn)
 ps:GetCurrentPlayerOptions():Drunk(0)
 ps:GetPlayerOptions('ModsLevel_Song'):Drunk(1,1)
end
GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(1)
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2,1)
"#,
    );

    p.rust.set_option_clock_deltas([0.03125, 0.0625, 0.125]);
    p.prepare(&frame(0.1));

    p.rust.set_option_clock_deltas([0.015625, 0.03125, 0.0625]);
    p.prepare(&frame(0.1));
    p.update(&frame(0.1));
    p.compare(
        r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.03125)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.0625)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.125)
"#,
    );
    p.update(&frame(0.2));
    p.compare(
        r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.046875)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.09375)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.1875)
"#,
    );
    p.update(&frame(0.3));
    p.compare(
        r#"
return GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(),
GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk(),
GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()
"#,
    );
}

#[test]
fn replay_overlay_observation_precedes_foreground_writes_and_survives_gc() {
    let mut p = Pair::new(&[SOURCE], |context| {
        context.song_steps = [
            song::SongLuaDifficulty::Challenge,
            song::SongLuaDifficulty::Edit,
        ]
        .into_iter()
        .map(|difficulty| song::SongLuaStep {
            steps_type: "StepsType_Dance_Single".into(),
            difficulty,
            description: format!("{difficulty:?}"),
            chart_name: String::new(),
            credit: String::new(),
            meter: 9,
            music_file: String::new(),
            possible_grade_points: 100,
        })
        .collect();
        context.player_step_indices = [Some(0), Some(0)];
    });

    p.rust.enable_overlay_stage_observer().unwrap();
    p.run(
        r#"
ROOT:SetUpdateFunction(function()
 GAMESTATE:SetCurrentSteps(PLAYER_1,GAMESTATE:GetCurrentSong():GetAllSteps()[2])
 SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(0.2)
end)
collectgarbage('collect');collectgarbage('collect')
"#,
    );
    p.update(&frame(0.1));

    let rust = p.rust.take_overlay_stage_observation().unwrap();
    reference_test::assert_debug("song/tests/rust_live_scene.rs::stage", &rust);
    assert_eq!(rust.player_step_indices[0], Some(0));
    assert_eq!(rust.players[0].life, 0.5);
    assert_eq!(
        p.rust.selected_context().unwrap().player_step_indices[0],
        Some(1)
    );
    assert!(p.rust.take_overlay_stage_observation().is_none());

    p.update(&frame(0.2));
    reference_test::assert_debug(
        "song/tests/rust_live_scene.rs::next_stage",
        &("next", &p.rust.take_overlay_stage_observation()),
    );
}

#[test]
fn replay_captured_subscriber_paths_preserve_the_native_order() {
    let source = SOURCE.replace(
        "Def.Quad{Name='Q'",
        r#"
Def.Actor{Name='Other',ProbeMessageCommand=function()SEEN=(SEEN or '')..'O' end},
Def.Quad{Name='Q',ProbeMessageCommand=function()SEEN=(SEEN or '')..'Q' end"#,
    );
    let mut p = Pair::new(&[&source], |context| {
        context.message_actor_order = vec![("1/1".into(), 20), ("1/2".into(), 10)];
    });
    p.run("SEEN='';MESSAGEMAN:Broadcast('Probe');assert(SEEN=='QO');return SEEN");
    p.run("collectgarbage('collect');SEEN='';MESSAGEMAN:Broadcast('Probe');assert(SEEN=='QO')");
}

#[test]
fn replay_missing_subscriber_identity_dispatches_and_reports_coverage_once() {
    let source = SOURCE
        .replace("return Def.ActorFrame{Name='Root',", "return Def.ActorFrame{Name='Root',ProbeMessageCommand=function()CALLS=(CALLS or 0)+1 end,")
        .replace("Def.Quad{Name='Q'", "Def.Quad{Name='Q',ProbeMessageCommand=function()CALLS=(CALLS or 0)+1 end");
    let mut p = Pair::new(&[&source], |context| {
        context.message_actor_order = vec![("1".into(), 10)];
    });
    p.run("CALLS=0;MESSAGEMAN:Broadcast('Probe');assert(CALLS==2)");

    let rust = p.rust.update(frame(0.1)).unwrap();
    assert_eq!(
        rust.coverage_gaps,
        ["missing captured subscriber identity for 1/1"]
    );
    p.output(rust);
    p.compare(SNAPSHOT);
    p.run("collectgarbage('collect');MESSAGEMAN:Broadcast('Probe');assert(CALLS==4)");

    let rust = p.rust.update(frame(0.2)).unwrap();
    assert!(rust.coverage_gaps.is_empty());
    p.output(rust);
    p.compare(SNAPSHOT);
}

#[path = "support/live_snapshot.rs"]
mod live_snapshot;
#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
