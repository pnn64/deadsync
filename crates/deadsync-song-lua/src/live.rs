use mlua::{Function, Lua, Table, Value};
use std::fmt;

use crate::{SongLuaCompileContext, SongLuaOverlayState, SongLuaTrackedActorTarget, LUA_PLAYERS};

pub(crate) const TAP_SCORES: [&str; 11] = [
    "None", "HitMine", "AvoidMine", "CheckpointMiss", "Miss", "W5",
    "W4", "W3", "W2", "W1", "CheckpointHit",
];

pub(crate) struct LiveMode;

pub(crate) fn active(lua: &Lua) -> bool {
    lua.app_data_ref::<LiveMode>().is_some()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SongLuaLivePlayer {
    pub possible_points: i32,
    pub actual_points: i32,
    pub current_possible_points: i32,
    pub life: f32,
    pub failed: bool,
    pub combo: u32,
    pub miss_combo: u32,
    /// Native TapNoteScore order: None, HitMine, AvoidMine, CheckpointMiss,
    /// Miss, W5, W4, W3, W2, W1, CheckpointHit.
    pub tap_scores: [u32; 11],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SongLuaLiveFrame {
    pub beat: f64,
    pub music_seconds: f64,
    pub screen_seconds: f64,
    pub delta_seconds: f64,
    pub players: [SongLuaLivePlayer; LUA_PLAYERS],
}

impl SongLuaLivePlayer {
    pub fn from_player(player: &deadsync_gameplay::PlayerRuntime, possible_points: i32) -> Self {
        let [w1, w2, w3, w4, w5, miss] = player.judgment_counts;
        Self {
            possible_points, actual_points: player.earned_grade_points,
            current_possible_points: player.current_possible_points,
            life: player.life, failed: player.fail_time.is_some(),
            combo: player.combo, miss_combo: player.miss_combo,
            tap_scores: [0, player.mines_hit, player.mines_avoided, player.checkpoints_missed,
                miss, w5, w4, w3, w2, w1, player.checkpoints_hit],
        }
    }
}

impl SongLuaLiveFrame {
    /// Gameplay stores a sole cabinet P2 in runtime slot zero, as does the
    /// preparation context's Lua P1. No course carry or frozen display counts
    /// are substituted for the current stage's actual progress.
    pub fn from_gameplay<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        game: &deadsync_gameplay::GameplayRuntimeState<P, O, C, D>, delta_seconds: f32,
    ) -> Self {
        Self {
            beat: f64::from(game.clock.song_position.current_beat),
            music_seconds: game.clock.song_position.current_music_time_ns as f64 / 1_000_000_000.0,
            screen_seconds: f64::from(game.boundary.total_elapsed_in_screen),
            delta_seconds: f64::from(delta_seconds),
            players: std::array::from_fn(|player| game.stage_itg_score_inputs(player).map_or_else(
                SongLuaLivePlayer::default, |score| SongLuaLivePlayer::from_player(
                    &game.players_runtime.players[player], score.possible_grade_points))),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SongLuaLiveRequest {
    NewScreen(String),
    Transition(String),
}

pub(crate) struct TransitionRequest(pub String);

#[derive(Clone, Copy)]
pub enum SongLuaLiveTarget {
    Root(usize),
    Background(usize, usize),
    Foreground(usize, usize),
}

#[derive(Debug, Default)]
pub struct SongLuaLiveOutput {
    pub request: Option<SongLuaLiveRequest>,
    pub broadcasts: Vec<(f32, String, bool)>,
    pub sounds: Vec<crate::SongLuaSoundEvent>,
    /// Native-style callback errors retain partial state and are reported once.
    pub errors: Vec<String>,
}

/// Owns one initialized Lua scene on the gameplay/screen thread. Worker
/// preparation transfers ownership once; the VM is never cloned. Actor tables
/// and snapshot buffers live for the screen and are sized during preparation.
/// Tween queues retain only pending steps, not a whole-song sample history.
/// Advancement runs authored callbacks without speculative future probes.
/// Asset loads requested by those callbacks still require runtime integration.
pub struct SongLuaLiveScene {
    pub(crate) lua: Lua,
    root: Value,
    stage: Table,
    players: [Table; LUA_PLAYERS],
    pub(crate) actors: Vec<Table>,
    pub(crate) states: Vec<SongLuaOverlayState>,
    pub(crate) tracked: Vec<(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)>,
    pub(crate) registry: Vec<Table>,
    targets: Vec<(usize, SongLuaLiveTarget)>,
    context: SongLuaCompileContext,
    last_beat: f64,
    startup_output: SongLuaLiveOutput,
}

impl fmt::Debug for SongLuaLiveScene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SongLuaLiveScene")
            .field("actors", &self.actors.len())
            .field("tracked", &self.tracked.len())
            .finish_non_exhaustive()
    }
}

impl SongLuaLiveScene {
    pub(crate) fn new(
        lua: Lua,
        root: Value,
        context: SongLuaCompileContext,
        actors: Vec<Table>,
        tracked: Vec<(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)>,
        mut startup_broadcasts: Vec<(f32, String, bool)>,
    ) -> mlua::Result<Self> {
        let manager = lua.globals().get::<Table>("STATSMAN")?;
        let stage = manager.get::<Function>("GetCurStageStats")?.call::<Table>(manager)?;
        let players = [
            stage.get::<Function>("GetPlayerStageStats")?.call::<Table>((stage.clone(), "PlayerNumber_P1"))?,
            stage.get::<Function>("GetPlayerStageStats")?.call::<Table>((stage.clone(), "PlayerNumber_P2"))?,
        ];
        let registry = crate::song_lua_actor_registry(&lua)?
            .sequence_values::<Table>().collect::<mlua::Result<Vec<_>>>()?;
        crate::lua_util::prepare_live_updates(&lua, &root)?;
        let states = actors.iter().map(|actor| read_state(&lua, actor))
            .collect::<mlua::Result<Vec<_>>>()?;
        let mut startup_output = take_output(&lua)?;
        startup_broadcasts.append(&mut startup_output.broadcasts);
        startup_output.broadcasts = startup_broadcasts;
        let last_beat = f64::from(crate::compile_song_runtime_values(&lua)?.0);
        Ok(Self { lua, root, stage, players, actors, states, tracked, registry,
            targets: Vec::new(), context, last_beat, startup_output })
    }

    pub fn advance(&mut self, frame: SongLuaLiveFrame) -> mlua::Result<SongLuaLiveOutput> {
        // GameState.cpp::UpdateSongPosition maintains global Song timing apart
        // from each Player's selected Steps timing. The domain frame carries
        // the player's beat; Lua's GAMESTATE and music effects use the song.
        let position = self.context.song_timing.as_ref()
            .map(|timing| timing.get_song_position(frame.music_seconds as f32));
        let beat = position.as_ref().map_or(frame.beat, |position| f64::from(position.beat));
        self.feed_stats(&frame)?;
        crate::set_compile_song_runtime_values(&self.lua, beat as f32, frame.music_seconds as f32)?;
        if let Some(position) = position {
            let runtime = self.lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
            runtime.set(crate::SONG_LUA_RUNTIME_BPS_KEY, position.bpm / 60.0)?;
            runtime.set("__songlua_freeze", position.is_in_freeze)?;
            runtime.set("__songlua_delay", position.is_in_delay)?;
        }
        crate::set_compile_song_runtime_delta_values(&self.lua,
            (beat - self.last_beat) as f32, frame.delta_seconds as f32)?;
        self.last_beat = beat;
        crate::lua_util::set_live_frame(&self.lua, frame.screen_seconds, beat, frame.delta_seconds);
        crate::song_tables::advance_option_levels(&self.lua, frame.screen_seconds)?;
        crate::lua_util::run_actor_compile_update_functions_with_delta(
            &self.lua, &self.root, frame.delta_seconds)?;
        for (actor, state) in self.actors.iter().zip(&mut self.states) {
            *state = read_state(&self.lua, actor)?;
        }
        for (_, actor, state) in &mut self.tracked {
            *state = read_state(&self.lua, actor)?;
        }
        // Capture blocks are compiler audit history. Actual tween destinations
        // and commands live in the bounded pending queue; discard old blocks.
        for actor in &self.registry {
            crate::reset_actor_capture(&self.lua, actor)?;
        }
        take_output(&self.lua)
    }

    fn feed_stats(&self, frame: &SongLuaLiveFrame) -> mlua::Result<()> {
        self.stage.raw_set("__songlua_gameplay_seconds", frame.screen_seconds)?;
        for (stats, value) in self.players.iter().zip(frame.players) {
            stats.raw_set("__songlua_possible_dance_points", value.possible_points)?;
            stats.raw_set("__songlua_actual_dance_points", value.actual_points)?;
            stats.raw_set("__songlua_current_possible_dance_points", value.current_possible_points)?;
            stats.raw_set("__songlua_failed", value.failed)?;
            stats.raw_set("__songlua_life", value.life)?;
            stats.raw_set("__songlua_combo", value.combo)?;
            stats.raw_set("__songlua_miss_combo", value.miss_combo)?;
            let counts = stats.raw_get::<Table>("__songlua_tap_scores")?;
            for (name, count) in TAP_SCORES.into_iter().zip(value.tap_scores) {
                counts.raw_set(name, count)?;
            }
        }
        Ok(())
    }

    pub fn context(&self) -> &SongLuaCompileContext { &self.context }

    /// Screen-entry binding from shared Lua actor positions to prepared visual
    /// slots. Ownership remains in this scene; the runtime only receives poses.
    pub fn bind_targets(&mut self, targets: Vec<(usize, SongLuaLiveTarget)>) -> Result<(), String> {
        if targets.iter().any(|(global, _)| *global >= self.states.len()) {
            return Err("live Lua binding index is outside its scene".into());
        }
        self.targets = targets;
        Ok(())
    }

    /// Apply current local poses. The screen marks these actors dynamic in
    /// FrameScratch at entry, before rendering starts; baked timelines stay empty.
    pub fn write_visuals<S>(
        &self,
        visuals: &mut deadsync_gameplay::SongLuaRuntimeVisuals<
            crate::SongLuaOverlayActor<S>, crate::SongLuaCapturedActor,
            crate::gameplay::SongLuaRuntimeOverlayStateDelta>,
    ) -> Result<(), String> {
        for &(global, target) in &self.targets {
            let actor = match target {
                SongLuaLiveTarget::Root(local) => visuals.overlays.get_mut(local),
                SongLuaLiveTarget::Background(layer, local) => visuals.background_visual_layers.get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
                SongLuaLiveTarget::Foreground(layer, local) => visuals.foreground_visual_layers.get_mut(layer)
                    .and_then(|layer| layer.overlays.get_mut(local)),
            }.ok_or_else(|| "live Lua actor has no prepared visual slot".to_owned())?;
            actor.initial_state = self.states[global];
        }
        for (target, state) in self.tracked_states() {
            use crate::SongLuaTrackedActorTarget as T;
            match target {
                T::Player(p) => visuals.player_actors[p].initial_state = state,
                T::PlayerJudgment(p) => visuals.player_actors[p].judgment.initial_state = state,
                T::PlayerCombo(p) => visuals.player_actors[p].combo.initial_state = state,
                T::SongForeground => visuals.song_foreground.initial_state = state,
                T::ScreenLayer(layer) => visuals.screen_layers[layer].initial_state = state,
            }
        }
        visuals.hidden_players = std::array::from_fn(|p| !visuals.player_actors[p].initial_state.visible);
        Ok(())
    }

    pub fn states(&self) -> &[SongLuaOverlayState] { &self.states }

    pub fn tracked_states(&self) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.tracked.iter().map(|(target, _, state)| (*target, *state))
    }

    pub fn take_startup_output(&mut self) -> SongLuaLiveOutput {
        std::mem::take(&mut self.startup_output)
    }
}

fn take_output(lua: &Lua) -> mlua::Result<SongLuaLiveOutput> {
    let request = if let Some(request) = lua.remove_app_data::<crate::host::SongLuaScreenRequest>() {
        Some(SongLuaLiveRequest::NewScreen(request.0))
    } else {
        lua.remove_app_data::<TransitionRequest>().map(|request| SongLuaLiveRequest::Transition(request.0))
    };
    let broadcasts = crate::lua_util::take_runtime_broadcasts(lua);
    let sounds = crate::lua_util::take_runtime_sounds(lua);
    let errors = lua.remove_app_data::<crate::lua_util::SongLuaUpdateErrors>()
        .map_or_else(Vec::new, |errors| errors.0);
    if let Some(history) = lua.globals().raw_get::<Option<Table>>(crate::SONG_LUA_BROADCASTS_KEY)? {
        history.clear()?;
    }
    Ok(SongLuaLiveOutput { request, broadcasts, sounds, errors })
}

pub(crate) fn read_state(lua: &Lua, actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    let mut state = crate::actor_overlay_initial_state(actor).map_err(mlua::Error::external)?;
    crate::lua_util::replay_tween_pose(lua, actor, &mut state);
    state.spin_baked = crate::lua_util::spin_render_pose(actor)?.is_some();
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path, sync::Arc};
    use crate::{SongLuaDifficulty, SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayKind};

    type Compiled = crate::CompiledSongLua<SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>>;

    fn compile(name: &str, source: &str) -> Compiled {
        let dir = std::env::temp_dir().join(format!("deadsync-live-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("test directory");
        let entry = dir.join("default.lua");
        fs::write(&entry, source).expect("test script");
        let mut context = SongLuaCompileContext::new(&dir, name);
        context.live = true;
        context.music_length_seconds = 60.0;
        crate::compile_song_lua_with_default_host(&entry, &context, SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])), |_| None, |_, _| None).expect("live preparation")
    }

    #[test]
    fn live_queue() {
        let mut compiled = std::thread::spawn(|| compile("queue", r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local count = 0
return Def.ActorFrame {
    Name = 'Moving',
    OnCommand = function(self) self:x(0):linear(1):x(100):queuecommand('Later') end,
    LaterCommand = function(self)
        count = count + 1
        self:aux(stats:GetCurrentPossibleDancePoints())
        MESSAGEMAN:Broadcast('ActualTick', { count = count })
    end,
    UnsolicitedMessageCommand = function(self) error('speculative listener called') end,
}
"#)).join().expect("worker transfers the initialized VM");
        let global = compiled.live_actor_indices()[compiled.overlays.iter()
            .position(|actor| actor.name.as_deref() == Some("Moving")).expect("moving actor")];
        let mut scene = compiled.live_scene.take().expect("owned scene");
        assert_eq!(scene.states()[global].x, 0.0, "startup must retain current pose, not destination");
        assert!(scene.take_startup_output().errors.is_empty());
        let mut player = deadsync_gameplay::init_player_runtime();
        player.current_possible_points = 5;
        let mut frame = SongLuaLiveFrame { screen_seconds: 0.5, delta_seconds: 0.5,
            players: [SongLuaLivePlayer::from_player(&player, 10), SongLuaLivePlayer::default()],
            ..SongLuaLiveFrame::default() };
        let output = scene.advance(frame).expect("first half frame");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert!((scene.states()[global].x - 50.0).abs() < 1e-5);
        player.current_possible_points = 10;
        frame.players[0] = SongLuaLivePlayer::from_player(&player, 10);
        frame.screen_seconds = 1.0;
        let output = scene.advance(frame).expect("queue completion");
        assert!(output.errors.is_empty() && output.broadcasts.is_empty());
        assert_eq!(scene.states()[global].x, 100.0);
        // Actor.cpp::UpdateTweening stops when the positive delta is exhausted.
        // A trailing zero-time command runs on the next positive update.
        frame.screen_seconds = 1.1;
        frame.delta_seconds = 0.1;
        let output = scene.advance(frame).expect("trailing command");
        assert!(output.errors.is_empty());
        assert_eq!(output.broadcasts.len(), 1);
        assert_eq!(output.broadcasts[0].1, "ActualTick");
        assert_eq!(scene.states()[global].aux, 10.0);
        frame.screen_seconds = 1.2;
        assert!(scene.advance(frame).expect("later frame").broadcasts.is_empty(), "drain events exactly once");
    }

    #[test]
    fn live_song_clock() {
        let mut compiled = compile("song-clock", r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        CLOCK = {GAMESTATE:GetSongBeat(), GAMESTATE:GetSongPosition():GetSongBeat(),
            GAMESTATE:GetCurMusicSeconds(), GAMESTATE:GetSongBPS(), GAMESTATE:GetSongPosition():GetCurBPS()}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        // Native SongPosition adds the machine offset and group sync offset.
        // A 120 BPM chart has different beats from this 60 BPM song clock.
        scene.context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
            0.009, -0.008, &deadsync_rules::timing::TimingSegments {
                bpms: vec![(0.0, 60.0), (2.0, 120.0)], ..Default::default()
            }, &[]));
        scene.lua.set_app_data(crate::runtime::SongLuaClock(
            scene.context.song_timing.clone().expect("global song timing")));
        for (seconds, player_beat, expected, bps) in [(1.017, 2.0, 1.0, 1.0), (3.017, 6.0, 4.0, 2.0)] {
            let output = scene.advance(SongLuaLiveFrame { beat: player_beat, music_seconds: seconds,
                screen_seconds: seconds, delta_seconds: 0.1, ..Default::default() }).expect("live frame");
            assert!(output.errors.is_empty());
            let values = scene.lua.globals().get::<Table>("CLOCK").expect("actual Lua queries");
            for index in [1, 2] {
                let actual = values.get::<f64>(index).expect("song beat");
                assert!((actual - expected).abs() < 1e-5,
                    "query {index}: seconds={seconds} song={actual} player={player_beat} expected={expected}");
            }
            assert!((values.get::<f64>(3).expect("music seconds") - seconds).abs() < 1e-5);
            for index in [4, 5] { assert_eq!(values.get::<f64>(index).expect("current BPS"), bps); }
        }
    }

    #[test]
    fn live_stats() {
        let mut compiled = compile("stats", r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local stage = STATSMAN:GetCurStageStats()
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        OBSERVED = {stats:GetActualDancePoints(), stats:GetPossibleDancePoints(),
            stats:GetCurrentPossibleDancePoints(), stats:GetCurrentLife(), stats:GetFailed(),
            stats:GetCurrentCombo(), stats:GetCurrentMissCombo(), stats:GetTapNoteScores('TapNoteScore_Miss'),
            stats:GetTapNoteScores('TapNoteScore_W1'), stats:GetTapNoteScores('TapNoteScore_HitMine'),
            stage:GetGameplaySeconds()}
    end)
end }
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        let mut player = deadsync_gameplay::init_player_runtime();
        player.earned_grade_points = -24;
        player.current_possible_points = 20;
        player.life = 0.0;
        player.fail_time = Some(2.0);
        player.combo = 3;
        player.miss_combo = 2;
        player.judgment_counts = [3, 0, 0, 0, 0, 6];
        player.scoring_counts = [0, 0, 0, 0, 0, 2];
        player.mines_hit = 1;
        let output = scene.advance(SongLuaLiveFrame { screen_seconds: 3.5, delta_seconds: 0.5,
            players: [SongLuaLivePlayer::from_player(&player, 25), SongLuaLivePlayer::default()],
            ..SongLuaLiveFrame::default() }).expect("actual stage snapshot");
        assert!(output.errors.is_empty());
        let observed = scene.lua.globals().get::<Table>("OBSERVED").expect("callback values");
        assert_eq!(observed.get::<i32>(1).expect("actual"), -24);
        assert_eq!(observed.get::<i32>(2).expect("maximum"), 25);
        assert_eq!(observed.get::<i32>(3).expect("progress"), 20);
        assert_eq!(observed.get::<f32>(4).expect("life"), 0.0);
        assert!(observed.get::<bool>(5).expect("failure"));
        for (index, expected) in [(6, 3), (7, 2), (8, 6), (9, 3), (10, 1)] {
            assert_eq!(observed.get::<u32>(index).expect("native count"), expected);
        }
        assert_eq!(observed.get::<f64>(11).expect("screen seconds"), 3.5);
    }

    #[test]
    fn live_reload() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
        let native: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/startup-reload-native.json")).expect("native control");
        let song = deadsync_simfile::song::parse_song_meta_file(&fixtures.join("startup-reload.ssc"),
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0.0, |_| 0.0).expect("native simfile");
        let mut context = SongLuaCompileContext::new(&fixtures, "Native reload");
        context.live = true;
        context.song_steps = song.charts.iter().map(crate::SongLuaStep::from).collect();
        context.player_step_indices[0] = context.song_steps.iter().position(|steps|
            steps.difficulty == SongLuaDifficulty::Challenge);
        context.players[0].difficulty = SongLuaDifficulty::Challenge;
        let mut compiled = crate::compile_song_lua_with_default_host(&fixtures.join("startup-reload.lua"),
            &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("retained native control");
        assert_eq!(compiled.startup_reloads, 1);
        let scene = compiled.live_scene.take().expect("single VM");
        assert_eq!(scene.lua.globals().get::<u32>("HARNESS_NATIVE_RELOAD_COUNT").expect("same VM count"), 2);
        let stats = &scene.players[0];
        assert_eq!(stats.get::<Function>("GetPossibleDancePoints").expect("maximum getter")
            .call::<i32>(stats.clone()).expect("current maximum"), native["snapshots"][1]["possible"].as_i64().expect("native maximum") as i32);
        assert_eq!(scene.context().song_steps[scene.context().player_step_indices[0].expect("selected chart")]
            .difficulty, SongLuaDifficulty::Easy);
    }

    #[test]
    fn live_layers() {
        let dir = std::env::temp_dir().join(format!("deadsync-live-layers-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("layer directory");
        let background = dir.join("background.lua");
        let foreground = dir.join("foreground.lua");
        fs::write(&background, r#"
LIVE_ORDER = 0
return Def.ActorFrame { Name = 'Background', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then LIVE_ORDER = LIVE_ORDER + 1; self:x(LIVE_ORDER) end
    end)
end }
"#).expect("background script");
        fs::write(&foreground, r#"
return Def.ActorFrame { Name = 'Foreground', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then self:x(2 * LIVE_ORDER) end
    end)
end }
"#).expect("foreground script");
        let mut context = SongLuaCompileContext::new(&dir, "shared layers");
        context.live = true;
        context.background_layer_count = 1;
        let mut layers = crate::compile_song_lua_layers_with_default_host(&[&background, &foreground],
            1, &context, SongLuaNoteskinResolver::default(), |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None::<crate::SongLuaOverlayModelLayer<()>>, |_, _| None).expect("one shared VM");
        assert!(layers[0].live_scene.is_none());
        let mut scene = layers[1].live_scene.take().expect("primary owns the VM");
        let frame = SongLuaLiveFrame { screen_seconds: 0.1, delta_seconds: 0.1, ..SongLuaLiveFrame::default() };
        assert!(scene.advance(frame).expect("shared update").errors.is_empty());
        for (layer, name, x) in [(0, "Background", 1.0), (1, "Foreground", 2.0)] {
            let local = layers[layer].overlays.iter().position(|actor|
                actor.name.as_deref() == Some(name)).expect("layer actor");
            assert_eq!(scene.states()[layers[layer].live_actor_indices()[local]].x, x);
        }
    }

    #[test]
    fn live_requests() {
        let mut compiled = compile("transition", r#"
return Def.ActorFrame {
    OffCommand = function(self) self:aux(17) end,
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                assert(self:getaux() == 17, 'OffCommand must run immediately')
                error('live diagnostic')
            end
        end)
    end,
}
"#);
        let mut scene = compiled.live_scene.take().expect("scene");
        let frame = SongLuaLiveFrame { screen_seconds: 0.1, delta_seconds: 0.1, ..SongLuaLiveFrame::default() };
        let output = scene.advance(frame).expect("native partial callback");
        assert_eq!(output.request, Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into())));
        assert_eq!(output.errors.len(), 1);
        assert!(output.errors[0].contains("live diagnostic"), "{:?}", output.errors);
        assert!(scene.advance(frame).expect("next frame").errors.is_empty(), "native error is reported once");
    }
}
