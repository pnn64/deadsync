use super::{MusicStartSnap, PracticePlayback, PracticePlaybackInput, PracticePlaybackUpdate};
use crate::practice::editor::{
    PracticeEditor, PracticeEditorCommand, PracticeEditorConfig, PracticeEditorFeedback,
    PracticeEditorRequest, PracticeEditorSnapshot,
};
use crate::{GameplayAudioSnapshot, GameplayProfileData, GameplayRuntimeState};
use deadsync_input::{InputEvent, practice::PracticeKeyEvent};

const DISPLAY_BEAT_EPSILON: f32 = 0.000_1;
const DISPLAY_SCROLL_TIME_CONSTANT: f32 = 0.07;
const DISPLAY_SCROLL_SNAP_EPSILON: f32 = 0.001;
const DISPLAY_SCROLL_MAX_SMOOTH_BEATS: f32 = 64.0;

/// Presentation work requested after the practice controller handles input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeViewRequest {
    None,
    OpenMainMenu,
    OpenHelpMenu,
    CloseMenu,
    ActivateMenu,
    StepMenu(isize),
    ChangeEditScrollSpeed(isize),
    PlaybackStarted,
    PlaybackStopped,
}

/// Owns the handoff between practice editing and chart playback.
#[derive(Debug)]
pub struct PracticeSession {
    editor: PracticeEditor,
    playback: PracticePlayback,
    display_beat: f32,
    pending_music_rates: Vec<f32>,
}

type Game<P, O, C, D> = GameplayRuntimeState<P, O, C, D>;

impl PracticeSession {
    #[must_use]
    pub fn new<P: GameplayProfileData, O, C, D>(
        game: &mut Game<P, O, C, D>,
        config: PracticeEditorConfig,
    ) -> Self {
        let playback = PracticePlayback::new(game);
        let mut editor = PracticeEditor::new(config);
        editor.set_cursor(game, editor.cursor_beat());
        let display_beat = editor.cursor_beat();
        Self {
            editor,
            playback,
            display_beat,
            pending_music_rates: Vec::with_capacity(4),
        }
    }

    #[must_use]
    pub const fn editor(&self) -> &PracticeEditor {
        &self.editor
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playback.is_playing()
    }

    pub const fn clear_navigation_holds(&mut self) {
        self.editor.clear_navigation_holds();
    }

