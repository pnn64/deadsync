use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenType {
    Exclusive,
    Borderless,
}

impl FullscreenType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "Exclusive",
            Self::Borderless => "Borderless",
        }
    }
}

impl FromStr for FullscreenType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "exclusive" => Ok(Self::Exclusive),
            "borderless" => Ok(Self::Borderless),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Windowed,
    Fullscreen(FullscreenType),
}
