use std::{path::PathBuf, time::Instant};

use deadsync_core::input::MAX_PLAYERS;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use deadsync_score::stage_stats;

use crate::CourseRunState;

/// Session-wide values that survive screen swaps.
pub struct SessionState {
    pub preferred_difficulty_index: usize,
    pub session_start_time: Option<Instant>,
    pub menu_start_side: PlayerSide,
    pub played_stages: Vec<stage_stats::StageSummary>,
    pub pending_post_select_summary_exit: bool,
    pub course_individual_stage_indices: Vec<usize>,
    pub combo_carry: [u32; MAX_PLAYERS],
    pub gameplay_restart_count: u32,
    pub restart_pending: bool,
    /// Preferred rate to restore after Practice and its editor options close.
    pub pre_practice_music_rate: Option<f32>,
    pub course_run: Option<CourseRunState>,
    pub last_course_wheel_path: Option<PathBuf>,
    pub last_course_wheel_difficulty_name: Option<String>,
}

impl SessionState {
    pub const fn new(preferred_difficulty_index: usize, combo_carry: [u32; MAX_PLAYERS]) -> Self {
        Self {
            preferred_difficulty_index,
            session_start_time: None,
            menu_start_side: PlayerSide::P1,
            played_stages: Vec::new(),
            pending_post_select_summary_exit: false,
            course_individual_stage_indices: Vec::new(),
            combo_carry,
            gameplay_restart_count: 0,
            restart_pending: false,
            pre_practice_music_rate: None,
            course_run: None,
            last_course_wheel_path: None,
            last_course_wheel_difficulty_name: None,
        }
    }

    /// Starts a session once and clears state accumulated by a prior session.
    pub fn begin_play_session(&mut self, now: Instant) -> bool {
        if self.session_start_time.is_some() {
            return false;
        }
        self.session_start_time = Some(now);
        self.played_stages.clear();
        self.course_individual_stage_indices.clear();
        true
    }

    pub fn clear_course_runtime(&mut self) {
        self.course_run = None;
    }

    pub const fn remember_menu_start(&mut self, input: &InputEvent) {
        if input.pressed {
            match input.action {
                VirtualAction::p1_start => self.menu_start_side = PlayerSide::P1,
                VirtualAction::p2_start => self.menu_start_side = PlayerSide::P2,
                _ => {}
            }
        }
    }

    /// Record authoritative data independently of any optional evaluation page.
    pub fn record_stage_result(&mut self, stage: stage_stats::StageSummary) {
        if let Some(course) = self.course_run.as_mut() {
            self.course_individual_stage_indices
                .push(self.played_stages.len());
            course.stage_summaries.push(stage.clone());
        }
        self.played_stages.push(stage);
    }

    pub fn take_final_course(&mut self, failed: bool) -> Option<CourseRunState> {
        let course = self.course_run.as_ref()?;
        if !course.run.is_finished(failed) {
            return None;
        }
        self.course_run.take()
    }

    pub fn reset_for_menu(&mut self, combo_carry: [u32; MAX_PLAYERS]) {
        self.session_start_time = None;
        self.menu_start_side = PlayerSide::P1;
        self.played_stages.clear();
        self.course_individual_stage_indices.clear();
        self.combo_carry = combo_carry;
        self.pre_practice_music_rate = None;
        self.clear_course_runtime();
        self.last_course_wheel_path = None;
        self.last_course_wheel_difficulty_name = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_preserves_preferences_and_starts_empty() {
        let state = SessionState::new(3, [12, 34]);
        assert_eq!(state.preferred_difficulty_index, 3);
        assert_eq!(state.combo_carry, [12, 34]);
        assert!(state.session_start_time.is_none());
        assert!(state.played_stages.is_empty());
        assert_eq!(state.gameplay_restart_count, 0);
        assert!(state.pre_practice_music_rate.is_none());
    }

    #[test]
    fn play_session_starts_once_and_clears_prior_lists() {
        let mut state = SessionState::new(0, [0; MAX_PLAYERS]);
        state.course_individual_stage_indices.extend([1, 2]);
        let started = Instant::now();
        assert!(state.begin_play_session(started));
        assert_eq!(state.session_start_time, Some(started));
        assert!(state.course_individual_stage_indices.is_empty());
        assert!(!state.begin_play_session(started));
    }

    #[test]
    fn menu_start_side_follows_the_pressed_start_and_resets_on_entry() {
        use deadsync_core::input::InputSource;

        let mut state = SessionState::new(0, [0; MAX_PLAYERS]);
        let now = Instant::now();
        let input = |action, pressed| {
            InputEvent::new(action, 0, pressed, InputSource::Keyboard, now, 0, now, now)
        };
        assert_eq!(state.menu_start_side, PlayerSide::P1);
        state.remember_menu_start(&input(VirtualAction::p2_start, true));
        state.remember_menu_start(&input(VirtualAction::p1_start, false));
        state.remember_menu_start(&input(VirtualAction::p1_menu_up, true));
        assert_eq!(state.menu_start_side, PlayerSide::P2);
        state.reset_for_menu([0; MAX_PLAYERS]);
        assert_eq!(state.menu_start_side, PlayerSide::P1);
    }

    #[test]
    fn menu_reset_clears_session_and_course_navigation() {
        let mut state = SessionState::new(4, [1, 2]);
        state.session_start_time = Some(Instant::now());
        state.pre_practice_music_rate = Some(1.25);
        state.last_course_wheel_path = Some(PathBuf::from("course.crs"));
        state.last_course_wheel_difficulty_name = Some("Hard".to_string());
        state.reset_for_menu([8, 13]);
        assert!(state.session_start_time.is_none());
        assert_eq!(state.combo_carry, [8, 13]);
        assert!(state.pre_practice_music_rate.is_none());
        assert!(state.last_course_wheel_path.is_none());
        assert!(state.last_course_wheel_difficulty_name.is_none());
    }

    #[test]
    fn final_course_take_requires_an_active_course() {
        let mut state = SessionState::new(0, [0; MAX_PLAYERS]);
        assert!(state.take_final_course(false).is_none());
    }
}
