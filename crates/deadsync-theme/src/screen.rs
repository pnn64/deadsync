/// Stable identity for a screen supplied by a concrete theme.
///
/// The shell treats this value as opaque. Only the concrete theme interprets
/// the identifier or decides how it participates in that theme's screen flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThemeScreenId(&'static str);

impl ThemeScreenId {
    #[inline(always)]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[inline(always)]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Generic identity/effect boundary implemented by each concrete theme.
pub trait Theme {
    type Screen: Copy + Eq;
    type RuntimeRequest;

    fn screen_id(screen: Self::Screen) -> ThemeScreenId;
}

#[cfg(test)]
mod tests {
    use super::ThemeScreenId;

    #[test]
    fn screen_ids_are_opaque_stable_strings() {
        const MENU: ThemeScreenId = ThemeScreenId::new("ScreenTitleMenu");
        assert_eq!(MENU.as_str(), "ScreenTitleMenu");
    }
}
