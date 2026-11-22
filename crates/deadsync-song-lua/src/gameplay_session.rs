//! Game-owned ordering for live song Lua, gameplay updates, input and screen exit.
//!
//! The host supplies presentation work and resolves presentation destinations.
//! This module owns frame ordering and the meaning of authored gameplay exits.

use crate::{
    SongLuaInputEvent, SongLuaInputType, SongLuaLiveFrame, SongLuaLiveOutput, SongLuaLiveRequest,
    SongLuaLiveScene,
};
use crate::playback::{
    FrameScratch, GameplayCoreState, NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
    NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY, SongMedia, prewarm_texture_bindings,
};
use deadlib_present::actors::{ActorResourceArena, SpriteSource};
use deadlib_present::texture::TextureContext;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    GameplayAction, GameplayAudioSnapshot, GameplayExit, GameplayInputPlayerSide,
    GameplayProfileData, GameplayRuntimeState, handle_core_input, update_core,
    update_core_with_pre_player,
};
use deadsync_input::{
    CallbackInputState, HeldCallbackInputs, InputEvent, PhysicalInput, VirtualAction,
};
use deadsync_notefield::{NotefieldFieldScratch, NotefieldNoteskinView, NotefieldSettings};
use deadsync_noteskin::{NoteskinRuntime, NoteskinSlot};
use std::cell::{RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

pub const SONG_LUA_OUT_DELAY: f32 = 0.5;
pub const SONG_LUA_OUT_FADE_DURATION: f32 = 1.0;
pub const GAMEPLAY_EXIT_TRANSITION_SECONDS: f32 = SONG_LUA_OUT_DELAY + SONG_LUA_OUT_FADE_DURATION;

/// Presentation-only snapshot of the native gameplay exit clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaOutView {
    pub phase: u8,
    pub left: f32,
}

/// Ordered output of a gameplay frame. Presentation destinations are resolved
/// before the live scene ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaGameplayEffect<S> {
    Action(GameplayAction),
    ReloadSongLua,
    Navigate(S),
}

/// Noteskin components resolved by the game for the lifetime of a stage.
pub trait StageNoteskins<S: NoteskinSlot> {
    /// Base, mine, receptor, and tap explosion components for cache warmup.
    fn components_for_player(&self, player: usize) -> [Option<&NoteskinRuntime<S>>; 4];

    fn actor_texture_source(
        &self,
        slot: &S,
        _arena: &ActorResourceArena,
        textures: &impl TextureContext,
    ) -> SpriteSource {
        let key = slot.texture_key_shared();
        SpriteSource::TextureHandle {
            handle: textures.texture_handle(&key),
            generation: textures.texture_registry_generation(),
            key,
        }
    }

    /// Components used for drawing after applying profile settings and fallbacks.
    fn selected_components_for_player(
        &self,
        player: usize,
        hide_tap_explosion: bool,
    ) -> [Option<&NoteskinRuntime<S>>; 4] {
        let [base, mine, receptor, tap_explosion] = self.components_for_player(player);
        let tap_explosion = if hide_tap_explosion {
            None
        } else {
            tap_explosion.or(base)
        };
        [base, mine.or(base), receptor.or(base), tap_explosion]
    }
}

impl<S: NoteskinSlot> StageNoteskins<S> for () {
    fn components_for_player(&self, _player: usize) -> [Option<&NoteskinRuntime<S>>; 4] {
        [None; 4]
    }
}

/// The live simulation, authored callbacks, and playback state for a stage.
pub struct GameplayStage<G, N = ()> {
    pub gameplay: G,
    pub song_lua: SongLuaGameplaySession,
    pub song_media: SongMedia,
    pub song_scratch: Option<Box<FrameScratch>>,
    pub noteskin_assets: N,
    actor_resources: ActorResourceArena,
    notefield_settings: [NotefieldSettings; MAX_PLAYERS],
    // These caches also survive replacing the authored compositor storage.
    notefield_scratch: Option<[RefCell<NotefieldFieldScratch>; MAX_PLAYERS]>,
}

impl<G> GameplayStage<G> {
    /// Construct a stage for running the controller without a compositor.
    pub fn new(gameplay: G, song_lua: SongLuaGameplaySession) -> Self {
        Self {
            gameplay,
            song_lua,
            song_media: SongMedia::default(),
            song_scratch: None,
            noteskin_assets: (),
            actor_resources: ActorResourceArena::new(0),
            notefield_settings: [NotefieldSettings::default(); MAX_PLAYERS],
            notefield_scratch: None,
        }
    }
}

impl<G, N> GameplayStage<G, N> {
    pub const fn actor_resources(&self) -> &ActorResourceArena {
        &self.actor_resources
    }

    /// Warm dynamic render storage after successful callback installation.
    pub fn song_lua_attachment_parts(
        &mut self,
    ) -> (&mut G, &mut SongLuaGameplaySession, impl FnOnce() + '_) {
        let song_scratch = &mut self.song_scratch;
        (&mut self.gameplay, &mut self.song_lua, move || {
            if let Some(scratch) = song_scratch {
                scratch.enable_dynamic_scene();
            }
        })
    }

    pub fn song_frame(&self) -> &FrameScratch {
        self.song_scratch
            .as_deref()
            .expect("song frame restored after composition")
    }

