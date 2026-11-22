//! Game-owned ordering for live song Lua, gameplay updates, input and screen exit.
//!
//! The host supplies presentation work and converts semantic actions to its
//! screen effects. This module owns when those operations run.

use crate::{
    SongLuaInputEvent, SongLuaInputType, SongLuaLiveFrame, SongLuaLiveOutput, SongLuaLiveRequest,
    SongLuaLiveScene,
};
use deadsync_gameplay::{
    GameplayAction, GameplayAudioSnapshot, GameplayExit, GameplayInputPlayerSide,
    GameplayProfileData, GameplayRuntimeState, handle_core_input, update_core,
    update_core_with_pre_player,
};
use deadsync_input::{
    CallbackInputState, HeldCallbackInputs, InputEvent, PhysicalInput, VirtualAction,
};

pub const SONG_LUA_OUT_DELAY: f32 = 0.5;
pub const SONG_LUA_OUT_FADE_DURATION: f32 = 1.0;
pub const GAMEPLAY_EXIT_TRANSITION_SECONDS: f32 = SONG_LUA_OUT_DELAY + SONG_LUA_OUT_FADE_DURATION;

/// Presentation-only snapshot of the native gameplay exit clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaOutView {
    pub phase: u8,
    pub left: f32,
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

    fn advance<H: SongLuaGameplayHost>(
        &mut self,
        game: &mut HostGame<H>,
        delta: f32,
        uptime_seconds: Option<f64>,
        effects: &mut Vec<H::Effect>,
        frame_actions: &mut FrameActions,
    ) -> Result<(), String>
    where
        HostGame<H>: SongLuaGameplayVisualState,
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
            effects.push(H::reload_song_lua_effect());
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
                H::reload_song_lua_effect()
            } else {
                H::next_screen_effect(target.as_deref())?
            };
            self.end(game)?;
            effects.push(effect);
            frame_actions.pop |= !reload;
            frame_actions.screen_changes = true;
        }
        Ok(())
    }
}

/// Advance one complete game frame, including authored Update/input/Draw phases.
///
/// Host borrows end before each presentation or frame-input hook, allowing an
/// existing owner to receive its complete state without allocating a closure or
/// using dynamic dispatch.
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
    let mut frame_actions = FrameActions::default();
    let action = {
        let (game, session) = host.gameplay_parts();
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
        if let Err(error) = session.advance::<H>(
            game,
            delta_time,
            uptime_seconds,
            effects,
            &mut frame_actions,
        ) {
            session.output.errors.push(error);
        }
        action
    };

    host.update_gameplay_visuals(delta_time);
    let has_scene = {
        let (game, session) = host.gameplay_parts();
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
                effects.push(H::gameplay_effect(resolved_action));
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
        let (game, session) = host.gameplay_parts();
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

    let (game, session) = host.gameplay_parts();
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
        return;
    }
    let effect = H::gameplay_effect(action);
    if matches!(action, GameplayAction::NavigateNoFade(_))
        && let Err(error) = session.end(game)
    {
        session.output.errors.push(error);
    }
    effects.push(effect);
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
