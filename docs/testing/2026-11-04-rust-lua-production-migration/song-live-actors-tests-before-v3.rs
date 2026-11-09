use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveScene,
    rust_engine::{RustSongRuntime, SongLayerPlan, SongLiveFrames, test_support},
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
    rust: RustSongRuntime,
    frames: SongLiveFrames,
    screen: song::rust_engine::ActorScene,
    _directory: tempfile::TempDir,
}
impl Pair {
    fn new(configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        Self::with_source("return Def.ActorFrame{}", configure)
    }
    fn with_source(source: &str, configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("default.lua");
        std::fs::write(&path, source).unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "live actor update order");
        context.live = true;
        context.players[1].enabled = true;
        context.music_length_seconds = 2.;
        configure(&mut context);
        let mut outputs = song::compile_song_lua_layers_with_default_host(
            &[&path],
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
        rust.begin_song_compilation().unwrap();
        let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
        rust.initialize_song_layers(screen, &plan).unwrap().unwrap();
        rust.activate_song_layers(screen, &selected)
            .unwrap()
            .unwrap();
        let frames = rust.new_song_live_frames(&selected).unwrap();
        rust.begin_actor_playback();
        let mut pair = Self {
            native,
            rust,
            frames,
            screen,
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
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                v => panic!("scalar expected: {v:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "{source}");
    }
    fn run(&mut self, source: &str) {
        self.compare(source)
    }
    fn prepare(&mut self, frame: &SongLuaLiveFrame, deltas: Option<[f32; 3]>) {
        if let Some(deltas) = deltas {
            self.native.set_option_clock_deltas(deltas);
        }
        self.native.prepare_gameplay_frame(frame.clone()).unwrap();
        self.frames
            .prepare_gameplay_frame(&mut self.rust, frame, deltas, false)
            .unwrap();
        self.compare(SNAPSHOT);
    }
    fn update(&mut self, frame: &SongLuaLiveFrame, deltas: Option<[f32; 3]>) {
        if let Some(deltas) = deltas {
            self.native.set_option_clock_deltas(deltas);
        }
        self.native.update(frame.clone()).unwrap();
        self.frames
            .update(&mut self.rust, frame, deltas, false)
            .unwrap();
        assert!(
            self.rust
                .advance_song_live_actors(self.screen, frame.delta_seconds, None)
                .unwrap()
        );
        self.frames.finish_health_update(&mut self.rust).unwrap();
        self.compare(SNAPSHOT);
    }
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.frames.release(&mut self.rust).unwrap();
        self.rust.end_actor_scene(self.screen).unwrap();
        self.rust.release_actor_scene(self.screen).unwrap();
        self.native.end().unwrap();
    }
}

// This compares the actual player animation queues/getters and authored update
// observations, not a duplicated implementation of the native feedback logic.
const SNAPSHOT: &str = r#"
local out={}; local n=0
local function add(v)n=n+1;out[n]=v==nil and 'nil' or v end
for i=1,2 do
 local p=SCREENMAN:GetTopScreen():GetChild('PlayerP'..i)
 local j=p:GetChild('Judgment'):GetChild('JudgmentWithOffsets')
 add(p.__songlua_last_judgment_seconds)
 for _,a in ipairs{j,p:GetChild('NoteField')}do
  for _,k in ipairs{'GetX','GetY','GetZoomX','GetZoomY','GetZoomZ','GetRotationZ','GetVisible','GetTweenTimeLeft'}do add(a[k](a))end
 end
 add(j:GetState())
 for _,h in ipairs(p.__songlua_hold_judgments)do
  local s=h:GetChild('HoldJudgment'); add(s.__songlua_last_hold_seconds)
  add(s:GetState());add(s:GetZoomX());add(s:GetDiffuseAlpha());add(s:GetTweenTimeLeft())
 end
end
add(table.concat(EVENTS,';'))
return unpack(out,1,n)
"#;
const ACTORS: &str = r#"
p=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
q=SCREENMAN:GetTopScreen():GetChild('PlayerP2')
j=p:GetChild('Judgment'):GetChild('JudgmentWithOffsets')
nf=p:GetChild('NoteField')
function note(label,delta)EVENTS[#EVENTS+1]=label..':'..string.format('%0.9g',delta)end
"#;
fn frame(seconds: f64) -> SongLuaLiveFrame {
    let mut frame = SongLuaLiveFrame {
        music_seconds: seconds,
        screen_seconds: seconds,
        delta_seconds: 0.1,
        ..Default::default()
    };
    frame.players[0].life = 0.5;
    frame.players[1].life = 0.5;
    frame
}
fn judgment(
    seconds: f32,
    grade: deadsync_rules::judgment::JudgeGrade,
    window: Option<deadsync_rules::judgment::TimingWindow>,
    offset: f32,
) -> deadsync_gameplay::JudgmentRenderInfo {
    deadsync_gameplay::JudgmentRenderInfo {
        started_at_screen_s: seconds,
        judgment: deadsync_rules::judgment::Judgment {
            grade,
            window,
            time_error_ms: offset,
            time_error_music_ns: (offset as f64 * 1_000_000.) as i64,
            miss_because_held: false,
        },
    }
}

#[test]
fn live_screen_player_children_columns_song_and_final_screen_order() {
    let mut p = Pair::with_source(
        r#"ROOT=Def.ActorFrame{Name='Song',Def.ActorFrame{Name='Child'}}; return ROOT"#,
        |_| {},
    );
    p.run(ACTORS);
    p.run(
        r#"
    local top=SCREENMAN:GetTopScreen()
    for _,name in ipairs{'Underlay','Overlay','Debug','In','Out','Cancel'}do
      local a=top:GetChild(name);if a then a:SetUpdateFunction(function(_,d)note(name,d)end)end
    end
    p:SetUpdateFunction(function(_,d)note('Player',d)end)
    p:GetChild('Combo'):SetUpdateFunction(function(_,d)note('Combo',d)end)
    nf:SetUpdateFunction(function(_,d)note('Field',d)end)
    nf:GetColumnActors()[1]:SetUpdateFunction(function(_,d)note('Column',d)end)
    ROOT:GetChild('Child'):SetUpdateFunction(function(_,d)note('Child',d)end)
    ROOT:SetUpdateFunction(function(_,d)note('Song',d)end)
    top:SetUpdateFunction(function(_,d)note('Screen',d)end)
    "#,
    );
    p.update(&frame(0.1), None);
    p.run("assert(table.concat(EVENTS,';'):match('Combo:.-Field:.-Column:.-Player:.-Child:.-Song:.-Screen:'))");
}

#[test]
fn wrapper_rates_hibernation_and_column_input_delta_match() {
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    p.run(
        r#"
    p:SetUpdateRate(2);p:hibernate(0.15)
    p:AddWrapperState():SetUpdateRate(3):SetUpdateFunction(function(_,d)note('wrapper',d)end)
    p:SetUpdateFunction(function(_,d)note('player',d)end)
    nf:SetUpdateRate(0.5):SetUpdateFunction(function(_,d)note('field',d)end)
    nf:GetColumnActors()[1]:SetUpdateFunction(function(_,d)note('column',d)end)
    "#,
    );
    for t in [0.1, 0.2, 0.3, 0.4] {
        p.update(&frame(t), None);
    }
}

#[test]
fn judgment_frames_fa_plus_early_late_autoplay_and_tilt_match() {
    use deadsync_rules::judgment::{JudgeGrade as G, TimingWindow as W};
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    for states in [6, 7, 12, 14] {
        p.run(&format!("j.GetNumStates=function()return {states} end; SL.P1.ActiveModifiers.ShowFaPlusWindow='yes';SL.P1.ActiveModifiers.JudgmentTilt=0;SL.P1.ActiveModifiers.TiltMultiplier=1.25"));
        for (i, (grade, window, offset, autoplay)) in [
            (G::Fantastic, Some(W::W0), -1., false),
            (G::Fantastic, Some(W::W1), 12.5, false),
            (G::Fantastic, None, -60., true),
            (G::Great, Some(W::W3), -42., false),
            (G::Miss, None, 110., false),
        ]
        .into_iter()
        .enumerate()
        {
            let seconds = states as f64 + i as f64 / 10.;
            let mut f = frame(seconds);
            f.players[0].last_judgment = Some(judgment(seconds as f32, grade, window, offset));
            f.players[0].autoplay = autoplay;
            p.update(&f, None);
        }
    }
}

#[test]
fn held_let_go_missed_hold_and_duplicate_timestamp_queues_match() {
    use deadsync_rules::note::HoldResult as H;
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    for (i, result) in [H::Held, H::LetGo, H::Missed, H::Held]
        .into_iter()
        .enumerate()
    {
        let mut f = frame(i as f64 / 10.);
        for (column, render) in f.players[0].hold_judgments.iter_mut().enumerate().take(4) {
            *render = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
                result,
                started_at_screen_s: i as f32 + column as f32 / 10.,
            });
        }
        p.update(&f, None);
        p.update(&f, None);
    }
}