    pub fn render_targets(&self) -> &[deadlib_present::actors::RenderTarget] {
        self.song_scratch
            .as_deref()
            .map_or(&[], FrameScratch::render_targets)
    }

    pub(crate) fn notefield_scratch(&self, player: usize) -> RefMut<'_, NotefieldFieldScratch> {
        self.notefield_scratch
            .as_ref()
            .expect("field storage prepared with stage playback")[player]
            .borrow_mut()
    }

    pub(crate) fn notefield_settings(&self, player: usize) -> NotefieldSettings {
        self.notefield_settings[player]
    }
}

impl<P: GameplayProfileData, S: NoteskinSlot + Clone> GameplayStage<GameplayCoreState<P, S>> {
    /// Prepare the reusable song media and compositor storage for a stage.
    pub fn with_song_playback(
        gameplay: GameplayCoreState<P, S>,
        song_lua: SongLuaGameplaySession,
        sound_paths: Vec<PathBuf>,
    ) -> Self {
        Self::with_song_playback_and_noteskins(
            gameplay,
            song_lua,
            sound_paths,
            (),
            [NotefieldSettings::default(); MAX_PLAYERS],
        )
    }
}

impl<P: GameplayProfileData, S: NoteskinSlot + Clone, N: StageNoteskins<S>>
    GameplayStage<GameplayCoreState<P, S>, N>
{
    /// Prepare player field caches from the noteskins selected for the stage.
    pub fn with_song_playback_and_noteskins(
        gameplay: GameplayCoreState<P, S>,
        song_lua: SongLuaGameplaySession,
        sound_paths: Vec<PathBuf>,
        noteskin_assets: N,
        notefield_settings: [NotefieldSettings; MAX_PLAYERS],
    ) -> Self {
        let song_scratch = Some(Box::new(FrameScratch::new(&gameplay)));
        let song_media = SongMedia::new(&gameplay, sound_paths);
        let skins: [_; MAX_PLAYERS] =
            std::array::from_fn(|player| noteskin_assets.components_for_player(player));
        let actor_resources = ActorResourceArena::default();
        for component in 0..4 {
            for skin in skins
                .iter()
                .take(gameplay.num_players())
                .filter_map(|skins| skins[component])
            {
                skin.for_each_slot(|slot| {
                    let _ = noteskin_assets.actor_texture_source(
                        slot,
                        &actor_resources,
                        &deadlib_assets::METADATA_TEXTURE_CONTEXT,
                    );
                });
            }
        }
        let notefield_scratch = Some(std::array::from_fn(|player| {
            let active = usize::from(player < gameplay.num_players());
            RefCell::new(NotefieldFieldScratch::new(
                active * gameplay.cols_per_player(),
                [
                    active * NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
                    active * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
                ],
                skins[player],
            ))
        }));
        Self {
            gameplay,
            song_lua,
            song_media,
            song_scratch,
            noteskin_assets,
            actor_resources,
            notefield_settings,
            notefield_scratch,
        }
    }

    pub(crate) fn notefield_noteskins(&self, player: usize) -> NotefieldNoteskinView<'_, S> {
        let [base, mine, receptor, tap_explosion] = self
            .noteskin_assets
            .selected_components_for_player(
                player,
                self.notefield_settings[player].hide_tap_explosion_noteskin,
            );
        NotefieldNoteskinView {
            base,
            mine,
            receptor,
            tap_explosion,
        }
    }

    pub fn prewarm_song_texture_bindings(&mut self, assets: &deadlib_assets::AssetManager) {
        if let Some(scratch) = self.song_scratch.as_deref_mut() {
            prewarm_texture_bindings(&self.gameplay, scratch, assets);
        }
    }

    #[must_use]
    pub fn refresh_song_foreground(&mut self) -> bool {
        self.song_media.refresh_foreground(&self.gameplay)
    }

    pub fn for_each_song_video_position(&mut self, visit: impl FnMut(&Path, f32, bool)) {
        self.song_media.for_each_video_position(&self.gameplay, visit);
    }

    pub fn for_each_scheduled_song_sound(&mut self, previous: f32, visit: impl FnMut(&Path)) {
        self.song_media
            .for_each_sound_event(previous, self.gameplay.current_music_time_display(), visit);
    }
}

impl<G, N> Deref for GameplayStage<G, N> {
    type Target = G;

    #[inline]
    fn deref(&self) -> &G {
        &self.gameplay
    }
}

impl<G, N> DerefMut for GameplayStage<G, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut G {
        &mut self.gameplay
    }
}

