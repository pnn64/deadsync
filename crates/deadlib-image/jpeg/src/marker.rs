use crate::{Error, Result};
use std::io::{self, BufRead, Read};

/// Read the first frame header without loading the image or retaining APP data.
/// A zero height signals that the height is deferred to a DNL segment.
pub fn read_frame_header(mut input: impl BufRead, limits: crate::Limits) -> Result<crate::FrameHeader> {
    let mut prefix = [0; 2];
    input.read_exact(&mut prefix).map_err(read_error)?;
    if prefix != [0xff, 0xd8] {
        return Err(Error::Invalid("missing SOI"));
    }
    let mut input = input.take(limits.max_metadata_bytes as u64);
    loop {
        if read_byte(&mut input)? != 0xff {
            continue;
        }
        let mut marker = read_byte(&mut input)?;
        while marker == 0xff {
            marker = read_byte(&mut input)?;
        }
        match marker {
            0 | 1 | 0xd0..=0xd7 => continue,
            0xd8 | 0xd9 | 0xda => return Err(Error::Invalid("missing SOF")),
            _ => {}
        }
        input.read_exact(&mut prefix).map_err(read_error)?;
        let length = usize::from(u16::from_be_bytes(prefix))
            .checked_sub(2)
            .ok_or(Error::Invalid("segment length below two"))?;
        if crate::Process::try_from(marker).is_ok() {
            let mut frame = [0; 6 + 3 * u8::MAX as usize];
            let bytes = frame.get_mut(..length).ok_or(Error::Invalid("SOF length"))?;
            input.read_exact(bytes).map_err(read_error)?;
            return crate::FrameHeader::parse(marker, bytes);
        }
        let mut remaining = length;
        while remaining != 0 {
            let consumed = input.fill_buf().map_err(read_error)?.len().min(remaining);
            if consumed == 0 {
                return Err(Error::Truncated);
            }
            input.consume(consumed);
            remaining -= consumed;
        }
    }
}

fn read_byte(input: &mut impl std::io::Read) -> Result<u8> {
    let mut byte = [0];
    input.read_exact(&mut byte).map_err(read_error)?;
    Ok(byte[0])
}

fn read_error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::UnexpectedEof => Error::Truncated,
        kind => Error::Io(kind),
    }
}

/// A marker segment or scan. Entropy bytes retain stuffing and restart markers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event<'a> {
    Segment {
        marker: u8,
        data: &'a [u8],
        offset: usize,
    },
    Scan {
        header: &'a [u8],
        entropy: &'a [u8],
        offset: usize,
    },
    End {
        offset: usize,
    },
}

/// Zero-copy JPEG interchange marker reader. Bytes after EOI remain unconsumed.
/// On an error the reader becomes terminal; malformed input cannot loop.
pub struct MarkerReader<'a> {
    input: &'a [u8],
    position: usize,
    ended: bool,
    strict: bool,
    pub(crate) missing_eoi: bool,
}

impl<'a> MarkerReader<'a> {
    pub fn new(input: &'a [u8]) -> Result<Self> {
        Self::with_strict(input, true)
    }

    /// With false, skip stray bytes between markers and expose a final scan
    /// ending at EOF. Image decoders must still verify complete image data;
    /// marker framing alone cannot establish whether a missing EOI is harmless.
    pub fn with_strict(input: &'a [u8], strict: bool) -> Result<Self> {
        if input.len() < 2 {
            return Err(Error::Truncated);
        }
        if input[..2] != [0xff, 0xd8] {
            return Err(Error::Invalid("missing SOI"));
        }
        Ok(Self {
            input,
            position: 2,
            ended: false,
            strict,
            missing_eoi: false,
        })
    }
    pub fn consumed(&self) -> usize {
        self.position
    }

