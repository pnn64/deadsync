//! Cursor and interruption policy for course playback.
//!
//! Stage metadata and recorded results remain with their runtime owners. Queries
//! receive the live stage count and Endless mode instead of retaining copies.

/// Whether an Endless course needs another prepared cycle before stage entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourseRenewal {
    NotRequired,
    AppendCycle,
    InvalidCursor,
}

/// The cursor points past the active stage after that stage enters successfully.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CourseProgress {
    next_stage_index: usize,
    interrupted: bool,
}

impl CourseProgress {
    #[must_use]
    pub const fn new(next_stage_index: usize) -> Self {
        Self {
            next_stage_index,
            interrupted: false,
        }
    }

    #[must_use]
    pub const fn next_stage_index(self) -> usize {
        self.next_stage_index
    }

    #[must_use]
    pub const fn is_interrupted(self) -> bool {
        self.interrupted
    }

    #[must_use]
    pub const fn has_next_stage(self, stage_count: usize, endless: bool) -> bool {
        !self.interrupted && stage_count > 0 && (endless || self.next_stage_index < stage_count)
    }

    /// Availability uses modulo for Endless. Entry must separately validate
    /// whether a new cycle needs preparation through the renewal decision.
    #[must_use]
    pub const fn stage_index(self, stage_count: usize, endless: bool) -> Option<usize> {
        if !self.has_next_stage(stage_count, endless) {
            return None;
        }
        Some(if endless {
            self.next_stage_index % stage_count
        } else {
            self.next_stage_index
        })
    }

    #[must_use]
    pub const fn is_finished(self, stage_count: usize, endless: bool, failed: bool) -> bool {
        failed || self.interrupted || (!endless && self.next_stage_index >= stage_count)
    }

    /// A Lua replacement reloads the active stage without rewinding the
    /// canonical cursor or changing the interruption state.
    #[must_use]
    pub const fn for_entry(self, lua_reload: bool) -> Self {
        Self {
            next_stage_index: if lua_reload {
                self.next_stage_index.saturating_sub(1)
            } else {
                self.next_stage_index
            },
            interrupted: self.interrupted,
        }
    }

    /// Advance only after stage initialization and entry have succeeded.
    pub const fn finish_entry(&mut self, lua_reload: bool) {
        if !lua_reload {
            self.next_stage_index = self.next_stage_index.saturating_add(1);
        }
    }

    #[must_use]
    pub const fn uses_next_stage_lead_in(self) -> bool {
        self.next_stage_index > 0
    }

    /// Loading interruption finishes the course without failing its players.
    pub const fn interrupt(&mut self) {
        self.interrupted = true;
    }

    /// A failed load has no result of its own. Only recorded stages permit
    /// recovery through the accumulated course result.
    #[must_use]
    pub const fn interrupt_loading(&mut self, completed_stages: usize) -> bool {
        if completed_stages == 0 {
            return false;
        }
        self.interrupt();
        true
    }

    #[must_use]
    pub const fn can_append_endless_cycle(self, endless: bool) -> bool {
        !self.interrupted && endless
    }

    /// Preserve the entry-time comparison independently of availability.
    ///
    /// Interruption is checked by append authorization after the caller requests
    /// a fresh cycle. Keeping that order lets a rejected renewal finish through
    /// the existing course-result path.
    #[must_use]
    pub const fn renewal(self, stage_count: usize, endless: bool) -> CourseRenewal {
        if !endless || self.next_stage_index < stage_count {
            CourseRenewal::NotRequired
        } else if self.next_stage_index == stage_count {
            CourseRenewal::AppendCycle
        } else {
            CourseRenewal::InvalidCursor
        }
    }
}

#[cfg(test)]
#[path = "course_progress_tests.rs"]
mod tests;