impl<P: GameplayProfileData, O, C, D, N> GameplayStage<GameplayRuntimeState<P, O, C, D>, N>
where
    GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
{
    pub fn update<S>(
        &mut self,
        delta_time: f32,
        audio_snapshot: GameplayAudioSnapshot,
        fallback_host_nanos: impl FnOnce() -> u64,
        resolve_presentation: impl FnOnce(&str) -> Result<S, String>,
        emit: impl FnMut(SongLuaGameplayEffect<S>),
        update_visuals: impl FnOnce(&GameplayRuntimeState<P, O, C, D>, f32),
    ) {
        drive_frame(
            self,
            |stage| (&mut stage.gameplay, &mut stage.song_lua),
            delta_time,
            audio_snapshot,
            fallback_host_nanos,
            |name| match name.unwrap_or_default() {
                "" | "ScreenEvaluationStage" => Ok(SongLuaGameplayEffect::Action(
                    GameplayAction::NavigateNoFade(GameplayExit::Complete),
                )),
                "ScreenSelectMusic" => Ok(SongLuaGameplayEffect::Action(
                    GameplayAction::NavigateNoFade(GameplayExit::Cancel),
                )),
                name => resolve_presentation(name).map(SongLuaGameplayEffect::Navigate),
            },
            emit,
            |stage, delta| update_visuals(&stage.gameplay, delta),
            |_| {},
        );
    }

    #[must_use]
    pub fn handle_input(
        &mut self,
        event: &InputEvent,
        physical: Option<PhysicalInput>,
    ) -> GameplayAction {
        handle_input_with_action(
            &mut self.gameplay,
            &mut self.song_lua,
            event,
            physical,
            std::convert::identity,
        )
        .unwrap_or(GameplayAction::None)
    }

    pub fn take_output(&mut self) -> SongLuaLiveOutput {
        self.song_lua.take_output(&mut self.gameplay)
    }
}

/// Statically bound access to gameplay and its presentation host.
///
/// Implementations only provide state access, presentation updates and effect
/// conversion. The full frame and input order are fixed by this module.
pub trait SongLuaGameplayHost: Sized {
    type Profile: GameplayProfileData;
    type OverlayActor;
    type CapturedActor;
    type StateDelta;
    type Effect;

    fn gameplay_parts(&mut self) -> (&mut HostGame<Self>, &mut SongLuaGameplaySession);

    fn update_gameplay_visuals(&mut self, delta_time: f32);

    fn gameplay_effect(action: GameplayAction) -> Self::Effect;
    fn reload_song_lua_effect() -> Self::Effect;

    /// Validate and resolve a presentation destination before the scene ends.
    /// ScreenGameplay is reserved and handled directly by the game controller.
    fn next_screen_effect(name: Option<&str>) -> Result<Self::Effect, String>;
}

type HostGame<H> = GameplayRuntimeState<
    <H as SongLuaGameplayHost>::Profile,
    <H as SongLuaGameplayHost>::OverlayActor,
    <H as SongLuaGameplayHost>::CapturedActor,
    <H as SongLuaGameplayHost>::StateDelta,
>;

type LiveGame<P, N, M> = GameplayRuntimeState<
    P,
    crate::SongLuaOverlayActor<
        crate::SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>,
    >,
    crate::SongLuaCapturedActor,
    crate::gameplay::SongLuaRuntimeOverlayStateDelta,
>;

mod sealed {
    pub trait Sealed {}

    impl<P, N, M> Sealed for super::LiveGame<P, N, M> {}
}

/// The fixed live-visual bridge for game states with song Lua actor storage.
///
/// This is sealed so hosts cannot replace publication with theme-specific logic.
pub trait SongLuaGameplayVisualState: sealed::Sealed {
    fn publish_song_lua_visuals(&mut self, scene: &mut SongLuaLiveScene) -> Result<(), String>;
}

impl<P, N, M> SongLuaGameplayVisualState for LiveGame<P, N, M> {
    #[inline]
    fn publish_song_lua_visuals(&mut self, scene: &mut SongLuaLiveScene) -> Result<(), String> {
        scene.write_visuals(&mut self.mods.song_lua_visuals)
    }
}

// ScreenGameplay out.lua sleeps 0.5s then tweens for 1s. Actor.cpp consumes
// each step with f32 arithmetic; Transition::UpdateInternal dispatches its
// completion on the following update, before advancing the finished tween.
#[derive(Debug)]
struct SongLuaOut {
    phase: u8,
    left: f32,
}

impl SongLuaOut {
    fn advance(&mut self, mut delta: f32) -> bool {
        if self.phase == 2 {
            return true;
        }
        while self.phase < 2 {
            let used = self.left.min(delta);
            delta -= used;
            self.left -= used;
            if self.left > 0.0 {
                break;
            }
            self.phase += 1;
            self.left = if self.phase == 1 {
                SONG_LUA_OUT_FADE_DURATION
            } else {
                0.0
            };
            if delta <= 0.0 {
                break;
            }
        }
        false
    }
}

#[cfg(feature = "test-support")]
pub(crate) fn out_clock_completion_frame(deltas: &[f32]) -> Option<usize> {
    let mut out = SongLuaOut {
        phase: 0,
        left: SONG_LUA_OUT_DELAY,
    };
    deltas.iter().position(|&delta| out.advance(delta))
}

/// Record emitted game actions without interpreting presentation effect values.
#[derive(Default)]
struct FrameActions {
    pop: bool,
    screen_changes: bool,
}

impl FrameActions {
    fn record_gameplay_action(&mut self, action: GameplayAction) {
        match action {
            GameplayAction::NavigateNoFade(_) => {
                self.pop = true;
                self.screen_changes = true;
            }
            GameplayAction::Navigate(_) => self.screen_changes = true,
            // A requested gameplay restart retains the existing frame's Draw.
            GameplayAction::None | GameplayAction::Restart => {}
        }
    }
}

/// A song's live scene and all state needed to deliver its gameplay lifecycle.
#[derive(Default)]
pub struct SongLuaGameplaySession {
    scene: Option<SongLuaLiveScene>,
    inputs: CallbackInputState,
    repeats: Vec<(InputEvent, Option<PhysicalInput>)>,
    output: SongLuaLiveOutput,
    out: Option<SongLuaOut>,
    ended: bool,
    draw_reload_pending: bool,
}

