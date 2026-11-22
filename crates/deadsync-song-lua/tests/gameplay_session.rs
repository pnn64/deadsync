use deadlib_lua::compile_named;
use deadlib_present::actors::TextAttribute;
use deadsync_core::input::InputSource;
use deadsync_gameplay as game;
use deadsync_input::{InputEvent, PhysicalInput, VirtualAction};
use deadsync_profile_gameplay::GameplayProfile;
use deadsync_song_lua::{self as song, gameplay_session as session};
use session::{SongLuaGameplayHost, SongLuaGameplaySession};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "support/paths.rs"]
mod paths;

type Overlay = song::SongLuaOverlayActor<song::SongLuaOverlayKind<(), (), TextAttribute>>;
type Delta = song::gameplay::SongLuaRuntimeOverlayStateDelta;
type Game = game::GameplayRuntimeState<GameplayProfile, Overlay, song::SongLuaCapturedActor, Delta>;

// No theme or renderer is needed to own a real gameplay state. The compiler's
// live actor slots are installed below, after the chart establishes its timing.
struct EmptyVisuals;

impl game::SongLuaRuntimeBuilder<Overlay, song::SongLuaCapturedActor, Delta> for EmptyVisuals {
    fn build_song_lua_runtime(
        self,
        p: game::SongLuaRuntimeWindowBuild<'_>,
    ) -> game::SongLuaRuntimeBuildOutput<Overlay, song::SongLuaCapturedActor, Delta> {
        (
            Default::default(),
            Default::default(),
            game::SongLuaRuntimeVisuals {
                overlays: vec![],
                screen_overlay_index: None,
                overlay_eases: vec![],
                overlay_ease_ranges: vec![],
                overlay_events: vec![],
                background_visual_layers: vec![],
                foreground_visual_layers: vec![],
                player_actors: Default::default(),
                player_events: Default::default(),
                player_judgment_events: Default::default(),
                player_combo_events: Default::default(),
                song_foreground: Default::default(),
                song_foreground_events: vec![],
                hidden_players: [false; 2],
                screen_layers: Default::default(),
                screen_layer_events: Default::default(),
                note_hides: Default::default(),
                column_offsets: Default::default(),
                column_splines: Default::default(),
                screen_width: p.screen_width,
                screen_height: p.screen_height,
            },
        )
    }
}

fn game_for_style(dir: &Path, style: game::GameplayInputPlayStyle, notes: &str) -> Game {
    paths::init_paths();
    let path = dir.join("session.ssc");
    std::fs::write(&path, format!(
        "#TITLE:Session;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n{notes}\n;\n"
    )).unwrap();
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.).unwrap());
    let chart = Arc::new(song.charts[0].clone());
    let data = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.)
            .unwrap()
            .remove(0),
    );
    game::init_gameplay_runtime(
        song,
        [chart.clone(), chart],
        [data.clone(), data],
        game::GameplayViewport::new(640., 480.),
        game::GameplaySession {
            play_style: style,
            ..Default::default()
        },
        game::GameplayConfig::default(),
        deadsync_chart::SyncPref::Default,
        Default::default(),
        Default::default(),
        EmptyVisuals,
        |_, _, _, _, _| vec![],
        5,
        1.,
        [deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.); 2],
        std::array::from_fn(|_| GameplayProfile::from(deadsync_profile::Profile::default())),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        [game::CourseLifeConfig::Bar; 2],
        false,
        [0; 2],
    )
}

#[derive(Debug, PartialEq, Eq)]
enum Effect {
    None,
    Navigate(game::GameplayExit),
    Pop(game::GameplayExit),
    Reload,
}

struct Host {
    game: Game,
    session: SongLuaGameplaySession,
    visual_updates: usize,
    _dir: tempfile::TempDir,
}

impl SongLuaGameplayHost for Host {
    type Profile = GameplayProfile;
    type OverlayActor = Overlay;
    type CapturedActor = song::SongLuaCapturedActor;
    type StateDelta = Delta;
    type Effect = Effect;

    fn gameplay_parts(&mut self) -> (&mut Game, &mut SongLuaGameplaySession) {
        (&mut self.game, &mut self.session)
    }

