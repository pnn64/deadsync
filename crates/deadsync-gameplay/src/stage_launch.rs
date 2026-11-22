//! Launch policy shared by song and course selection.

pub const OPTIONS_WINDOW_SECONDS: f32 = 1.5;
pub const OPTIONS_ENTRY_SECONDS: f32 = 0.125 + 0.1 + 0.125 + 1.0;

#[derive(Clone, Debug)]
pub enum StageLaunchRequest<T> {
    Confirm(T),
    PlayerOptions,
}

impl<T> StageLaunchRequest<T> {
    pub fn map_selection<U>(self, map: impl FnOnce(T) -> U) -> StageLaunchRequest<U> {
        match self {
            Self::Confirm(selection) => StageLaunchRequest::Confirm(map(selection)),
            Self::PlayerOptions => StageLaunchRequest::PlayerOptions,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageLaunchTarget {
    Gameplay,
    PlayerOptions,
}

/// Read-only prompt state for a selection screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StageLaunchPrompt {
    #[default]
    Hidden,
    OfferOptions,
    EnteringOptions,
}

pub struct StageLaunch<T> {
    state: StageLaunchState<T>,
}

enum StageLaunchState<T> {
    Idle,
    Waiting {
        selection: T,
        target: StageLaunchTarget,
        elapsed: f32,
    },
    Ready {
        selection: T,
        target: StageLaunchTarget,
    },
}

impl<T> Default for StageLaunch<T> {
    fn default() -> Self {
        Self {
            state: StageLaunchState::Idle,
        }
    }
}

impl<T> StageLaunch<T> {
    pub fn request(&mut self, request: StageLaunchRequest<T>) {
        match (request, &mut self.state) {
            (StageLaunchRequest::Confirm(selection), launch @ StageLaunchState::Idle) => {
                *launch = StageLaunchState::Waiting {
                    selection,
                    target: StageLaunchTarget::Gameplay,
                    elapsed: 0.0,
                };
            }
            (
                StageLaunchRequest::PlayerOptions,
                StageLaunchState::Waiting {
                    target, elapsed, ..
                },
            ) if *target == StageLaunchTarget::Gameplay => {
                *target = StageLaunchTarget::PlayerOptions;
                *elapsed = 0.0;
            }
            _ => {}
        }
    }

    pub const fn prompt(&self) -> StageLaunchPrompt {
        match &self.state {
            StageLaunchState::Waiting {
                target: StageLaunchTarget::Gameplay,
                ..
            } => StageLaunchPrompt::OfferOptions,
            StageLaunchState::Waiting {
                target: StageLaunchTarget::PlayerOptions,
                ..
            } => StageLaunchPrompt::EnteringOptions,
            StageLaunchState::Idle | StageLaunchState::Ready { .. } => StageLaunchPrompt::Hidden,
        }
    }

    pub const fn preload_selection(&self) -> Option<&T> {
        match &self.state {
            StageLaunchState::Waiting {
                selection,
                target: StageLaunchTarget::Gameplay,
                ..
            } => Some(selection),
            _ => None,
        }
    }

    pub fn advance(&mut self, delta: f32) -> Option<StageLaunchTarget> {
        let StageLaunchState::Waiting {
            target, elapsed, ..
        } = &mut self.state
        else {
            return None;
        };
        *elapsed += delta.max(0.0);
        let deadline = match target {
            StageLaunchTarget::Gameplay => OPTIONS_WINDOW_SECONDS,
            StageLaunchTarget::PlayerOptions => OPTIONS_ENTRY_SECONDS,
        };
        if *elapsed < deadline {
            return None;
        }
        let StageLaunchState::Waiting {
            selection, target, ..
        } = std::mem::replace(&mut self.state, StageLaunchState::Idle)
        else {
            unreachable!()
        };
        self.state = StageLaunchState::Ready { selection, target };
        Some(target)
    }

    pub fn ready_selection(&self, target: StageLaunchTarget) -> Option<&T> {
        match &self.state {
            StageLaunchState::Ready {
                selection,
                target: ready,
            } if *ready == target => Some(selection),
            _ => None,
        }
    }

    pub fn ready_for(&self, target: StageLaunchTarget) -> bool {
        self.ready_selection(target).is_some()
    }

    /// Consume the confirmation only when the shell enters its destination.
    pub fn take_ready(&mut self, target: StageLaunchTarget) -> Option<T> {
        if !self.ready_for(target) {
            return None;
        }
        let StageLaunchState::Ready { selection, .. } =
            std::mem::replace(&mut self.state, StageLaunchState::Idle)
        else {
            unreachable!()
        };
        Some(selection)
    }
}