#[test]
fn mini_zoom_uses_owned_current_options_after_player_update_before_song() {
    let mut p = Pair::with_source("ROOT=Def.ActorFrame{};return ROOT", |_| {});
    p.run(ACTORS);
    p.run(r#"
    local current=GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current')
    p:SetUpdateFunction(function()current.__songlua_player_option_state.mini=1.25;nf:zoomz(3)end)
    ROOT:SetUpdateFunction(function()assert(nf:GetZoomX()==0.375 and nf:GetZoomY()==0.375);note('mini',nf:GetZoomZ())end)
    GAMESTATE.GetPlayerState=function()error('feedback must use owned options')end
    "#);
    // Health uses owned options too; no public lookup is needed by this frame.
    p.update(&frame(0.1), None);
    p.run("p.__songlua_oitg_zoom_mode='truthy'");
    p.update(&frame(0.2), None);
}

#[test]
fn feedback_runs_for_hibernating_and_inactive_players() {
    use deadsync_rules::judgment::JudgeGrade;
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    p.run("p:hibernate(10);p.__songlua_live_inactive=true;GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.mini=1");
    let mut f = frame(0.1);
    f.players[0].last_judgment = Some(judgment(0.1, JudgeGrade::Excellent, None, 20.));
    p.update(&f, None);
    p.run("assert(nf:GetZoomX()==0.5 and p.__songlua_last_judgment_seconds~=nil)");
}

#[test]
fn hold_callbacks_can_change_judgment_and_zoom_inputs_before_their_phases() {
    use deadsync_rules::{judgment::JudgeGrade, note::HoldResult};
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    p.run(r#"
    local h=p.__songlua_hold_judgments[1]:GetChild('HoldJudgment')
    local stop=h.stoptweening;h.stoptweening=function(self)
      SL.P1.ActiveModifiers.JudgmentTilt=true
      SL.P1.ActiveModifiers.TiltMultiplier=2
      GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.mini=0.5
      collectgarbage('collect');return stop(self)
    end
    "#);
    let mut f = frame(0.1);
    f.players[0].last_judgment = Some(judgment(0.1, JudgeGrade::Great, None, 30.));
    f.players[0].hold_judgments[0] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
        result: HoldResult::Held,
        started_at_screen_s: 0.1,
    });
    p.update(&f, None);
}

