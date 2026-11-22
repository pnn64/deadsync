//! Confirmed song selection and its launch deadline.

use crate::GameplayInputPlayStyle;
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use std::sync::Arc;

pub const OPTIONS_WINDOW_SECONDS: f32 = 1.5;
pub const OPTIONS_ENTRY_SECONDS: f32 = 0.125 + 0.1 + 0.125 + 1.0;

/// The selected song and chart slots in wheel order. Solo play uses the primary slot.
#[derive(Clone, Debug)]
pub struct SongSelection {
    pub song: Arc<SongData>,
    pub steps: [usize; MAX_PLAYERS],
    pub preferred: [usize; MAX_PLAYERS],
}

impl SongSelection {
    pub const fn chart_slots(
        &self,
        play_style: GameplayInputPlayStyle,
    ) -> ([usize; MAX_PLAYERS], [usize; MAX_PLAYERS]) {
        if play_style.player_count() == 1 {
            (
                [self.steps[0]; MAX_PLAYERS],
                [self.preferred[0]; MAX_PLAYERS],
            )
        } else {
            (self.steps, self.preferred)
        }
    }
}

#[derive(Clone, Debug)]
pub enum SongLaunchRequest {
    Confirm(SongSelection),
    PlayerOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLaunchTarget {
    Gameplay,
    PlayerOptions,
}

/// Read-only prompt state for the selection screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SongLaunchPrompt {
    #[default]
    Hidden,
    OfferOptions,
    EnteringOptions,
}

#[derive(Default)]
pub struct SongLaunch {
    state: SongLaunchState,
}

#[derive(Default)]
enum SongLaunchState {
    #[default]
    Idle,
    Waiting {
        selection: SongSelection,
        target: SongLaunchTarget,
        elapsed: f32,
    },
    Ready {
        selection: SongSelection,
        target: SongLaunchTarget,
    },
}

impl SongLaunch {
    pub fn request(&mut self, request: SongLaunchRequest) {
        match (request, &mut self.state) {
            (SongLaunchRequest::Confirm(selection), launch @ SongLaunchState::Idle) => {
                *launch = SongLaunchState::Waiting {
                    selection,
                    target: SongLaunchTarget::Gameplay,
                    elapsed: 0.0,
                };
            }
            (
                SongLaunchRequest::PlayerOptions,
                SongLaunchState::Waiting {
                    target, elapsed, ..
                },
            ) if *target == SongLaunchTarget::Gameplay => {
                *target = SongLaunchTarget::PlayerOptions;
                *elapsed = 0.0;
            }
            _ => {}
        }
    }

    pub const fn prompt(&self) -> SongLaunchPrompt {
        match &self.state {
            SongLaunchState::Waiting {
                target: SongLaunchTarget::Gameplay,
                ..
            } => SongLaunchPrompt::OfferOptions,
            SongLaunchState::Waiting {
                target: SongLaunchTarget::PlayerOptions,
                ..
            } => SongLaunchPrompt::EnteringOptions,
            SongLaunchState::Idle | SongLaunchState::Ready { .. } => SongLaunchPrompt::Hidden,
        }
    }

    pub const fn preload_selection(&self) -> Option<&SongSelection> {
        match &self.state {
            SongLaunchState::Waiting {
                selection,
                target: SongLaunchTarget::Gameplay,
                ..
            } => Some(selection),
            _ => None,
        }
    }

    pub fn advance(&mut self, delta: f32) -> Option<SongLaunchTarget> {
        let SongLaunchState::Waiting {
            target, elapsed, ..
        } = &mut self.state
        else {
            return None;
        };
        *elapsed += delta.max(0.0);
        let deadline = match target {
            SongLaunchTarget::Gameplay => OPTIONS_WINDOW_SECONDS,
            SongLaunchTarget::PlayerOptions => OPTIONS_ENTRY_SECONDS,
        };
        if *elapsed < deadline {
            return None;
        }
        let SongLaunchState::Waiting {
            selection, target, ..
        } = std::mem::take(&mut self.state)
        else {
            unreachable!()
        };
        self.state = SongLaunchState::Ready { selection, target };
        Some(target)
    }

    pub fn ready_for(&self, target: SongLaunchTarget) -> bool {
        matches!(&self.state, SongLaunchState::Ready { target: ready, .. } if *ready == target)
    }

    /// Consume the confirmation only when the shell enters its destination.
    pub fn take_ready(&mut self, target: SongLaunchTarget) -> Option<SongSelection> {
        if !self.ready_for(target) {
            return None;
        }
        let SongLaunchState::Ready { selection, .. } = std::mem::take(&mut self.state) else {
            unreachable!()
        };
        Some(selection)
    }
}
