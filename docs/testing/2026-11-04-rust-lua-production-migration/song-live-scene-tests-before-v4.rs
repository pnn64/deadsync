use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveScene,
    rust_engine::{
        RustSongRuntime, SongLayerPlan, SongLiveScene, SongLiveSceneBindings, test_support,
    },
};
use std::{path::Path, sync::Arc};

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
    _directory: tempfile::TempDir,
}
impl Pair {
    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let directory = tempfile::tempdir().unwrap();
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
        let actors = overlays
            .actors
            .iter()
            .map(|a| rust.song_overlay_actor_value(a).unwrap())
            .collect();
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
            _directory: directory,
        };
        pair.run("S=STATSMAN:GetCurStageStats(); P={S:GetPlayerStageStats(0),S:GetPlayerStageStats(1)}; EVENTS={}");
        pair.compare(SNAPSHOT);
        pair
    }
    fn compare(&mut self, source: &str) {
        let expected: mlua::MultiValue = test_support::legacy_live_scene_lua(&self.native)
            .load(source)
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
        assert_eq!(rust.actor_sounds.len(), native.actor_sounds.len());
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
    p.draw();
    let f = frame(0.1);
    p.prepare(&f);
    p.prepare(&frame(0.2));
    p.update(&f);
    p.draw();
    p.update(&frame(0.2));
    p.draw();
    let n = p.native.end().unwrap();
    let r = p.rust.end().unwrap();
    p.output(n, r);
    p.compare(SNAPSHOT);
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
    let s=SOURCE.replace("Q:x(Q:GetX()+d)","Q:x(Q:GetX()+d);MESSAGEMAN:Broadcast('Nested',{tick=tick,list={1,2},flag=true});SCREENMAN:SetNewScreen('ScreenGameplay');SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_GoToNextScreen')");
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