impl SongLuaGameplaySession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply the common normal-gameplay music and transition timing.
    /// Presentation skins must not select this game policy; low-level scene
    /// assembly can use new or Default without changing the core's timing.
    pub fn for_gameplay<P: GameplayProfileData, O, C, D>(
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Self {
        game.set_music_end_fade_timing(0.25, 5.0);
        Self::new()
    }

    #[inline]
    pub const fn scene(&self) -> Option<&SongLuaLiveScene> {
        self.scene.as_ref()
    }

    #[inline]
    pub const fn scene_mut(&mut self) -> Option<&mut SongLuaLiveScene> {
        self.scene.as_mut()
    }

    #[inline]
    pub const fn take_scene(&mut self) -> Option<SongLuaLiveScene> {
        self.scene.take()
    }

    #[inline]
    pub const fn as_ref(&self) -> Option<&SongLuaLiveScene> {
        self.scene()
    }

    #[inline]
    pub const fn as_mut(&mut self) -> Option<&mut SongLuaLiveScene> {
        self.scene_mut()
    }

    #[inline]
    pub const fn take(&mut self) -> Option<SongLuaLiveScene> {
        self.take_scene()
    }

    #[inline]
    pub const fn is_some(&self) -> bool {
        self.scene.is_some()
    }

    #[inline]
    pub const fn is_none(&self) -> bool {
        self.scene.is_none()
    }

    #[inline]
    pub const fn is_ended(&self) -> bool {
        self.ended
    }

    #[inline]
    pub fn out_transition(&self) -> Option<SongLuaOutView> {
        self.out.as_ref().map(|out| SongLuaOutView {
            phase: out.phase,
            left: out.left,
        })
    }

    /// Transfer authored callbacks from preparation to the live game.
    pub fn attach<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        mut scene: SongLuaLiveScene,
        after_callbacks: impl FnOnce(),
    ) -> Result<(), String>
    where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        scene.initialize_gameplay_clock(game);
        game.publish_song_lua_visuals(&mut scene)?;
        scene
            .prepare_gameplay_player_options(game)
            .map_err(|error| error.to_string())?;
        // The compatibility Player metric is -1, compared as an unsigned value.
        scene
            .install_note_field_tap_callback(game, u32::MAX)
            .map_err(|error| error.to_string())?;
        scene
            .install_tap_row_judgment_callback(game)
            .map_err(|error| error.to_string())?;
        scene
            .install_step_callback(game)
            .map_err(|error| error.to_string())?;
        scene
            .install_hold_judgment_callback(game)
            .map_err(|error| error.to_string())?;
        scene
            .install_avoided_mine_judgment_callback(game)
            .map_err(|error| error.to_string())?;
        scene
            .install_life_changed_callback(game)
            .map_err(|error| error.to_string())?;
        after_callbacks();
        self.output = scene.take_startup_output();
        if !self.output.errors.is_empty() {
            game.mods.song_lua_compile_succeeded = false;
        }
        scene.apply_output_player_writes(&self.output, game);
        self.scene = Some(scene);
        Ok(())
    }

    pub fn end<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), String> {
        if self.ended {
            return Ok(());
        }
        let Some(scene) = &mut self.scene else {
            return Ok(());
        };
        // Final accounting belongs to screen exit, after Player updates have
        // completed the native out transition.
        if game.progress.stage.song_completed_naturally
            && game.current_music_time_ns() >= game.music_end_time_ns()
        {
            game.finalize_completed_mines();
        }
        let output = scene.end().map_err(|error| error.to_string())?;
        scene.apply_output_player_writes(&output, game);
        self.output.append(output);
        self.ended = true;
        game.note_field_tap_callback = None;
        game.tap_row_judgment_callback = None;
        game.hold_judgment_callback = None;
        game.avoided_mine_judgment_callback = None;
        game.life_changed_callback = None;
        game.chart_runtime
            .mine_scan
            .pending_avoided_mine_judgments
            .clear();
        Ok(())
    }

    pub fn take_output<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> SongLuaLiveOutput {
        if !self.output.errors.is_empty() {
            game.mods.song_lua_compile_succeeded = false;
        }
        std::mem::take(&mut self.output)
    }

    #[cfg(feature = "test-support")]
    pub fn draw_frames(&self) -> &[crate::SongLuaDrawFrame] {
        &self.output.draw_frames
    }

    pub fn queue_inputs(
        &mut self,
        events: impl IntoIterator<Item = (InputEvent, Option<PhysicalInput>)>,
    ) {
        if self.scene.is_some() && !self.ended {
            self.inputs.queue(events);
        }
    }

    pub fn reconcile_held_inputs(&mut self, is_down: impl FnMut(&InputEvent) -> bool) {
        self.inputs.reconcile_held(is_down);
    }

    pub fn clear_inputs<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        for (event, physical) in self.inputs.take_releases() {
            self.dispatch_callback(game, &event, physical, SongLuaInputType::Release);
        }
        self.inputs.reset_alias_history();
    }

    pub fn take_held_inputs(&mut self) -> HeldCallbackInputs {
        self.inputs.take_held()
    }

    pub fn restore_held_inputs(&mut self, held: HeldCallbackInputs) {
        self.inputs.restore_held(held);
    }

    pub fn dispatch_input<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        event: &InputEvent,
        physical: Option<PhysicalInput>,
    ) where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        if self.scene.is_none() || self.ended || !self.inputs.accept(*event, physical) {
            return;
        }
        self.dispatch_callback(
            game,
            event,
            physical,
            if event.pressed {
                SongLuaInputType::FirstPress
            } else {
                SongLuaInputType::Release
            },
        );
    }

    fn dispatch_callback<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        event: &InputEvent,
        physical: Option<PhysicalInput>,
        kind: SongLuaInputType,
    ) where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        let (cabinet, button, menu) = match event.action {
            VirtualAction::p1_left => (0, "Left", "MenuLeft"),
            VirtualAction::p1_right => (0, "Right", "MenuRight"),
            VirtualAction::p1_up => (0, "Up", "MenuUp"),
            VirtualAction::p1_down => (0, "Down", "MenuDown"),
            VirtualAction::p2_left => (1, "Left", "MenuLeft"),
            VirtualAction::p2_right => (1, "Right", "MenuRight"),
            VirtualAction::p2_up => (1, "Up", "MenuUp"),
            VirtualAction::p2_down => (1, "Down", "MenuDown"),
            VirtualAction::p1_start => (0, "Start", "Start"),
            VirtualAction::p2_start => (1, "Start", "Start"),
            VirtualAction::p1_select => (0, "Select", "Select"),
            VirtualAction::p2_select => (1, "Select", "Select"),
            VirtualAction::p1_back => (0, "Back", "Back"),
            VirtualAction::p2_back => (1, "Back", "Back"),
            VirtualAction::p1_menu_left => (0, "MenuLeft", "MenuLeft"),
            VirtualAction::p1_menu_right => (0, "MenuRight", "MenuRight"),
            VirtualAction::p1_menu_up => (0, "MenuUp", "MenuUp"),
            VirtualAction::p1_menu_down => (0, "MenuDown", "MenuDown"),
            VirtualAction::p2_menu_left => (1, "MenuLeft", "MenuLeft"),
            VirtualAction::p2_menu_right => (1, "MenuRight", "MenuRight"),
            VirtualAction::p2_menu_up => (1, "MenuUp", "MenuUp"),
            VirtualAction::p2_menu_down => (1, "MenuDown", "MenuDown"),
            VirtualAction::p1_center => (0, "Center", ""),
            VirtualAction::p2_center => (1, "Center", ""),
            _ => return,
        };
        let player = if game.num_players() == 1 {
            let active = usize::from(game.setup.session.player_side == GameplayInputPlayerSide::P2);
            if !game.setup.session.play_style.is_double() && cabinet != active {
                return;
            }
            active
        } else {
            cabinet
        };
        let joystick = event.source == deadsync_core::input::InputSource::Gamepad;
        let (device, device_button) = song_lua_device_names(physical);
        let input = SongLuaInputEvent {
            player: Some(player),
            controller: Some(cabinet),
            button,
            game_button: menu,
            kind,
            device: &device,
            device_button: &device_button,
            level: if event.pressed { 1.0 } else { 0.0 },
            z: 0,
            down: event.pressed,
            ago: event.timestamp.elapsed().as_secs_f64(),
            is_joystick: joystick,
            is_mouse: false,
        };
        self.input(game, &input);
    }

    /// Authored input callbacks do not suppress normal gameplay input.
    pub fn input<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        input: &SongLuaInputEvent<'_>,
    ) where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        if self.ended {
            return;
        }
        let Some(scene) = &mut self.scene else {
            return;
        };
        if let Err(error) = scene.refresh_gameplay_statistics(game) {
            self.output.errors.push(error.to_string());
        }
        match scene.input(input) {
            Ok((_, output)) => {
                // Input follows ScreenManager::Update. Preserve these requests
                // through output drains for the next frame boundary.
                match output.request.as_ref() {
                    Some(SongLuaLiveRequest::NewScreen(screen)) if screen == "ScreenGameplay" => {
                        self.draw_reload_pending = true;
                    }
                    Some(SongLuaLiveRequest::Transition(message))
                        if message == "SM_DoNextScreen" && self.out.is_none() =>
                    {
                        game.progress.stage.song_completed_naturally = true;
                        self.out = Some(SongLuaOut {
                            phase: 0,
                            left: SONG_LUA_OUT_DELAY,
                        });
                    }
                    _ => {}
                }
                scene.apply_output_player_writes(&output, game);
                self.output.append(output);
                if let Err(error) = scene.prepare_gameplay_player_options(game) {
                    self.output.errors.push(error.to_string());
                }
                if let Err(error) = game.publish_song_lua_visuals(scene) {
                    self.output.errors.push(error);
                }
            }
            Err(error) => self.output.errors.push(error.to_string()),
        }
    }

    fn draw<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), String>
    where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        if self.ended {
            return Ok(());
        }
        let Some(scene) = &mut self.scene else {
            return Ok(());
        };
        let custom = scene.has_custom_draw().map_err(|error| error.to_string())?;
        let output = scene.draw().map_err(|error| error.to_string())?;
        if !custom {
            // The native Player draw still publishes NoteField getter state.
            // With no Lua draw callback, the existing visual publication is current.
            self.output.append(output);
            return Ok(());
        }
        scene.apply_output_player_writes(&output, game);
        scene
            .prepare_gameplay_player_options(game)
            .map_err(|error| error.to_string())?;
        match output.request.as_ref() {
            Some(SongLuaLiveRequest::Transition(message))
                if message == "SM_DoNextScreen" && self.out.is_none() =>
            {
                game.progress.stage.song_completed_naturally = true;
                self.out = Some(SongLuaOut {
                    phase: 0,
                    left: SONG_LUA_OUT_DELAY,
                });
            }
            Some(SongLuaLiveRequest::NewScreen(screen)) if screen == "ScreenGameplay" => {
                // Draw occurs after ScreenManager::Update, so loading waits
                // until the next update has completed.
                self.draw_reload_pending = true;
            }
            _ => {}
        }
        self.output.append(output);
        game.publish_song_lua_visuals(scene)
    }

    fn advance<P: GameplayProfileData, O, C, D, S>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        delta: f32,
        uptime_seconds: Option<f64>,
        next_screen_effect: impl FnOnce(Option<&str>) -> Result<SongLuaGameplayEffect<S>, String>,
        emit: &mut impl FnMut(SongLuaGameplayEffect<S>),
        frame_actions: &mut FrameActions,
    ) -> Result<(), String>
    where
        GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
    {
        let Some(scene) = &mut self.scene else {
            return Ok(());
        };
        scene.apply_pending_player_writes(game);
        scene.runtime_mut().publish_gameplay_life_records(game);
        let mut frame = SongLuaLiveFrame::from_gameplay(game, delta);
        frame.uptime_seconds = uptime_seconds;
        let outro_before_actors = frame.gameplay_outro;
        let output = scene.update(frame).map_err(|error| error.to_string())?;
        scene.apply_output_player_writes(&output, game);
        scene
            .prepare_gameplay_player_options(game)
            .map_err(|error| error.to_string())?;
        let gameplay_clock_active = scene.gameplay_clock_active();
        scene.finish_gameplay_audio(game);
        scene
            .finish_gameplay_failures(game, gameplay_clock_active)
            .map_err(|error| error.to_string())?;
        // Lua may drain life after the core's earlier failure check.
        if !outro_before_actors && game.finish_gameplay_if_ready(false).is_some() {
            scene.note_gameplay_outro_after_actors();
        }
        scene
            .finish_gameplay_clock(game, gameplay_clock_active)
            .map_err(|error| error.to_string())?;
        let request = output.request.as_ref().or(self.output.request.as_ref());
        if matches!(request, Some(SongLuaLiveRequest::Transition(message)) if message == "SM_DoNextScreen")
            && self.out.is_none()
        {
            game.progress.stage.song_completed_naturally = true;
            self.out = Some(SongLuaOut {
                phase: 0,
                left: SONG_LUA_OUT_DELAY,
            });
        }
        if self.draw_reload_pending
            || matches!(request, Some(SongLuaLiveRequest::NewScreen(screen)) if screen == "ScreenGameplay")
        {
            emit(SongLuaGameplayEffect::ReloadSongLua);
            frame_actions.screen_changes = true;
            self.draw_reload_pending = false;
        }
        // Startup events precede the first update and reach the shell once.
        self.output.append(output);
        game.publish_song_lua_visuals(scene)?;
        let failed_out = scene.outro_off_dispatched()
            && scene.failed_outro()
            && self.out.as_ref().is_some_and(|out| out.phase != 2);
        if scene.outro_off_dispatched() && (self.out.is_none() || failed_out) {
            // A newly started transition receives its first delta next frame.
            self.out = Some(if scene.failed_outro() {
                // The fallback failed actor is blank. Completion is dispatched
                // on its following update.
                SongLuaOut {
                    phase: 2,
                    left: 0.0,
                }
            } else {
                SongLuaOut {
                    phase: 0,
                    left: SONG_LUA_OUT_DELAY,
                }
            });
        } else if self.out.as_mut().is_some_and(|out| out.advance(delta)) {
            let target = scene
                .next_screen_name()
                .map_err(|error| error.to_string())?;
            let reload = target.as_deref() == Some("ScreenGameplay");
            let effect = if reload {
                SongLuaGameplayEffect::ReloadSongLua
            } else {
                next_screen_effect(target.as_deref())?
            };
            self.end(game)?;
            emit(effect);
            frame_actions.pop |= !reload;
            frame_actions.screen_changes = true;
        }
        Ok(())
    }
}

