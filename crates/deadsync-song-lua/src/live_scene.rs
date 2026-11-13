//! Retained screen lifecycle over the shared Rust VM and verified live phases.
use super::*;
use crate::{SongLuaLiveFrame, SongLuaLiveOutput, SongLuaOverlayState, SongLuaTrackedActorTarget};
use actors::compiler::Query;
use actors::live_scene::Phase;
use std::path::PathBuf;
mod gameplay;
mod interactions;
mod navigation;
#[cfg(feature = "test-support")]
mod replay;
mod visuals;
type ReloadParts = (
    SongLiveRuntime,
    SongLuaCompileContext,
    Vec<PathBuf>,
    usize,
    SongLuaLiveOutput,
);

/// Actor bindings collected by the live compiler before transferring its VM.
/// Values remain rooted by the active screen until the constructor retains them.
pub struct SongLiveSceneBindings {
    pub screen: ActorScene,
    pub root: Value,
    pub actors: Vec<Value>,
    pub tracked: Vec<(SongLuaTrackedActorTarget, Value)>,
    pub entry_paths: Vec<PathBuf>,
    pub primary_index: usize,
    pub startup_broadcasts: Vec<(f32, String, bool)>,
}
pub struct SongLiveScene {
    runtime: SongLiveRuntime,
    screen: ActorScene,
    retained: Root,
    frames: SongLiveFrames,
    layers: SongLiveLayers,
    context: SongLuaCompileContext,
    actors: Vec<Value>,
    tracked: Vec<(SongLuaTrackedActorTarget, Value)>,
    states: Vec<SongLuaOverlayState>,
    tracked_states: Vec<SongLuaOverlayState>,
    targets: Vec<(usize, crate::SongLuaLiveTarget)>,
    visual_targets: rustc_hash::FxHashMap<usize, crate::SongLuaLiveTarget>,
    entry_paths: Vec<PathBuf>,
    primary_index: usize,
    startup_output: SongLuaLiveOutput,
    last_music: f32,
    prepared: bool,
    outro_started: bool,
    outro_reset: bool,
    outro_failed: bool,
    off_pending: bool,
    ended: bool,
    updated: bool,
    #[cfg(feature = "test-support")]
    option_clock_deltas: Option<[f32; LUA_PLAYERS + 1]>,
    #[cfg(feature = "test-support")]
    overlay_observation: Arc<Mutex<Option<crate::SongLuaOverlayStageObservation>>>,
}
impl std::fmt::Debug for SongLiveScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SongLuaLiveScene")
            .field("actors", &self.actors.len())
            .field("tracked", &self.tracked.len())
            .finish_non_exhaustive()
    }
}
impl SongRuntime {
    /// Begin a new live load before theme or authored Init callbacks. Retained
    /// selected statistics survive until the new scene selects its own owners.
    pub fn begin_live_song_load(&mut self) {
        self.statistics.lock().unwrap().players = None;
        self.actors.begin_live_load(&mut self.vm);
        self.begin_screen_request_capture();
    }
}
impl SongLiveScene {
    pub fn new(
        runtime: SongRuntime,
        context: &SongLuaCompileContext,
        b: SongLiveSceneBindings,
    ) -> Result<Self, Error> {
        Self::new_shared(SongLiveRuntime::new(runtime), context, b)
    }
    pub(super) fn new_shared(
        owner: SongLiveRuntime,
        context: &SongLuaCompileContext,
        b: SongLiveSceneBindings,
    ) -> Result<Self, Error> {
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if !context.live
            || runtime.actor_scene_root(b.screen).is_none()
            || !runtime.actors.has_live_load(&mut runtime.vm)
        {
            return Err(error(runtime,b"live scene requires an active live screen and begin_live_song_load before construction"));
        }
        let batch = runtime.vm.try_new_table()?;
        let retained = runtime.vm.pin(Value::Table(batch))?;
        let result = (|| {
            runtime.vm.raw_table_set(batch, Value::Number(1.), b.root)?;
            for (slot, values) in [
                (2., b.actors.as_slice()),
                (3., &b.tracked.iter().map(|(_, v)| *v).collect::<Vec<_>>()),
            ] {
                let list = runtime.vm.try_new_table()?;
                runtime
                    .vm
                    .raw_table_set(batch, Value::Number(slot), Value::Table(list))?;
                for (i, &v) in values.iter().enumerate() {
                    if !matches!(v, Value::Table(_)) {
                        return Err(error(runtime, b"live actor table expected"));
                    }
                    runtime
                        .vm
                        .raw_table_set(list, Value::Number((i + 1) as f64), v)?;
                }
            }
            runtime.actors.prepare_input_plan(&mut runtime.vm, b.root)?;
            let frames = runtime.new_song_live_frames(context)?;
            let layers = match runtime.new_song_live_layers(b.screen, context) {
                Ok(Some(l)) => l,
                other => {
                    let mut f = frames;
                    f.release(runtime)?;
                    return Err(other
                        .err()
                        .unwrap_or_else(|| error(runtime, b"live layers were released")));
                }
            };
            Ok((frames, layers))
        })();
        let (frames, layers) = match result {
            Ok(v) => v,
            Err(e) => {
                runtime.vm.unpin(retained)?;
                return Err(e);
            }
        };
        runtime.begin_actor_playback();
        drop(guard);
        let mut scene = Self {
            runtime: owner,
            screen: b.screen,
            retained,
            frames,
            layers,
            context: context.clone(),
            actors: b.actors,
            tracked: b.tracked,
            states: Vec::new(),
            tracked_states: Vec::new(),
            targets: Vec::new(),
            visual_targets: Default::default(),
            entry_paths: b.entry_paths,
            primary_index: b.primary_index,
            startup_output: Default::default(),
            last_music: -9999.,
            prepared: false,
            outro_started: false,
            outro_reset: false,
            outro_failed: false,
            off_pending: false,
            ended: false,
            updated: false,
            #[cfg(feature = "test-support")]
            option_clock_deltas: None,
            #[cfg(feature = "test-support")]
            overlay_observation: Default::default(),
        };
        let owner = scene.runtime.clone();
        let mut guard = owner.access();
        scene.read_states(&mut guard)?;
        let mut startup = scene.take_output(&mut guard)?;
        let mut prior = b.startup_broadcasts;
        prior.append(&mut startup.broadcasts);
        startup.broadcasts = prior;
        scene.startup_output = startup;
        Ok(scene)
    }
    #[cfg(feature = "test-support")]
    pub(crate) fn fixture_actor_handles(&self) -> Result<(Value, Vec<Value>), Error> {
        let mut runtime = self.runtime.access();
        Ok((self.root(&mut runtime)?, self.actors.clone()))
    }
    #[cfg(feature = "test-support")]
    pub(crate) fn fixture_initial_context(&self) -> &SongLuaCompileContext {
        &self.context
    }
    #[cfg(feature = "test-support")]
    pub(crate) fn fixture_take_output(&mut self) -> Result<SongLuaLiveOutput, Error> {
        let owner = self.runtime.clone();
        let mut runtime = owner.access();
        self.take_output(&mut runtime)
    }
    pub fn runtime(&self) -> std::sync::MutexGuard<'_, SongRuntime> {
        self.runtime.access()
    }
    pub fn runtime_mut(&mut self) -> std::sync::MutexGuard<'_, SongRuntime> {
        self.runtime.access()
    }
    pub fn states(&self) -> &[SongLuaOverlayState] {
        &self.states
    }
    #[cfg(feature = "test-support")]
    pub fn set_arrow_clock_inputs(
        &mut self,
        tipsy: [Option<f64>; LUA_PLAYERS],
        tan_tipsy: [Option<f64>; LUA_PLAYERS],
    ) {
        self.runtime
            .access()
            .set_arrow_clock_inputs(tipsy, tan_tipsy);
    }
    pub fn tracked_states(
        &self,
    ) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.tracked
            .iter()
            .zip(&self.tracked_states)
            .map(|((t, _), s)| (*t, *s))
    }
    pub fn take_startup_output(&mut self) -> SongLuaLiveOutput {
        std::mem::take(&mut self.startup_output)
    }
    pub fn matches_entries(&self, paths: &[&std::path::Path], primary: usize) -> bool {
        primary == self.primary_index
            && paths.len() == self.entry_paths.len()
            && paths
                .iter()
                .zip(&self.entry_paths)
                .all(|(p, s)| crate::entry_file_path(p).as_ref() == Some(s))
    }
    pub fn selected_context(&self) -> Result<SongLuaCompileContext, Error> {
        self.runtime
            .access()
            .selected_gameplay_context(&self.context)
    }
    pub fn note_gameplay_outro_after_actors(&mut self) {
        self.outro_started = true;
    }
    pub fn outro_off_dispatched(&self) -> bool {
        self.outro_reset && !self.off_pending
    }
    pub fn failed_outro(&self) -> bool {
        self.outro_failed
    }
    pub fn prepare_gameplay_frame(&mut self, frame: SongLuaLiveFrame) -> Result<(), Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if !self.prepared {
            #[cfg(feature = "test-support")]
            let deltas = self.option_clock_deltas.take();
            #[cfg(not(feature = "test-support"))]
            let deltas = None;
            self.frames
                .prepare_gameplay_frame(runtime, &frame, deltas, self.outro_started)?;
            let beat = runtime.values()?.0;
            if let Err(e) = self.layers.prepare_backgrounds(
                runtime,
                beat as f64,
                frame.music_seconds,
                self.last_music,
            ) {
                self.frames.discard_prepared();
                return Err(e);
            }
            self.prepared = true;
        }
        Ok(())
    }
    pub fn update(&mut self, frame: SongLuaLiveFrame) -> Result<SongLuaLiveOutput, Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if self.ended {
            return Err(error(runtime, b"cannot advance an ended song Lua scene"));
        }
        let prepared = std::mem::take(&mut self.prepared);
        #[cfg(feature = "test-support")]
        let deltas = if prepared {
            None
        } else {
            self.option_clock_deltas.take()
        };
        #[cfg(not(feature = "test-support"))]
        let deltas = None;
        let beat = self
            .frames
            .update(runtime, &frame, deltas, self.outro_started)?;
        if !prepared {
            self.layers
                .prepare_backgrounds(runtime, beat, frame.music_seconds, self.last_music)?;
        }
        self.layers.advance(
            runtime,
            beat,
            frame.music_seconds,
            self.last_music,
            frame.delta_seconds,
        )?;
        if self.off_pending {
            self.off(runtime)?;
            self.off_pending = false;
        }
        if self.outro_started && !self.outro_reset {
            runtime.clear_attack_options()?;
            self.outro_reset = true;
            let failed = self.frames.failed_players(runtime)?;
            let mut all = true;
            for (i, p) in self.context.players.iter().enumerate() {
                if p.enabled {
                    if failed[i] {
                        self.frames.record_life(runtime, i, 0.)?;
                    } else {
                        all = false;
                    }
                }
            }
            self.outro_failed = all;
            if all {
                self.off_pending = true;
            } else {
                self.off(runtime)?;
            }
        }
        self.outro_started |= frame.gameplay_outro;
        self.last_music = frame.music_seconds as f32;
        self.read_states(runtime)?;
        if let Err(e) = self.frames.finish_health_update(runtime) {
            let batch = runtime.vm.rooted(self.retained)?;
            runtime
                .actors
                .live_phase(&mut runtime.vm, self.screen, batch, Phase::Health(e))?;
        }
        runtime.update_arrow_effects()?;
        self.updated = true;
        self.take_output(runtime)
    }
    pub fn input(
        &mut self,
        event: &crate::SongLuaInputEvent<'_>,
    ) -> Result<(bool, SongLuaLiveOutput), Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if self.ended {
            return Err(error(
                runtime,
                b"cannot send input to an ended song Lua scene",
            ));
        }
        let (handled, errors) = runtime.dispatch_input(event)?;
        self.read_states(runtime)?;
        let mut output = self.take_output(runtime)?;
        output.errors.extend(errors);
        Ok((handled, output))
    }
    pub fn has_custom_draw(&mut self) -> Result<bool, Error> {
        Self::custom_draw(&mut self.runtime.access())
    }
    fn custom_draw(runtime: &mut SongRuntime) -> Result<bool, Error> {
        runtime.vm.with_native_stack(|s| {
            let base = s.top();
            let g = s.value(-10002).unwrap();
            s.push(g)?;
            s.push_bytes(crate::draw_commands::DRAW_INSTALLED_KEY.as_bytes())?;
            s.get_table(s.top_offset() - 1, true)?;
            let v = s.to_boolean(-1);
            s.set_top(base)?;
            Ok(v)
        })
    }
    pub fn draw(&mut self) -> Result<SongLuaLiveOutput, Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if self.ended || !self.updated {
            return Ok(Default::default());
        }
        if Self::custom_draw(runtime)? {
            let root = self.root(runtime)?;
            runtime
                .actors
                .compiler_read(&mut runtime.vm, root, Query::Draw)?;
            self.read_states(runtime)?;
        }
        self.take_output(runtime)
    }
    pub fn advance(&mut self, frame: SongLuaLiveFrame) -> Result<SongLuaLiveOutput, Error> {
        let mut out = self.update(frame)?;
        out.append(self.draw()?);
        Ok(out)
    }
    pub fn end(&mut self) -> Result<SongLuaLiveOutput, Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if self.ended {
            return Ok(Default::default());
        }
        self.ended = true;
        let root = self.root(runtime)?;
        runtime
            .actors
            .compiler_read(&mut runtime.vm, root, Query::End)?;
        let batch = runtime.vm.rooted(self.retained)?;
        runtime
            .actors
            .live_phase(&mut runtime.vm, self.screen, batch, Phase::EndSounds)?;
        self.take_output(runtime)
    }
    pub fn into_runtime(mut self) -> Result<SongLiveRuntime, Error> {
        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;
        if !self.ended {
            return Err(error(
                runtime,
                b"end the live scene before retaining its runtime",
            ));
        }
        self.layers.release(runtime)?;
        self.frames.release(runtime)?;
        runtime.vm.unpin(self.retained)?;
        runtime.retire_actor_scene(self.screen)?;
        drop(guard);
        Ok(owner)
    }
    pub(super) fn reload_parts(mut self) -> Result<ReloadParts, Error> {
        let ending = self.end()?;
        let context = self.selected_context()?;
        let paths = self.entry_paths.clone();
        let primary = self.primary_index;
        Ok((self.into_runtime()?, context, paths, primary, ending))
    }
    pub(super) fn prepend_startup_output(&mut self, mut ending: SongLuaLiveOutput) {
        ending.append(self.take_startup_output());
        self.startup_output = ending;
    }
    fn root(&self, runtime: &mut SongRuntime) -> Result<Value, Error> {
        let Value::Table(b) = runtime.vm.rooted(self.retained)? else {
            unreachable!()
        };
        runtime.vm.raw_table_get(b, Value::Number(1.))
    }
    fn read_states(&mut self, runtime: &mut SongRuntime) -> Result<(), Error> {
        let batch = runtime.vm.rooted(self.retained)?;
        let mut states =
            runtime
                .actors
                .live_phase(&mut runtime.vm, self.screen, batch, Phase::States)?;
        self.tracked_states = states.split_off(self.actors.len());
        self.states = states;
        Ok(())
    }
    fn off(&mut self, runtime: &mut SongRuntime) -> Result<(), Error> {
        let batch = runtime.vm.rooted(self.retained)?;
        runtime
            .actors
            .live_phase(&mut runtime.vm, self.screen, batch, Phase::Off)
            .map(|_| ())
    }
    fn take_output(&mut self, runtime: &mut SongRuntime) -> Result<SongLuaLiveOutput, Error> {
        let (request, source_requests) = runtime.take_screen_requests();
        let broadcasts = runtime.actors.take_song_broadcasts(&mut runtime.vm);
        let source_broadcasts = runtime.actors.take_live_sources(&mut runtime.vm);
        let sounds = runtime.take_prepared_sounds();
        let actor_sounds = runtime.take_actor_sounds();
        let errors = runtime.actors.take_callback_errors(&mut runtime.vm);
        #[cfg(feature = "test-support")]
        let coverage_gaps = runtime.actors.take_message_coverage(&mut runtime.vm);
        #[cfg(not(feature = "test-support"))]
        let coverage_gaps = Vec::new();
        let batch = runtime.vm.rooted(self.retained)?;
        runtime
            .actors
            .live_phase(&mut runtime.vm, self.screen, batch, Phase::ClearHistory)?;
        let player_writes = runtime.take_player_writes();
        let _ = runtime.take_actor_diagnostics();
        let mut sources: Vec<_> = self
            .actors
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, crate::SongLuaDrawSource::Overlay(i)))
            .collect();
        sources.extend(self.tracked.iter().map(|&(t, v)| (v, draw_source(t))));
        let draw_frames = runtime
            .actors
            .take_live_draw_frames(&mut runtime.vm, &sources)
            .map_err(|e| error(runtime, e.as_bytes()))?;
        Ok(SongLuaLiveOutput {
            actor_sounds,
            request,
            source_requests,
            broadcasts,
            source_broadcasts,
            sounds,
            errors,
            coverage_gaps,
            player_writes,
            draw_frames,
        })
    }
}
fn error(runtime: &mut SongRuntime, message: &[u8]) -> Error {
    match runtime.vm.with_native_stack(|s| Ok(s.error(message))) {
        Ok(e) | Err(e) => e,
    }
}
fn draw_source(t: SongLuaTrackedActorTarget) -> crate::SongLuaDrawSource {
    use crate::SongLuaDrawSource as D;
    use crate::SongLuaTrackedActorTarget as T;
    match t {
        T::Player(i) => D::Player(i),
        T::PlayerJudgment(i) => D::Judgment(i),
        T::PlayerCombo(i) => D::Combo(i),
        T::SongForeground => D::SongForeground,
        T::ScreenLayer(i) => D::ScreenLayer(i),
    }
}

#[cfg(feature = "test-support")]
impl SongLiveScene {
    pub(crate) fn fixture_end_update_capture(&self) {
        let mut guard = self.runtime.access();
        let runtime = &mut *guard;
        runtime
            .actors
            .end_song_update_capture(&mut runtime.vm, self.screen)
    }
}