#[test]
fn feedback_methods_modifiers_and_color_remain_rooted_across_replacement_gc() {
    use deadsync_rules::{judgment::JudgeGrade, note::HoldResult};
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    p.run(r#"
    local saved=SL.P1.ActiveModifiers;saved.JudgmentTilt=true;saved.TiltMultiplier=2
    local visible=j.visible;j.visible=function(self,value)SL.P1.ActiveModifiers={};collectgarbage('collect');return visible(self,value)end
    local h=p.__songlua_hold_judgments[1]:GetChild('HoldJudgment');local diffuse=h.diffuse
    h.diffuse=function(self,c)collectgarbage('collect');return diffuse(self,c)end
    "#);
    let mut f = frame(0.1);
    f.players[0].last_judgment = Some(judgment(0.1, JudgeGrade::Great, None, -30.));
    f.players[0].hold_judgments[0] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
        result: HoldResult::Held,
        started_at_screen_s: 0.1,
    });
    p.update(&f, None);
}

#[test]
fn queued_commands_and_runtime_wrappers_invalidate_next_plan_without_losing_current_jobs() {
    let mut p = Pair::with_source(
        "ROOT=Def.ActorFrame{Name='Song',Def.ActorFrame{Name='Child'}};return ROOT",
        |_| {},
    );
    p.run(ACTORS);
    p.run(r#"
    p:SetUpdateFunction(function(self,d)
      if not self.added then self.added=true;ROOT:AddWrapperState():SetUpdateFunction(function(_,d)note('newwrapper',d)end);collectgarbage('collect')end
      note('player',d)
    end)
    ROOT:SetUpdateFunction(function(_,d)note('song',d)end)
    ROOT:GetChild('Child'):SetUpdateFunction(function(_,d)note('child',d)end)
    ROOT:addcommand('Later',function(self)note('queue',1);self:linear(0.2):x(40)end)
    ROOT:sleep(0.05):queuecommand('Later')
    "#);
    for t in [0.1, 0.2, 0.3, 0.4] {
        p.update(&frame(t), None);
    }
}

#[test]
fn ordinary_actor_callback_errors_report_once_without_stopping_song_callbacks() {
    let mut p = Pair::with_source("ROOT=Def.ActorFrame{};return ROOT", |_| {});
    p.run(ACTORS);
    p.run("p:SetUpdateFunction(function()error('player-update-sentinel')end);ROOT:SetUpdateFunction(function(_,d)note('after-player-error',d)end)");
    p.update(&frame(0.1), None);
    p.update(&frame(0.2), None);
    assert_eq!(
        p.rust
            .take_actor_diagnostics()
            .iter()
            .filter(|s| s.contains("player-update-sentinel"))
            .count(),
        1
    );
    p.run("assert(#EVENTS==2)");
}

#[test]
fn entry_hook_precedes_inactivity_delta_selection_and_observes_player_feedback() {
    let mut p = Pair::with_source("ROOT=Def.ActorFrame{};return ROOT", |_| {});
    p.run(ACTORS);
    p.run("ROOT.__songlua_live_inactive=true;ROOT:SetUpdateFunction(function(_,d)note('entry-delta',d)end);GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.mini=1");
    let f = frame(0.1);
    p.frames.update(&mut p.rust, &f, None, false).unwrap();
    let hook=p.rust.vm_mut().execute(&compile_named(br#"return function(a)if a==ROOT then assert(nf:GetZoomX()==0.5);a.__songlua_live_inactive=false;a.__songlua_live_delta=0.25 end end"#,b"=entry hook").unwrap()).unwrap()[0];
    assert!(
        p.rust
            .advance_song_live_actors(p.screen, f.delta_seconds, Some(hook))
            .unwrap()
    );
    p.frames.finish_health_update(&mut p.rust).unwrap();
    test_support::legacy_live_scene_lua(&p.native)
        .load("ROOT.__songlua_live_inactive=false;ROOT.__songlua_live_delta=0.25")
        .exec()
        .unwrap();
    p.native.update(f).unwrap();
    p.compare(SNAPSHOT);
    p.run("assert(EVENTS[1]=='entry-delta:0.25')");
}

#[test]
fn failed_entry_callback_leaves_the_cached_plan_usable_on_the_next_frame() {
    let mut p = Pair::new(|_| {});
    p.run(ACTORS);
    let hook = p
        .rust
        .vm_mut()
        .execute(
            &compile_named(
                br#"return function(a)if a==p then error('entry-sentinel')end end"#,
                b"=bad entry hook",
            )
            .unwrap(),
        )
        .unwrap()[0];
    let error = p
        .rust
        .advance_song_live_actors(p.screen, 0.1, Some(hook))
        .unwrap_err();
    assert!(error.to_string().contains("entry-sentinel"));
    p.update(&frame(0.1), None);
}

#[test]
fn foreign_retired_nonlive_and_unactivated_owners_are_rejected() {
    let mut p = Pair::new(|_| {});
    let c = SongLuaCompileContext::new(Path::new("."), "foreign runtime");
    let mut other = RustSongRuntime::new(&c).unwrap();
    assert!(!other.advance_song_live_actors(p.screen, 0.1, None).unwrap());
    p.rust.set_live(false);
    assert!(
        p.rust
            .advance_song_live_actors(p.screen, 0.1, None)
            .is_err()
    );
    p.rust.set_live(true);
    let declaration = p
        .rust
        .vm_mut()
        .execute(&compile_named(b"return Def.Actor{}", b"=unactivated").unwrap())
        .unwrap()[0];
    let scene = p.rust.construct_actor(declaration).unwrap();
    assert!(p.rust.advance_song_live_actors(scene, 0.1, None).is_err());
    p.rust.release_actor_scene(scene).unwrap();
    assert!(p.rust.retire_actor_scene(p.screen).unwrap());
    assert!(
        !p.rust
            .advance_song_live_actors(p.screen, 0.1, None)
            .unwrap()
    );
}