/// Advance a frame and convert its output using the host's effect adapter.
pub fn update<H: SongLuaGameplayHost>(
    host: &mut H,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    effects: &mut Vec<H::Effect>,
    after_update_input: impl FnOnce(&mut H),
) where
    HostGame<H>: SongLuaGameplayVisualState,
{
    update_with_effects(
        host,
        delta_time,
        audio_snapshot,
        fallback_host_nanos,
        H::next_screen_effect,
        |effect| {
            effects.push(match effect {
                SongLuaGameplayEffect::Action(action) => H::gameplay_effect(action),
                SongLuaGameplayEffect::ReloadSongLua => H::reload_song_lua_effect(),
                SongLuaGameplayEffect::Navigate(effect) => effect,
            });
        },
        after_update_input,
    );
}

/// Compatibility frame entry point that retains the caller's Lua destination
/// adapter. The direct stage API resolves standard gameplay exits itself.
pub fn update_with_effects<H: SongLuaGameplayHost, S>(
    host: &mut H,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    resolve_screen: impl FnOnce(Option<&str>) -> Result<S, String>,
    emit: impl FnMut(SongLuaGameplayEffect<S>),
    after_update_input: impl FnOnce(&mut H),
) where
    HostGame<H>: SongLuaGameplayVisualState,
{
    drive_frame(
        host,
        H::gameplay_parts,
        delta_time,
        audio_snapshot,
        fallback_host_nanos,
        |name| resolve_screen(name).map(SongLuaGameplayEffect::Navigate),
        emit,
        H::update_gameplay_visuals,
        after_update_input,
    );
}

