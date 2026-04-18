#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsPane {
    Main,
    Advanced,
    Uncommon,
}

impl OptionsPane {
    pub(super) const COUNT: usize = 3;

    #[inline(always)]
    pub(super) const fn index(self) -> usize {
        self as usize
    }

    #[inline(always)]
    pub(super) const fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Main,
            1 => Self::Advanced,
            _ => Self::Uncommon,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum PaneTransition {
    None,
    FadingOut { target: OptionsPane, t: f32 },
    FadingIn { t: f32 },
}

impl PaneTransition {
    #[inline(always)]
    pub(super) fn alpha(self) -> f32 {
        match self {
            Self::None => 1.0,
            Self::FadingOut { t, .. } => (1.0 - t).clamp(0.0, 1.0),
            Self::FadingIn { t } => t.clamp(0.0, 1.0),
        }
    }

    #[inline(always)]
    pub(super) fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }
}