    /// Consume an optional segment at the current boundary without reading
    /// the next scan's entropy. Used for mandatory first-scan DNL adjacency.
    pub(crate) fn take_segment(&mut self, expected: u8) -> Result<Option<&'a [u8]>> {
        let mut position = self.position;
        if !self.strict && position == self.input.len() {
            return Ok(None);
        }
        if self.input.get(position) != Some(&0xff) {
            return Err(if position == self.input.len() {
                Error::Truncated
            } else {
                Error::Invalid("expected marker prefix")
            });
        }
        while self.input.get(position) == Some(&0xff) {
            position += 1;
        }
        let marker = *self.input.get(position).ok_or(Error::Truncated)?;
        if marker != expected {
            return Ok(None);
        }
        match self.next().ok_or(Error::Truncated)?? {
            Event::Segment { data, .. } => Ok(Some(data)),
            _ => unreachable!(),
        }
    }

    fn read_event(&mut self) -> Result<Event<'a>> {
        if !self.strict {
            // Recover only outside declared segments and scan data. Stop at
            // the first actual marker, whose length and payload remain checked.
            loop {
                while self.input.get(self.position).is_some_and(|&b| b != 0xff) {
                    self.position += 1;
                }
                if self.position == self.input.len() {
                    self.ended = true;
                    self.missing_eoi = true;
                    return Ok(Event::End {
                        offset: self.position,
                    });
                }
                let mut next = self.position + 1;
                while self.input.get(next) == Some(&0xff) {
                    next += 1;
                }
                if self.input.get(next) != Some(&0) {
                    break;
                }
                self.position = next + 1;
            }
        }
        let offset = self.position;
        if self.input.get(self.position) != Some(&0xff) {
            return Err(if self.position == self.input.len() {
                Error::Truncated
            } else {
                Error::Invalid("expected marker prefix")
            });
        }
        while self.input.get(self.position) == Some(&0xff) {
            self.position += 1;
        }
        let marker = *self.input.get(self.position).ok_or(Error::Truncated)?;
        self.position += 1;
        match marker {
            0x00 | 0xd8 | 0xd0..=0xd7 => {
                return Err(Error::Invalid("unexpected standalone marker"));
            }
            0xd9 => {
                self.ended = true;
                return Ok(Event::End { offset });
            }
            0x01 => {
                return Ok(Event::Segment {
                    marker,
                    data: &[],
                    offset,
                });
            }
            _ => {}
        }
        let length = self
            .input
            .get(self.position..self.position + 2)
            .ok_or(Error::Truncated)?;
        let length = u16::from_be_bytes([length[0], length[1]]) as usize;
        if length < 2 {
            return Err(Error::Invalid("segment length below two"));
        }
        let end = self.position.checked_add(length).ok_or(Error::Truncated)?;
        let data = self
            .input
            .get(self.position + 2..end)
            .ok_or(Error::Truncated)?;
        self.position = end;
        if marker != 0xda {
            return Ok(Event::Segment {
                marker,
                data,
                offset,
            });
        }
        let start = self.position;
        loop {
            self.position += crate::simd::find_ff(&self.input[self.position..]);
            if self.position == self.input.len() {
                if self.strict {
                    return Err(Error::Truncated);
                }
                return Ok(Event::Scan {
                    header: data,
                    entropy: &self.input[start..],
                    offset,
                });
            }
            let prefix = self.position;
            self.position += 1;
            while self.input.get(self.position) == Some(&0xff) {
                self.position += 1;
            }
            let next = *self.input.get(self.position).ok_or(Error::Truncated)?;
            if next == 0 {
                if self.position != prefix + 1 {
                    return Err(Error::Invalid("fill before stuffed byte"));
                }
                self.position += 1;
            } else if (0xd0..=0xd7).contains(&next) {
                self.position += 1;
            } else {
                self.position = prefix;
                return Ok(Event::Scan {
                    header: data,
                    entropy: &self.input[start..prefix],
                    offset,
                });
            }
        }
    }
}

impl<'a> Iterator for MarkerReader<'a> {
    type Item = Result<Event<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let event = self.read_event();
        if event.is_err() {
            self.ended = true;
        }
        Some(event)
    }
}