fn drive_frame<H, P: GameplayProfileData, O, C, D, S>(
    host: &mut H,
    gameplay_parts: impl Fn(
        &mut H,
    ) -> (
        &mut GameplayRuntimeState<P, O, C, D>,
        &mut SongLuaGameplaySession,
    ),
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    next_screen_effect: impl FnOnce(Option<&str>) -> Result<SongLuaGameplayEffect<S>, String>,
    mut emit: impl FnMut(SongLuaGameplayEffect<S>),
    update_visuals: impl FnOnce(&mut H, f32),
    after_update_input: impl FnOnce(&mut H),
) where
    GameplayRuntimeState<P, O, C, D>: SongLuaGameplayVisualState,
{
    let mut frame_actions = FrameActions::default();
    let action = {
        let (game, session) = gameplay_parts(host);
        if session.ended {
            return;
        }
        if let Some(scene) = &session.scene
            && let Err(error) = scene.prepare_gameplay_player_options(game)
        {
            session.output.errors.push(error.to_string());
        }
        let host_nanos = audio_snapshot.stream_clock.valid_at_host_nanos;
        let uptime_seconds = if session.scene.is_some() && host_nanos != 0 {
            Some(host_nanos as f64 / 1_000_000_000.0)
        } else {
            None
        };
        let action = if let Some(scene) = &mut session.scene {
            let errors = &mut session.output.errors;
            update_core_with_pre_player(
                game,
                delta_time,
                audio_snapshot,
                fallback_host_nanos,
                |game| {
                    let mut frame = SongLuaLiveFrame::from_gameplay(game, delta_time);
                    frame.uptime_seconds = uptime_seconds;
                    if let Err(error) = scene.prepare_gameplay_frame(frame) {
                        errors.push(error.to_string());
                    }
                    scene.apply_pending_player_writes(game);
                    if let Err(error) = scene.prepare_gameplay_player_options(game) {
                        errors.push(error.to_string());
                    }
                },
            )
        } else {
            update_core(game, delta_time, audio_snapshot, fallback_host_nanos)
        };
        if session.scene.is_none()
            && matches!(
                action,
                GameplayAction::Navigate(GameplayExit::Complete)
                    | GameplayAction::NavigateNoFade(GameplayExit::Complete)
            )
        {
            game.finalize_completed_stage();
        }
        if let Err(error) = session.advance(
            game,
            delta_time,
            uptime_seconds,
            next_screen_effect,
            &mut emit,
            &mut frame_actions,
        ) {
            session.output.errors.push(error);
        }
        action
    };

    update_visuals(host, delta_time);
    let has_scene = {
        let (game, session) = gameplay_parts(host);
        let resolved_action = game.resolve_frame_action(action);
        match action {
            GameplayAction::Navigate(GameplayExit::Complete)
            | GameplayAction::NavigateNoFade(GameplayExit::Complete)
                if session.out.is_some()
                    || (session.scene.is_some() && game.mods.attacks.cleared_for_outro) =>
            {
                // Repeated core completion requests keep the owned Lua outro's
                // original clock. Suppression uses the original core action.
            }
            GameplayAction::None if resolved_action == GameplayAction::None => {}
            _ => {
                emit(SongLuaGameplayEffect::Action(resolved_action));
                frame_actions.record_gameplay_action(resolved_action);
            }
        }
        session.scene.is_some()
    };
    if !has_scene {
        after_update_input(host);
        return;
    }

    {
        let (game, session) = gameplay_parts(host);
        let mut pending = session.inputs.take_pending();
        for (event, physical) in pending.drain(..) {
            session.dispatch_input(game, &event, physical);
        }
        session.inputs.restore_pending(pending);

        let mut repeats = std::mem::take(&mut session.repeats);
        session.inputs.append_repeats(delta_time, &mut repeats);
        for (event, physical) in repeats.drain(..) {
            session.dispatch_callback(game, &event, physical, SongLuaInputType::Repeat);
        }
        session.repeats = repeats;
    }
    after_update_input(host);

    let (game, session) = gameplay_parts(host);
    if frame_actions.pop
        && let Err(error) = session.end(game)
    {
        session.output.errors.push(error);
    }
    if !frame_actions.screen_changes
        && let Err(error) = session.draw(game)
    {
        session.output.errors.push(error);
    }
}