    fn update_gameplay_visuals(&mut self, _: f32) {
        self.visual_updates += 1;
    }

    fn gameplay_effect(action: game::GameplayAction) -> Effect {
        match action {
            game::GameplayAction::None => Effect::None,
            game::GameplayAction::Navigate(exit) => Effect::Navigate(exit),
            game::GameplayAction::NavigateNoFade(exit) => Effect::Pop(exit),
            game::GameplayAction::Restart => Effect::Reload,
        }
    }

    fn reload_song_lua_effect() -> Effect {
        Effect::Reload
    }

    fn next_screen_effect(name: Option<&str>) -> Result<Effect, String> {
        match name {
            None | Some("" | "ScreenEvaluationStage") => {
                Ok(Effect::Pop(game::GameplayExit::Complete))
            }
            Some("ScreenSelectMusic") => Ok(Effect::Pop(game::GameplayExit::Cancel)),
            Some(name) => Err(format!("unsupported gameplay destination: {name}")),
        }
    }
}

const LATE_NOTES: &str = "0000\n0000\n0000\n0000\n,\n1000\n0000\n0000\n1000";
const HOST_ORIGIN: i64 = 10_000_000_000;

impl Host {
    fn new(script: &str) -> Self {
        Self::with_notes(script, LATE_NOTES)
    }

    fn with_notes(script: &str, notes: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut game = game_for_style(dir.path(), game::GameplayInputPlayStyle::Single, notes);
        game.set_live_autoplay_enabled(false);
        let mut session = SongLuaGameplaySession::for_gameplay(&mut game);
        let path = dir.path().join("session.lua");
        std::fs::write(&path, script).unwrap();
        let mut context = song::SongLuaCompileContext::new(dir.path(), "session callbacks");
        context.live = true;
        context.defer_initial_live_update = true;
        context.players[1].enabled = true;
        context.song_timing = Some((*game.timing_runtime.timing).clone());
        let mut compiled = song::compile_song_lua_layers(
            &[&path],
            0,
            &context,
            Default::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_: &()| -> Option<song::SongLuaOverlayModelLayer<()>> { None },
            |_, _| None,
        )
        .unwrap();
        assert_eq!(compiled.len(), 1);
        let mut compiled = compiled.remove(0);
        let mut scene = compiled.live_scene.take().unwrap();
        scene
            .bind_targets(
                compiled
                    .live_indices
                    .iter()
                    .enumerate()
                    .map(|(local, &global)| (global, song::SongLuaLiveTarget::Root(local)))
                    .collect(),
            )
            .unwrap();
        let visuals = &mut game.mods.song_lua_visuals;
        visuals.overlays = compiled.overlays;
        visuals.screen_overlay_index = compiled.screen_overlay_index;
        visuals.player_actors = compiled.player_actors;
        visuals.song_foreground = compiled.song_foreground;
        visuals.screen_layers = compiled.screen_layers;
        visuals.hidden_players = compiled.hidden_players;
        session.attach(&mut game, scene, || {}).unwrap();
        let mut host = Self {
            game,
            session,
            visual_updates: 0,
            _dir: dir,
        };
        let output = host.take_output();
        assert!(
            output.errors.is_empty(),
            "startup errors: {:?}",
            output.errors
        );
        host
    }

    fn lua(&mut self, source: &str) {
        let mut runtime = self.session.scene_mut().unwrap().runtime_mut();
        runtime
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/session-assert.lua").unwrap())
            .unwrap();
    }

    fn take_output(&mut self) -> song::SongLuaLiveOutput {
        self.session.take_output(&mut self.game)
    }

    fn frame(&mut self, delta: f32, music_nanos: i64) -> (Vec<Effect>, song::SongLuaLiveOutput) {
        self.frame_with(delta, music_nanos, |_| {})
    }

