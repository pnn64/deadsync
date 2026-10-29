//! Frame-height declarations, T.81 B.2.5 and B.3.2.
use crate::{Error, FrameHeader, MarkerReader, Result};

/// Where the encoder declares a frame's final number of lines.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HeightDeclaration {
    /// Write the final height in SOF, without DNL.
    #[default]
    FrameHeader,
    /// Write this provisional SOF height and the final height in DNL directly
    /// after the first scan. Zero selects an initially unknown height; a
    /// nonzero value is a provisional height that DNL replaces.
    Dnl { initial_height: u16 },
}
impl HeightDeclaration {
    pub(crate) fn initial(self, height: u16) -> u16 {
        match self {
            Self::FrameHeader => height,
            Self::Dnl { initial_height } => initial_height,
        }
    }
    pub(crate) fn after_scan<S: crate::output::Sink>(
        self,
        output: &mut crate::output::Output<S>,
        height: u16,
        scan: usize,
    ) -> Result<()> {
        if scan == 0 && matches!(self, Self::Dnl { .. }) {
            output.segment(0xdc, &height.to_be_bytes())?;
        }
        Ok(())
    }
}

impl MarkerReader<'_> {
    /// Called only after a frame's first Scan event. Resolve DNL before
    /// allocating image storage. No entropy copy, second scan or full-image
    /// prepass is needed. Other DNL positions remain ordinary events and the
    /// process decoder rejects them. The public reader itself stays lossless.
    pub(crate) fn resolve_height(&mut self, frame: &mut FrameHeader) -> Result<()> {
        if let Some(data) = self.take_segment(0xdc)? {
            // B.2.5 permits Ld=4..65535; bytes beyond NL are not image data.
            let value = data.get(..2).ok_or(Error::Invalid("DNL length"))?;
            let height = u16::from_be_bytes([value[0], value[1]]);
            if height == 0 {
                return Err(Error::Invalid("zero DNL height"));
            }
            frame.height = height;
        } else if frame.height == 0 {
            return Err(Error::Invalid("missing DNL after first scan"));
        }
        Ok(())
    }
}
