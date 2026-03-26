use crate::core::ui::font::FontParseError;
use std::{error::Error as StdError, fmt};

#[derive(Debug)]
pub enum AssetError {
    FontParse(FontParseError),
    Image(image::ImageError),
    Backend(String),
    UnknownFont(&'static str),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FontParse(err) => write!(f, "{err}"),
            Self::Image(err) => write!(f, "{err}"),
            Self::Backend(err) => write!(f, "GPU texture operation failed: {err}"),
            Self::UnknownFont(name) => write!(f, "Unknown font name: {name}"),
        }
    }
}

impl StdError for AssetError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::FontParse(err) => Some(err),
            Self::Image(err) => Some(err),
            Self::Backend(_) | Self::UnknownFont(_) => None,
        }
    }
}

impl From<FontParseError> for AssetError {
    fn from(value: FontParseError) -> Self {
        Self::FontParse(value)
    }
}

impl From<image::ImageError> for AssetError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

impl From<Box<dyn StdError>> for AssetError {
    fn from(value: Box<dyn StdError>) -> Self {
        Self::Backend(value.to_string())
    }
}
