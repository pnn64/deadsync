use crate::Format;
use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum ImageError {
    Io(io::Error),
    Decode {
        format: Format,
        source: Box<dyn Error + Send + Sync>,
    },
    Encode {
        format: Format,
        source: Box<dyn Error + Send + Sync>,
    },
    LimitExceeded,
    UnsupportedFormat,
}

pub type ImageResult<T> = Result<T, ImageError>;

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Decode { format, source } => write!(f, "could not decode {format:?}: {source}"),
            Self::Encode { format, source } => write!(f, "could not encode {format:?}: {source}"),
            Self::LimitExceeded => f.write_str("image dimensions or allocation exceed the limit"),
            Self::UnsupportedFormat => f.write_str("unsupported image format"),
        }
    }
}

impl Error for ImageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Decode { source, .. } | Self::Encode { source, .. } => Some(source.as_ref()),
            Self::LimitExceeded | Self::UnsupportedFormat => None,
        }
    }
}

impl From<io::Error> for ImageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
