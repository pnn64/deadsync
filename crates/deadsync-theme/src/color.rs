//! Shared judgment palette values and configurable difficulty color schemes.

/// User-configurable judgment color roles. Fantastic is split into blue and
/// white because FA+ gameplay may show either variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JudgmentColorRole {
    FantasticBlue,
    FantasticWhite,
    Excellent,
    Great,
    Decent,
    WayOff,
    Miss,
}

impl JudgmentColorRole {
    pub const ALL: [Self; 7] = [
        Self::FantasticBlue,
        Self::FantasticWhite,
        Self::Excellent,
        Self::Great,
        Self::Decent,
        Self::WayOff,
        Self::Miss,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::FantasticBlue => 0,
            Self::FantasticWhite => 1,
            Self::Excellent => 2,
            Self::Great => 3,
            Self::Decent => 4,
            Self::WayOff => 5,
            Self::Miss => 6,
        }
    }

    #[must_use]
    pub const fn config_key(self) -> &'static str {
        match self {
            Self::FantasticBlue => "FantasticBlue",
            Self::FantasticWhite => "FantasticWhite",
            Self::Excellent => "Excellent",
            Self::Great => "Great",
            Self::Decent => "Decent",
            Self::WayOff => "WayOff",
            Self::Miss => "Miss",
        }
    }
}

/// Full and context-dimmed colors for every judgment role.
///
/// Entries follow `JudgmentColorRole::ALL`; themes supply each context variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgmentPalette {
    pub colors: [[f32; 4]; 7],
    pub gameplay_dim: [[f32; 4]; 7],
    pub evaluation_dim: [[f32; 4]; 7],
}

impl JudgmentPalette {
    #[must_use]
    pub const fn new(
        colors: [[f32; 4]; 7],
        gameplay_dim: [[f32; 4]; 7],
        evaluation_dim: [[f32; 4]; 7],
    ) -> Self {
        Self {
            colors,
            gameplay_dim,
            evaluation_dim,
        }
    }

    /// Derive dim RGB variants with gameplay/evaluation peaks on a 0..=255 scale.
    /// Alpha is preserved; black remains black.
    #[must_use]
    pub fn from_base_colors(colors: [[f32; 4]; 7], dim_peaks: [u8; 2]) -> Self {
        Self {
            colors,
            gameplay_dim: colors.map(|color| dim_judgment_color(color, dim_peaks[0])),
            evaluation_dim: colors.map(|color| dim_judgment_color(color, dim_peaks[1])),
        }
    }

    #[must_use]
    pub const fn color(self, role: JudgmentColorRole) -> [f32; 4] {
        self.colors[role.index()]
    }

    #[must_use]
    pub const fn gameplay_dim_color(self, role: JudgmentColorRole) -> [f32; 4] {
        self.gameplay_dim[role.index()]
    }

    #[must_use]
    pub const fn evaluation_dim_color(self, role: JudgmentColorRole) -> [f32; 4] {
        self.evaluation_dim[role.index()]
    }

    /// Replace one base color and derive all dim variants using the supplied peaks.
    #[must_use]
    pub fn with_color(
        mut self,
        role: JudgmentColorRole,
        color: [f32; 4],
        dim_peaks: [u8; 2],
    ) -> Self {
        self.colors[role.index()] = color;
        Self::from_base_colors(self.colors, dim_peaks)
    }
}

fn dim_judgment_color(color: [f32; 4], brightest_channel: u8) -> [f32; 4] {
    let max = color[0].max(color[1]).max(color[2]);
    if max <= f32::EPSILON {
        return [0.0, 0.0, 0.0, color[3]];
    }
    let scale = (f32::from(brightest_channel) / 255.0) / max;
    [
        color[0] * scale,
        color[1] * scale,
        color[2] * scale,
        color[3],
    ]
}

/// zmod's selectable difficulty-color rules. `SimplyLove` keeps difficulty
/// colors relative to the active theme color; `Itg` and `Ddr` use fixed
/// arcade-game palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DifficultyColorScheme {
    #[default]
    SimplyLove,
    Itg,
    Ddr,
}

impl DifficultyColorScheme {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SimplyLove => "Simply Love",
            Self::Itg => "ITG",
            Self::Ddr => "DDR",
        }
    }
}

impl std::str::FromStr for DifficultyColorScheme {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let key = || {
            value
                .bytes()
                .filter(u8::is_ascii_alphanumeric)
                .map(|b| b.to_ascii_lowercase())
        };
        if key().eq(b"simplylove".iter().copied())
            || key().eq(b"sl".iter().copied())
            || key().eq(b"default".iter().copied())
        {
            Ok(Self::SimplyLove)
        } else if key().eq(b"itg".iter().copied()) {
            Ok(Self::Itg)
        } else if key().eq(b"ddr".iter().copied()) {
            Ok(Self::Ddr)
        } else {
            Err(())
        }
    }
}

/// Theme-supplied built-in judgment palette and custom-color dimming levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgmentPalettePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub palette: JudgmentPalette,
    /// Brightest RGB channels for custom gameplay and evaluation colors, respectively.
    pub dim_peaks: [u8; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_dims_preserve_alpha_and_follow_supplied_peaks() {
        let mut colors = [[0.25, 0.5, 1.0, 0.75]; 7];
        colors[JudgmentColorRole::Miss.index()] = [0.0, 0.0, 0.0, 0.25];
        let palette = JudgmentPalette::from_base_colors(colors, [102, 51]);

        assert_eq!(palette.colors, colors);
        assert_eq!(
            palette.gameplay_dim_color(JudgmentColorRole::Great),
            [0.1, 0.2, 0.4, 0.75]
        );
        assert_eq!(
            palette.evaluation_dim_color(JudgmentColorRole::Great),
            [0.05, 0.1, 0.2, 0.75]
        );
        assert_eq!(
            palette.gameplay_dim_color(JudgmentColorRole::Miss),
            [0.0, 0.0, 0.0, 0.25]
        );
    }

    #[test]
    fn difficulty_color_scheme_parses_zmod_values() {
        use std::str::FromStr;

        assert_eq!(
            DifficultyColorScheme::from_str("Simply Love"),
            Ok(DifficultyColorScheme::SimplyLove)
        );
        assert_eq!(
            DifficultyColorScheme::from_str("ITG"),
            Ok(DifficultyColorScheme::Itg)
        );
        assert_eq!(
            DifficultyColorScheme::from_str("ddr"),
            Ok(DifficultyColorScheme::Ddr)
        );
        assert_eq!(
            "  Simply-Love  ".parse(),
            Ok(DifficultyColorScheme::SimplyLove)
        );
        assert_eq!("SL".parse(), Ok(DifficultyColorScheme::SimplyLove));
        assert_eq!("default".parse(), Ok(DifficultyColorScheme::SimplyLove));
        assert!(DifficultyColorScheme::from_str("unknown").is_err());
    }
}
