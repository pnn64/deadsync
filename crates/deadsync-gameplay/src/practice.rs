//! Canonical practice playback transitions and looping.
//!
//! Editor navigation and presentation are separate from this controller. In
//! particular, practice advances the core directly rather than running a normal
//! song-Lua gameplay session.

pub mod editor;
mod session;

pub use session::{
    PracticeDestination, PracticeMainMenuItem, PracticeMenu, PracticeMenuKind, PracticeSession,
    PracticeViewRequest,
};

/// A practice session and its prepared stage with a replaceable presenter.
pub struct PracticeHost<G, P> {
    pub gameplay: G,
    pub session: PracticeSession,
    pub presentation: P,
}

use crate::{
    GameplayAction, GameplayAudioCommand, GameplayAudioSnapshot, GameplayProfileData,
    GameplayRuntimeState, handle_core_input, update_core,
};
use deadsync_input::{InputEvent, VirtualAction};
use std::path::Path;

const LEAD_IN_SECONDS: f32 = 1.0;
const LOOP_AFTER_SECONDS: f32 = 1.0;
const MIN_CURSOR_BEAT: f32 = 0.0;

pub type MusicStartSnap = fn(&Path, f64) -> f64;

#[derive(Clone, Copy, Debug)]
struct PlaybackRange {
    start_beat: f32,
    stop_beat: f32,
}

/// The result of advancing practice playback, for presentation updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PracticePlaybackUpdate {
    Editing,
    Playing,
    Restarted,
}

/// The result of routing an input through practice playback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticePlaybackInput {
    Editing,
    Handled,
    Stopped { cursor_beat: f32 },
}

/// Game-owned practice mode, loop range and playback policy.
#[derive(Debug)]
pub struct PracticePlayback {
    range: Option<PlaybackRange>,
}

impl PracticePlayback {
    /// Begin editing with score saving and replay playback disabled.
    #[must_use]
    pub fn new<P: GameplayProfileData, O, C, D>(
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Self {
        game.disable_score_for_practice();
        Self { range: None }
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.range.is_some()
    }

    /// Restore the mode when an editor snapshot is applied to prepared gameplay.
    /// This does not queue audio or reset the newly prepared gameplay state.
    pub const fn restore_editing(&mut self) {
        self.range = None;
    }

    /// Stop any previous music when the practice screen is entered.
    pub fn on_enter<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) {
        game.push_audio_command(GameplayAudioCommand::StopMusic);
    }

    /// Start a range with snapped audio and the original judgment start time.
    pub fn start<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        start_beat: f32,
        stop_beat: f32,
        snap_music_start: MusicStartSnap,
    ) {
        let start_time = game.music_time_for_beat(start_beat);
        let playback_time = start_time - LEAD_IN_SECONDS;
        let playback_time = game.charts()[0]
            .music_path
            .as_ref()
            .map_or(playback_time, |music_path| {
                snap_music_start(music_path, f64::from(playback_time)) as f32
            });
        game.start_practice_music_at(playback_time, start_time);
        self.range = Some(PlaybackRange {
            start_beat,
            stop_beat,
        });
    }

    /// Return to editing and reset judgments before the edit notefield is shown.
    /// The returned beat lets the presentation restore its cursor and display.
    pub fn stop<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> f32 {
        game.push_audio_command(GameplayAudioCommand::StopMusic);
        let current_beat = game.current_beat().max(MIN_CURSOR_BEAT);
        let current_time = game.music_time_for_beat(current_beat);
        game.reset_practice_playback(current_time);
        self.range = None;
        current_beat
    }

    /// Advance playback and restart after the range or any core exit action.
    ///
    /// The preparation hook runs only after the game has decided to restart. It
    /// clears editor navigation holds before the core reset, retaining the
    /// existing input cleanup order without delegating the loop decision.
    pub fn update<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        delta_time: f32,
        audio_snapshot: GameplayAudioSnapshot,
        fallback_host_nanos: impl FnOnce() -> u64,
        snap_music_start: MusicStartSnap,
        before_restart: impl FnOnce(),
    ) -> PracticePlaybackUpdate {
        let Some(range) = self.range else {
            return PracticePlaybackUpdate::Editing;
        };
        let action = update_core(game, delta_time, audio_snapshot, fallback_host_nanos);
        let current_time = game.current_music_time_seconds();
        let stop_time = game.music_time_for_beat(range.stop_beat);
        if current_time >= stop_time + LOOP_AFTER_SECONDS || !matches!(action, GameplayAction::None)
        {
            before_restart();
            self.start(game, range.start_beat, range.stop_beat, snap_music_start);
            PracticePlaybackUpdate::Restarted
        } else {
            PracticePlaybackUpdate::Playing
        }
    }

    /// Route playing inputs, including immediate Back-to-editor behavior.
    ///
    /// The preparation hook can clear editor holds before stopping; it cannot
    /// change which inputs stop playback or how the core handles other inputs.
    pub fn handle_input<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        event: &InputEvent,
        before_stop: impl FnOnce(),
    ) -> PracticePlaybackInput {
        if !self.is_playing() {
            return PracticePlaybackInput::Editing;
        }
        if !event.pressed {
            if event.action.is_gameplay_arrow() {
                let _ = handle_core_input(game, event);
            }
            return PracticePlaybackInput::Handled;
        }
        if matches!(
            event.action,
            VirtualAction::p1_back | VirtualAction::p2_back
        ) {
            before_stop();
            return PracticePlaybackInput::Stopped {
                cursor_beat: self.stop(game),
            };
        }
        let _ = handle_core_input(game, event);
        PracticePlaybackInput::Handled
    }
}