    /// The application updates its session rate after draining stage audio.
    pub fn drain_music_rates(&mut self) -> std::vec::Drain<'_, f32> {
        self.pending_music_rates.drain(..)
    }

    pub fn restore<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        snapshot: PracticeEditorSnapshot,
    ) {
        self.playback.restore_editing();
        self.editor.restore(game, snapshot);
    }

    pub fn on_enter<P: GameplayProfileData, O, C, D>(&mut self, game: &mut Game<P, O, C, D>) {
        self.playback.on_enter(game);
        self.editor.set_cursor(game, self.editor.cursor_beat());
        self.snap_display_to_cursor(game);
    }

    /// Immediately align the stage clock with the edit cursor after setup or restore.
    pub fn snap_display_to_cursor<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
    ) {
        self.seek_display(game, self.editor.cursor_beat());
    }

    fn seek_display<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        beat: f32,
    ) {
        self.display_beat = beat;
        let music_time = game.music_time_for_beat(beat);
        game.seek_practice_display(music_time);
    }

    fn update_display_scroll<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        delta_time: f32,
    ) {
        let cursor = self.editor.cursor_beat();
        if delta_time <= 0.0 || (self.display_beat - cursor).abs() <= DISPLAY_BEAT_EPSILON {
            return;
        }
        let beat = next_display_beat(self.display_beat, cursor, delta_time);
        let beat = if (beat - cursor).abs() <= DISPLAY_BEAT_EPSILON {
            cursor
        } else {
            beat
        };
        self.seek_display(game, beat);
    }

    pub fn update<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        delta_time: f32,
        menu_open: bool,
        audio_snapshot: GameplayAudioSnapshot,
        fallback_host_nanos: impl FnOnce() -> u64,
        snap_music_start: MusicStartSnap,
        feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticePlaybackUpdate {
        let mut feedback = capture_music_rates(&mut self.pending_music_rates, feedback);
        self.editor
            .update_music_rate_hold(game, delta_time, &mut feedback);
        if !self.playback.is_playing() {
            game.advance_screen_elapsed(delta_time);
            self.editor
                .update_navigation_holds(game, delta_time, menu_open, feedback);
            self.update_display_scroll(game, delta_time);
            return PracticePlaybackUpdate::Editing;
        }

        self.playback.update(
            game,
            delta_time,
            audio_snapshot,
            fallback_host_nanos,
            snap_music_start,
            || self.editor.clear_navigation_holds(),
        )
    }

    pub fn handle_input<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        event: &InputEvent,
        menu_open: bool,
        snap_music_start: MusicStartSnap,
        feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticeViewRequest {
        match self
            .playback
            .handle_input(game, event, || self.editor.clear_navigation_holds())
        {
            PracticePlaybackInput::Editing => {}
            PracticePlaybackInput::Handled => return PracticeViewRequest::None,
            PracticePlaybackInput::Stopped { cursor_beat } => {
                self.editor.set_cursor(game, cursor_beat);
                self.snap_display_to_cursor(game);
                return PracticeViewRequest::PlaybackStopped;
            }
        }
        let request = self.editor.handle_input(
            game,
            event,
            menu_open,
            capture_music_rates(&mut self.pending_music_rates, feedback),
        );
        self.apply_editor_request(game, request, snap_music_start)
    }

    pub fn handle_raw_key<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        event: &PracticeKeyEvent,
        menu_open: bool,
        snap_music_start: MusicStartSnap,
        feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> (bool, PracticeViewRequest) {
        let result = self.editor.handle_raw_key(
            game,
            event,
            self.playback.is_playing(),
            menu_open,
            capture_music_rates(&mut self.pending_music_rates, feedback),
        );
        (
            result.consumed,
            self.apply_editor_request(game, result.request, snap_music_start),
        )
    }

    pub fn execute<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        command: PracticeEditorCommand,
        snap_music_start: MusicStartSnap,
        feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticeViewRequest {
        let request = self.editor.execute(
            game,
            command,
            capture_music_rates(&mut self.pending_music_rates, feedback),
        );
        self.apply_editor_request(game, request, snap_music_start)
    }

    fn apply_editor_request<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        request: PracticeEditorRequest,
        snap_music_start: MusicStartSnap,
    ) -> PracticeViewRequest {
        match request {
            PracticeEditorRequest::None => PracticeViewRequest::None,
            PracticeEditorRequest::OpenMainMenu => {
                self.clear_navigation_holds();
                PracticeViewRequest::OpenMainMenu
            }
            PracticeEditorRequest::OpenHelpMenu => {
                self.clear_navigation_holds();
                PracticeViewRequest::OpenHelpMenu
            }
            PracticeEditorRequest::CloseMenu => PracticeViewRequest::CloseMenu,
            PracticeEditorRequest::ActivateMenu => PracticeViewRequest::ActivateMenu,
            PracticeEditorRequest::StepMenu(delta) => PracticeViewRequest::StepMenu(delta),
            PracticeEditorRequest::ChangeEditScrollSpeed(delta) => {
                PracticeViewRequest::ChangeEditScrollSpeed(delta)
            }
            PracticeEditorRequest::StartPlayback {
                start_beat,
                stop_beat,
            } => {
                self.clear_navigation_holds();
                self.playback
                    .start(game, start_beat, stop_beat, snap_music_start);
                PracticeViewRequest::PlaybackStarted
            }
            PracticeEditorRequest::StopPlayback => {
                self.clear_navigation_holds();
                let cursor = self.playback.stop(game);
                self.editor.set_cursor(game, cursor);
                self.snap_display_to_cursor(game);
                PracticeViewRequest::PlaybackStopped
            }
        }
    }
}

/// Ease toward nearby cursors, or snap tiny gaps and long seeks.
fn next_display_beat(display_beat: f32, cursor_beat: f32, delta_time: f32) -> f32 {
    let diff = cursor_beat - display_beat;
    if diff.abs() <= DISPLAY_SCROLL_SNAP_EPSILON || diff.abs() > DISPLAY_SCROLL_MAX_SMOOTH_BEATS {
        return cursor_beat;
    }
    let t = 1.0 - (-delta_time / DISPLAY_SCROLL_TIME_CONSTANT).exp();
    diff.mul_add(t, display_beat)
}

fn capture_music_rates<'a, P: GameplayProfileData, O, C, D>(
    rates: &'a mut Vec<f32>,
    mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback) + 'a,
) -> impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback) + 'a {
    move |game, event| match event {
        PracticeEditorFeedback::PersistMusicRate(rate) => rates.push(rate),
        event => feedback(game, event),
    }
}

#[cfg(test)]
mod tests {
    use super::{DISPLAY_SCROLL_MAX_SMOOTH_BEATS, DISPLAY_SCROLL_SNAP_EPSILON, next_display_beat};

    #[test]
    fn practice_display_scroll_settles_and_snaps() {
        assert_eq!(next_display_beat(10.0, 10.0 + 0.0005, 0.016), 10.0 + 0.0005);
        let far = 10.0 + DISPLAY_SCROLL_MAX_SMOOTH_BEATS + 1.0;
        assert_eq!(next_display_beat(10.0, far, 0.016), far);
    }

    #[test]
    fn practice_display_scroll_eases_toward_cursor() {
        let target = 4.0;
        let mut beat = next_display_beat(0.0, target, 0.016);
        assert!(beat > 0.0 && beat < target);
        for _ in 0..240 {
            beat = next_display_beat(beat, target, 0.016);
        }
        assert!((beat - target).abs() <= DISPLAY_SCROLL_SNAP_EPSILON);
    }
}
