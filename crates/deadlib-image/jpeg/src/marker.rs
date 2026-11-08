use crate::{Error, Result};

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
}

impl<'a> MarkerReader<'a> {
    pub fn new(input: &'a [u8]) -> Result<Self> {
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
        })
    }
    pub fn consumed(&self) -> usize {
        self.position
    }

    /// Consume an optional segment at the current boundary without reading
    /// the next scan's entropy. Used for mandatory first-scan DNL adjacency.
    pub(crate) fn take_segment(&mut self, expected: u8) -> Result<Option<&'a [u8]>> {
        let mut position = self.position;
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
            let byte = *self.input.get(self.position).ok_or(Error::Truncated)?;
            if byte != 0xff {
                self.position += 1;
                continue;
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