    fn frame_with(
        &mut self,
        delta: f32,
        music_nanos: i64,
        after_input: impl FnOnce(&mut Self),
    ) -> (Vec<Effect>, song::SongLuaLiveOutput) {
        let host_nanos = (HOST_ORIGIN + music_nanos) as u64;
        let snapshot = game::GameplayAudioSnapshot {
            stream_clock: game::GameplayStreamClockSnapshot {
                music_nanos,
                has_music_mapping: true,
                valid_at_host_nanos: host_nanos,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut effects = Vec::new();
        session::update(
            self,
            delta,
            snapshot,
            || host_nanos,
            &mut effects,
            after_input,
        );
        (effects, self.take_output())
    }
}

fn event(action: VirtualAction, input_slot: u32, pressed: bool, music_nanos: i64) -> InputEvent {
    let now = Instant::now();
    InputEvent::new(
        action,
        input_slot,
        pressed,
        InputSource::Keyboard,
        now,
        (HOST_ORIGIN + music_nanos) as u64,
        now,
        now,
    )
}

fn no_errors(output: &song::SongLuaLiveOutput) {
    assert!(output.errors.is_empty(), "Lua errors: {:?}", output.errors);
}

#[test]
fn queued_physical_input_and_repeats_follow_update_and_never_score_twice() {
    let mut host = Host::with_notes(
        r#"
Trace = {}
function mark(value) Trace[#Trace + 1] = value end
return Def.ActorFrame {
 OnCommand=function(self)
  SCREENMAN:GetTopScreen():GetChild('In'):finishtweening()
  self:sleep(30)
  self:SetUpdateFunction(function() mark('update') end)
  self:SetDrawFunction(function() mark('draw') end)
  SCREENMAN:GetTopScreen():AddInputCallback(function(e)
   if e.type == 'InputEventType_FirstPress' then mark('press')
   elseif e.type == 'InputEventType_Repeat' then mark('repeat')
   elseif e.type == 'InputEventType_Release' then mark('release') end
  end)
 end,
}
"#,
        "1000\n1000\n1000\n0001",
    );
    no_errors(&host.frame(0.0, -250_000_000).1);
    host.lua("Trace = {}");
    let first = host.game.chart_runtime.note_time_cache_ns[0];
    let second = host.game.chart_runtime.note_time_cache_ns[1];
    let physical = Some(PhysicalInput::Key(winit::keyboard::KeyCode::ArrowLeft));
    let mut effects = Vec::new();
    session::handle_input(
        &mut host,
        &event(VirtualAction::p1_left, 1, true, first),
        physical,
        &mut effects,
    );
    assert!(effects.is_empty());
    host.lua("assert(#Trace == 0, 'physical callbacks wait for Update')");
    let (effects, output) = host.frame_with(0.4, first, |host| host.lua("mark('after_input')"));
    assert!(effects.is_empty());
    no_errors(&output);
    host.lua("assert(table.concat(Trace, ',') == 'update,press,repeat,after_input,draw', table.concat(Trace, ','))");
    assert!(host.game.chart_runtime.notes[0].result.is_some());
    assert!(host.game.chart_runtime.notes[1].result.is_none());
    assert_eq!(host.game.recorded_replay_edges().len(), 1);

    host.lua("Trace = {}");
    no_errors(&host.frame(0.5, second).1);
    host.lua("assert(table.concat(Trace, ',') == 'update,repeat,draw', table.concat(Trace, ','))");
    assert!(
        host.game.chart_runtime.notes[1].result.is_none(),
        "the repeat must not judge the next tap"
    );
    assert_eq!(
        host.game.recorded_replay_edges().len(),
        1,
        "repeat callbacks must not become physical edges"
    );
    assert_eq!(
        host.game.players()[0].judgment_counts.iter().sum::<u32>(),
        1
    );
}

#[test]
fn input_and_draw_reload_requests_survive_output_drains_until_next_update() {
    for from_input in [true, false] {
        let script = r#"
Armed = false
Draws = 0
local function reload()
 if Armed then Armed = false; SCREENMAN:SetNewScreen('ScreenGameplay') end
end
return Def.ActorFrame {
 OnCommand=function(self)
  SCREENMAN:GetTopScreen():GetChild('In'):finishtweening()
  self:sleep(30)
  self:SetDrawFunction(function() Draws = Draws + 1; if not FROM_INPUT then reload() end end)
  SCREENMAN:GetTopScreen():AddInputCallback(function(e)
   if FROM_INPUT and e.type == 'InputEventType_FirstPress' then reload() end
  end)
 end,
}
"#
        .replace("FROM_INPUT", if from_input { "true" } else { "false" });
        let mut host = Host::new(&script);
        no_errors(&host.frame(0.0, 0).1);
        host.lua("Armed = true; Draws = 0");
        if from_input {
            session::handle_input(
                &mut host,
                &event(VirtualAction::p1_menu_left, 1, true, 1_000_000),
                None,
                &mut Vec::new(),
            );
        }
        let (effects, output) = host.frame(0.001, 1_000_000);
        no_errors(&output);
        assert!(
            effects.is_empty(),
            "requests made after Update wait for the next frame"
        );
        assert!(
            matches!(output.request, Some(song::SongLuaLiveRequest::NewScreen(ref name)) if name == "ScreenGameplay")
        );
        assert!(
            host.take_output().request.is_none(),
            "the public output has been fully drained"
        );
        host.lua("assert(Draws == 1)");
        let (effects, output) = host.frame(0.001, 2_000_000);
        no_errors(&output);
        assert_eq!(effects, [Effect::Reload]);
        host.lua("assert(Draws == 1, 'reload frames must not Draw')");
        assert!(
            !host.session.is_ended(),
            "reload retains the live VM for reconstruction"
        );
    }
}

#[test]
fn immediate_pop_ends_once_applies_final_writes_and_skips_draw() {
    let mut host = Host::new(
        r#"
Ends, Draws = 0, 0
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
return Def.ActorFrame {
 OnCommand=function(self)
  self:SetDrawFunction(function() Draws = Draws + 1 end)
 end,
 EndCommand=function()
  Ends = Ends + 1
  assert(SCREENMAN:GetTopScreen() == nil)
  player:SetLife(0.375)
 end,
}
"#,
    );
    host.game
        .begin_exit_transition(game::ExitTransitionKind::Cancel);
    host.game
        .control
        .exit_input
        .exit_transition
        .as_mut()
        .unwrap()
        .started_at = Instant::now() - Duration::from_secs(10);
    let mut effects = Vec::new();
    session::update_with_effects(
        &mut host,
        0.01,
        game::GameplayAudioSnapshot::default(),
        || HOST_ORIGIN as u64,
        |_| -> Result<(), String> { panic!("core cancellation must use the game action") },
        |effect| effects.push(effect),
        |_| {},
    );
    no_errors(&host.take_output());
    assert_eq!(
        effects,
        [session::SongLuaGameplayEffect::Action(game::GameplayAction::NavigateNoFade(
            game::GameplayExit::Cancel,
        ))]
    );
    assert!(host.session.is_ended());
    assert_eq!(host.game.players()[0].life, 0.375);
    host.lua("assert(Ends == 1 and Draws == 0)");
    let (effects, output) = host.frame(0.01, 10_000_000);
    no_errors(&output);
    assert!(effects.is_empty());
    host.session.end(&mut host.game).unwrap();
    host.lua("assert(Ends == 1 and Draws == 0, 'End is idempotent')");
    assert!(host.game.note_field_tap_callback.is_none());
    assert!(host.game.tap_row_judgment_callback.is_none());
    assert!(host.game.hold_judgment_callback.is_none());
    assert!(host.game.avoided_mine_judgment_callback.is_none());
    assert!(host.game.life_changed_callback.is_none());
}

#[test]
fn unsupported_destination_is_resolved_before_end_and_can_be_corrected() {
    for (destination, expected) in [
        (
            "ScreenEvaluationStage",
            Effect::Pop(game::GameplayExit::Complete),
        ),
        ("ScreenGameplay", Effect::Reload),
    ] {
        let mut host = Host::new(
            r#"
Ends, Draws = 0, 0
local started = false
return Def.ActorFrame {
 OnCommand=function(self)
  local screen = SCREENMAN:GetTopScreen()
  self:sleep(30)
  self:SetDrawFunction(function() Draws = Draws + 1 end)
  screen:SetNextScreenName('ScreenUnknown')
  self:SetUpdateFunction(function()
   if not started then
    started = true
    screen:StartTransitioningScreen('SM_DoNextScreen')
   end
  end)
 end,
 EndCommand=function() Ends = Ends + 1 end,
}
"#,
        );
        let mut rejected = false;
        for frame in 0..20 {
            let (effects, output) = host.frame(0.125, frame * 125_000_000);
            assert!(effects.is_empty());
            if !output.errors.is_empty() {
                assert_eq!(
                    output.errors,
                    ["unsupported gameplay destination: ScreenUnknown"]
                );
                rejected = true;
                break;
            }
        }
        assert!(
            rejected,
            "the completed transition must validate its destination"
        );
        assert!(!host.session.is_ended());
        host.lua(&format!(
            "assert(Ends == 0); DrawsBefore = Draws; SCREENMAN:GetTopScreen():SetNextScreenName('{destination}')"
        ));
        let (effects, output) = host.frame(0.125, 2_500_000_000);
        no_errors(&output);
        assert_eq!(effects, [expected]);
        assert!(host.session.is_ended());
        host.lua("assert(Ends == 1 and Draws == DrawsBefore)");
    }
}

#[test]
fn actor_life_rescue_precedes_failure_commit() {
    for rescue in [false, true] {
        let script = r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
Updates, Failed = 0, 0
return Def.ActorFrame {
 OnCommand=function(self)
  SCREENMAN:GetTopScreen():GetChild('In'):finishtweening()
  self:sleep(30)
  self:SetUpdateFunction(function()
   Updates = Updates + 1
   if Updates == 2 then
    player:SetLife(0)
    if RESCUE then player:SetLife(0.25) end
   end
  end)
 end,
 PlayerFailedMessageCommand=function() Failed = Failed + 1 end,
}
"#
        .replace("RESCUE", if rescue { "true" } else { "false" });
        let mut host = Host::new(&script);
        for frame in 0..3 {
            host.session
                .scene_mut()
                .unwrap()
                .set_gameplay_clock_input((frame > 0).then_some(0.25));
            no_errors(&host.frame(0.002, 1_000_000_000 + frame * 2_000_000).1);
        }
        let player = &host.game.players()[0];
        assert_eq!(player.stage_failed, !rescue);
        assert_eq!(player.life, if rescue { 0.25 } else { 0.0 });
        assert_eq!(player.alive_seconds, if rescue { 0.5 } else { 0.0 });
        host.lua(if rescue {
            "assert(Failed == 0)"
        } else {
            "assert(Failed == 1)"
        });
    }
}

#[test]
fn natural_outro_preserves_final_mine_delivery_and_ends_before_the_handoff() {
    let mut host = Host::with_notes(
        r#"
Offs, Ends, Mines, Draws = 0, 0, 0, 0
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
return Def.ActorFrame {
 OnCommand=function(self)
  SCREENMAN:GetTopScreen():GetChild('In'):finishtweening()
  self:sleep(9999)
  self:SetDrawFunction(function() Draws = Draws + 1 end)
 end,
 JudgmentMessageCommand=function(_, params)
  if params.TapNoteScore == 'TapNoteScore_AvoidMine' then
   assert(Ends == 0, 'final mine must be delivered while its owner is alive')
   Mines = Mines + 1
  end
 end,
 OffCommand=function()
  Offs = Offs + 1
  assert(SCREENMAN:GetTopScreen() ~= nil)
 end,
 EndCommand=function()
  Ends = Ends + 1
  assert(SCREENMAN:GetTopScreen() == nil)
  player:SetLife(0.375)
 end,
}
"#,
        "1000\n0000\n0000\nM000",
    );
    no_errors(&host.frame(0.0, -250_000_000).1);
    let end = host.game.music_end_time_ns();
    let mut saw_outro = false;
    let mut handed_off = false;
    for frame in -1..=80 {
        host.lua("DrawsBefore = Draws");
        let (effects, output) = host.frame(0.025, end + frame * 25_000_000);
        no_errors(&output);
        saw_outro |= host.session.out_transition().is_some();
        if !effects.is_empty() {
            assert_eq!(effects, [Effect::Pop(game::GameplayExit::Complete)]);
            assert!(
                saw_outro,
                "core completion must allow the owned Lua outro to run"
            );
            handed_off = true;
            break;
        }
        assert!(!host.session.is_ended());
    }
    assert!(
        handed_off,
        "natural completion must finish its owned transition"
    );
    assert!(host.game.progress.stage.song_completed_naturally);
    assert!(host.session.is_ended());
    assert_eq!(host.game.players()[0].mines_avoided, 1);
    assert_eq!(host.game.players()[0].life, 0.375);
    host.lua("assert(Offs == 1 and Ends == 1 and Mines == 1); assert(Draws == DrawsBefore, 'the handoff frame must not Draw')");
    let (effects, output) = host.frame(0.025, end + 3_000_000_000);
    no_errors(&output);
    assert!(effects.is_empty());
    host.lua("assert(Offs == 1 and Ends == 1 and Mines == 1 and Draws == DrawsBefore)");
}

#[test]
fn callback_routing_follows_current_style_and_active_side_after_reload() {
    let mut host = Host::new(
        r#"
Routes = {}
return Def.ActorFrame {
 OnCommand=function()
  SCREENMAN:GetTopScreen():AddInputCallback(function(e)
   if e.type == 'InputEventType_FirstPress' then
    Routes[#Routes + 1] = e.PlayerNumber .. ':' .. e.controller
   end
  end)
 end,
}
"#,
    );
    use game::{
        GameplayInputPlayStyle::{Double, Single},
        GameplayInputPlayerSide::{P1, P2},
    };
    for (style, side, expected) in [
        (Single, P1, "PlayerNumber_P1:GameController_1"),
        (
            Double,
            P1,
            "PlayerNumber_P1:GameController_1,PlayerNumber_P1:GameController_2",
        ),
        (Single, P1, "PlayerNumber_P1:GameController_1"),
        (Single, P2, "PlayerNumber_P2:GameController_2"),
        (
            Double,
            P2,
            "PlayerNumber_P2:GameController_1,PlayerNumber_P2:GameController_2",
        ),
        (Single, P2, "PlayerNumber_P2:GameController_2"),
    ] {
        host.game.setup.session.play_style = style;
        host.game.setup.session.player_side = side;
        host.lua("Routes = {}");
        for (slot, action) in [(1, VirtualAction::p1_left), (2, VirtualAction::p2_left)] {
            for pressed in [true, false] {
                host.session
                    .dispatch_input(&mut host.game, &event(action, slot, pressed, 0), None);
            }
        }
        no_errors(&host.take_output());
        host.lua(&format!(
            "assert(table.concat(Routes, ',') == '{expected}', table.concat(Routes, ','))"
        ));
    }
}

#[test]
fn a_frame_without_lua_still_updates_presentation_before_the_input_hook() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Host {
        game: game_for_style(dir.path(), game::GameplayInputPlayStyle::Single, LATE_NOTES),
        session: SongLuaGameplaySession::new(),
        visual_updates: 0,
        _dir: dir,
    };
    let mut input_calls = 0;
    let (effects, output) = host.frame_with(0.01, 0, |host| {
        assert_eq!(host.visual_updates, 1);
        assert!(host.session.is_none());
        input_calls += 1;
    });
    no_errors(&output);
    assert!(effects.is_empty());
    assert_eq!(input_calls, 1);
    assert!(output.draw_frames.is_empty());
}

#[test]
fn song_lua_out_native_clocks() {
    let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/itgmania-song-lua-micro/native-out-clocks.json"
    )))
    .expect("complete native transition inputs");
    for case in native["cases"].as_array().expect("native cases") {
        let frames = case["frames"].as_array().expect("native frames");
        let deltas: Vec<f32> = frames
            .iter()
            .map(|frame| frame["delta"].as_f64().expect("native delta") as f32)
            .collect();
        let index = song::test_support::song_lua_out_completion_frame(&deltas)
            .expect("native transition completes");
        assert_eq!(index, frames.len() - 1, "{} completion frame", case["name"]);
        assert_eq!(
            frames[index]["seconds"], case["end_seconds"],
            "{} completion clock",
            case["name"]
        );
    }
}
