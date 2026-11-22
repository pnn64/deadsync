use super::{MusicStartSnap, PracticePlayback, PracticePlaybackInput, PracticePlaybackUpdate};
use crate::practice::editor::{
    PracticeEditor, PracticeEditorCommand, PracticeEditorConfig, PracticeEditorFeedback,
    PracticeEditorRequest, PracticeEditorSnapshot,
};
use crate::{GameplayAudioSnapshot, GameplayProfileData, GameplayRuntimeState};
use deadsync_input::{InputEvent, practice::PracticeKeyEvent};

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
        Self {
            editor,
            playback,
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
                PracticeViewRequest::PlaybackStopped
            }
        }
    }
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
