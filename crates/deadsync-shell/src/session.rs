use std::{path::PathBuf, time::Instant};

use deadsync_core::input::MAX_PLAYERS;
use deadsync_score::stage_stats;

use crate::CourseRunState;

/// Session-wide values that survive screen swaps.
///
/// `EvaluationPage` remains opaque so the shell can own lifecycle policy
/// without depending on the bundled theme's evaluation screen state.
pub struct SessionState<EvaluationPage> {
    pub preferred_difficulty_index: usize,
    pub session_start_time: Option<Instant>,
    pub played_stages: Vec<stage_stats::StageSummary>,
    pub pending_post_select_summary_exit: bool,
    pub course_individual_stage_indices: Vec<usize>,
    pub combo_carry: [u32; MAX_PLAYERS],
    pub gameplay_restart_count: u32,
    pub restart_pending: bool,
    pub course_run: Option<CourseRunState>,
    pub course_stage_eval_pages: Vec<EvaluationPage>,
    pub course_eval_pages: Vec<EvaluationPage>,
    pub course_eval_page_index: usize,
    pub last_course_wheel_path: Option<PathBuf>,
    pub last_course_wheel_difficulty_name: Option<String>,
}

impl<EvaluationPage> SessionState<EvaluationPage> {
    pub const fn new(preferred_difficulty_index: usize, combo_carry: [u32; MAX_PLAYERS]) -> Self {
        Self {
            preferred_difficulty_index,
            session_start_time: None,
            played_stages: Vec::new(),
            pending_post_select_summary_exit: false,
            course_individual_stage_indices: Vec::new(),
            combo_carry,
            gameplay_restart_count: 0,
            restart_pending: false,
            course_run: None,
            course_stage_eval_pages: Vec::new(),
            course_eval_pages: Vec::new(),
            course_eval_page_index: 0,
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
        self.course_stage_eval_pages.clear();
        self.clear_course_eval_pages();
    }

    pub fn record_stage_result(
        &mut self,
        stage: Option<stage_stats::StageSummary>,
        course_page: Option<EvaluationPage>,
    ) {
        if let Some(stage) = stage {
            self.played_stages.push(stage.clone());
            if self.course_run.is_some() {
                self.course_individual_stage_indices
                    .push(self.played_stages.len().saturating_sub(1));
            }
            if let Some(course) = self.course_run.as_mut() {
                course.stage_summaries.push(stage);
            }
        }
        if let Some(page) = course_page {
            self.course_stage_eval_pages.push(page);
        }
    }

    pub fn take_final_course(
        &mut self,
        failed: bool,
    ) -> Option<(CourseRunState, Vec<EvaluationPage>)> {
        let course = self.course_run.as_ref()?;
        if !stage_stats::course_eval_is_final(course.next_stage_index, course.stages.len(), failed)
        {
            return None;
        }
        let course = self
            .course_run
            .take()
            .expect("course presence checked before finalization");
        let pages = std::mem::take(&mut self.course_stage_eval_pages);
        self.clear_course_eval_pages();
        Some((course, pages))
    }

    pub fn clear_course_eval_pages(&mut self) {
        self.course_eval_pages.clear();
        self.course_eval_page_index = 0;
    }

    pub fn replace_course_eval_pages(&mut self, pages: Vec<EvaluationPage>) {
        self.course_eval_pages = pages;
        self.course_eval_page_index = 0;
    }

    pub fn step_course_eval_page(&mut self, delta: i32) -> Option<EvaluationPage>
    where
        EvaluationPage: Clone,
    {
        let len = self.course_eval_pages.len();
        if len <= 1 || delta == 0 {
            return None;
        }
        self.course_eval_page_index =
            (self.course_eval_page_index as i32 + delta).rem_euclid(len as i32) as usize;
        self.course_eval_pages
            .get(self.course_eval_page_index)
            .cloned()
    }

    pub fn reset_for_menu(&mut self, combo_carry: [u32; MAX_PLAYERS]) {
        self.session_start_time = None;
        self.played_stages.clear();
        self.course_individual_stage_indices.clear();
        self.combo_carry = combo_carry;
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
        let state = SessionState::<usize>::new(3, [12, 34]);
        assert_eq!(state.preferred_difficulty_index, 3);
        assert_eq!(state.combo_carry, [12, 34]);
        assert!(state.session_start_time.is_none());
        assert!(state.played_stages.is_empty());
        assert_eq!(state.gameplay_restart_count, 0);
    }

    #[test]
    fn play_session_starts_once_and_clears_prior_lists() {
        let mut state = SessionState::<usize>::new(0, [0; MAX_PLAYERS]);
        state.course_individual_stage_indices.extend([1, 2]);
        let started = Instant::now();
        assert!(state.begin_play_session(started));
        assert_eq!(state.session_start_time, Some(started));
        assert!(state.course_individual_stage_indices.is_empty());
        assert!(!state.begin_play_session(started));
    }

    #[test]
    fn course_cleanup_drops_pages_and_resets_index() {
        let mut state = SessionState::new(0, [0; MAX_PLAYERS]);
        state.course_stage_eval_pages.extend([1, 2]);
        state.course_eval_pages.push(3);
        state.course_eval_page_index = 2;
        state.clear_course_runtime();
        assert!(state.course_stage_eval_pages.is_empty());
        assert!(state.course_eval_pages.is_empty());
        assert_eq!(state.course_eval_page_index, 0);
    }

    #[test]
    fn menu_reset_clears_session_and_course_navigation() {
        let mut state = SessionState::<usize>::new(4, [1, 2]);
        state.session_start_time = Some(Instant::now());
        state.course_stage_eval_pages.push(1);
        state.last_course_wheel_path = Some(PathBuf::from("course.crs"));
        state.last_course_wheel_difficulty_name = Some("Hard".to_string());
        state.reset_for_menu([8, 13]);
        assert!(state.session_start_time.is_none());
        assert_eq!(state.combo_carry, [8, 13]);
        assert!(state.course_stage_eval_pages.is_empty());
        assert!(state.last_course_wheel_path.is_none());
        assert!(state.last_course_wheel_difficulty_name.is_none());
    }

    #[test]
    fn course_pages_record_clear_replace_and_wrap() {
        let mut state = SessionState::new(0, [0; MAX_PLAYERS]);
        state.record_stage_result(None, Some(10));
        assert_eq!(state.course_stage_eval_pages, [10]);

        state.replace_course_eval_pages(vec![20, 30, 40]);
        assert_eq!(state.step_course_eval_page(-1), Some(40));
        assert_eq!(state.course_eval_page_index, 2);
        assert_eq!(state.step_course_eval_page(1), Some(20));
        assert_eq!(state.course_eval_page_index, 0);

        state.clear_course_eval_pages();
        assert!(state.course_eval_pages.is_empty());
        assert_eq!(state.course_eval_page_index, 0);
    }

    #[test]
    fn final_course_take_requires_an_active_course() {
        let mut state = SessionState::<usize>::new(0, [0; MAX_PLAYERS]);
        state.course_stage_eval_pages.push(1);
        assert!(state.take_final_course(false).is_none());
        assert_eq!(state.course_stage_eval_pages, [1]);
    }
}