/// Deliver normal gameplay input, then queue the same edge for authored input.
///
/// Lobby readiness and other application gates run before this function.
pub fn handle_input<H: SongLuaGameplayHost>(
    host: &mut H,
    event: &InputEvent,
    physical: Option<PhysicalInput>,
    effects: &mut Vec<H::Effect>,
) {
    let (game, session) = host.gameplay_parts();
    if let Some(effect) =
        handle_input_with_action(game, session, event, physical, H::gameplay_effect)
    {
        effects.push(effect);
    }
}

/// Deliver the input edge and return the action generated by the game.
#[must_use]
pub fn handle_input_action<H: SongLuaGameplayHost>(
    host: &mut H,
    event: &InputEvent,
    physical: Option<PhysicalInput>,
) -> GameplayAction {
    let (game, session) = host.gameplay_parts();
    handle_input_with_action(game, session, event, physical, std::convert::identity)
        .unwrap_or(GameplayAction::None)
}

fn handle_input_with_action<P: GameplayProfileData, O, C, D, T>(
    game: &mut GameplayRuntimeState<P, O, C, D>,
    session: &mut SongLuaGameplaySession,
    event: &InputEvent,
    physical: Option<PhysicalInput>,
    map_action: impl FnOnce(GameplayAction) -> T,
) -> Option<T> {
    let action = handle_core_input(game, event);
    session.queue_inputs(std::iter::once((*event, physical)));
    if matches!(
        action,
        GameplayAction::Navigate(GameplayExit::Complete)
            | GameplayAction::NavigateNoFade(GameplayExit::Complete)
    ) {
        game.mark_autoplay_stage_failures();
    }
    // Input's former effect append skips its no-op action.
    if action == GameplayAction::None {
        return None;
    }
    let effect = map_action(action);
    if matches!(action, GameplayAction::NavigateNoFade(_))
        && let Err(error) = session.end(game)
    {
        session.output.errors.push(error);
    }
    Some(effect)
}

fn song_lua_device_names(physical: Option<PhysicalInput>) -> (String, String) {
    use deadlib_platform::input::{KeyCode, PadDir};
    let (device, button) = match physical {
        Some(PhysicalInput::Key(key)) => {
            let name = match key {
                KeyCode::ArrowLeft => "left".into(),
                KeyCode::ArrowRight => "right".into(),
                KeyCode::ArrowUp => "up".into(),
                KeyCode::ArrowDown => "down".into(),
                KeyCode::Enter => "enter".into(),
                KeyCode::Escape => "escape".into(),
                KeyCode::Space => "space".into(),
                KeyCode::Tab => "tab".into(),
                KeyCode::Backspace => "backspace".into(),
                KeyCode::ControlLeft => "left ctrl".into(),
                KeyCode::ControlRight => "right ctrl".into(),
                KeyCode::ShiftLeft => "left shift".into(),
                KeyCode::ShiftRight => "right shift".into(),
                KeyCode::AltLeft => "left alt".into(),
                KeyCode::AltRight => "right alt".into(),
                KeyCode::SuperLeft => "left super".into(),
                KeyCode::SuperRight => "right super".into(),
                KeyCode::PageUp => "pgup".into(),
                KeyCode::PageDown => "pgdn".into(),
                KeyCode::Home => "home".into(),
                KeyCode::End => "end".into(),
                KeyCode::Insert => "insert".into(),
                KeyCode::Delete => "delete".into(),
                KeyCode::Comma => "comma".into(),
                KeyCode::Period => "period".into(),
                KeyCode::Backslash => "backslash".into(),
                KeyCode::Minus => "-".into(),
                KeyCode::Equal => "=".into(),
                KeyCode::BracketLeft => "[".into(),
                KeyCode::BracketRight => "]".into(),
                KeyCode::Semicolon => ";".into(),
                KeyCode::Quote => "'".into(),
                KeyCode::Backquote => "\u{0060}".into(),
                KeyCode::NumpadEnter => "KP enter".into(),
                KeyCode::NumpadAdd => "KP +".into(),
                KeyCode::NumpadSubtract => "KP -".into(),
                KeyCode::NumpadMultiply => "KP *".into(),
                KeyCode::NumpadDivide => "KP /".into(),
                KeyCode::NumpadDecimal => "KP .".into(),
                _ => {
                    let key = format!("{key:?}");
                    if let Some(key) = key
                        .strip_prefix("Key")
                        .or_else(|| key.strip_prefix("Digit"))
                    {
                        key.to_ascii_lowercase()
                    } else if key
                        .strip_prefix('F')
                        .and_then(|n| n.parse::<u8>().ok())
                        .is_some_and(|n| (1..=16).contains(&n))
                    {
                        key
                    } else if let Some(n) = key
                        .strip_prefix("Numpad")
                        .filter(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
                    {
                        format!("KP {n}")
                    } else {
                        "Invalid".into()
                    }
                }
            };
            ("InputDevice_Key".into(), name)
        }
        Some(PhysicalInput::PadDirection(id, dir)) => (
            format!("InputDevice_Joy{}", id.0 + 1),
            match dir {
                PadDir::Left => "Left1",
                PadDir::Right => "Right1",
                PadDir::Up => "Up1",
                PadDir::Down => "Down1",
            }
            .into(),
        ),
        Some(PhysicalInput::PadButton(id, code)) => (
            format!("InputDevice_Joy{}", id.0 + 1),
            if code.into_u32() < 32 {
                format!("B{}", code.into_u32() + 1)
            } else {
                "Invalid".into()
            },
        ),
        None => return ("InputDevice_Invalid".into(), "DeviceButton_Invalid".into()),
    };
    (device, format!("DeviceButton_{button}"))
}
