//! Native frame indexing for a codec-independent wavl chunk sequence.
use crate::{ChunkInfo, Encoding, Error, Format, reserve};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub chunk: ChunkInfo,
    pub start: u64,
    pub frames: u64,
    /// Last complete preceding data frame, or zero for initial digital silence.
    pub hold: u64,
}
#[derive(Debug, Default)]
pub(crate) struct Timeline {
    pub entries: Vec<Entry>,
    pub frames: u64,
    pub align: Option<u16>,
}
impl Timeline {
    pub fn push(&mut self, chunk: ChunkInfo, silence: u64) -> Result<(), Error> {
        reserve(&mut self.entries, 1)?;
        self.entries.push(Entry {
            chunk,
            start: 0,
            frames: silence,
            hold: 0,
        });
        Ok(())
    }
    pub fn initialize(&mut self, format: Format<'_>, fact: Option<u64>) -> Result<(), Error> {
        let fact = fact.ok_or(Error::Invalid("wavl requires fact sample count"))?;
        self.align = matches!(format.encoding, Encoding::Pcm | Encoding::Float)
            .then_some(format.block_align);
        let mut last = 0;
        let mut frames = 0u64;
        for entry in &mut self.entries {
            entry.start = frames;
            entry.hold = last;
            if entry.chunk.id == *b"data" {
                entry.frames = 0;
                if let Some(align) = self.align {
                    if !entry.chunk.size.is_multiple_of(u64::from(align)) {
                        return Err(Error::Invalid("partial PCM frame"));
                    }
                    entry.frames = entry.chunk.size / u64::from(align);
                    if entry.frames != 0 {
                        last = entry.chunk.offset + entry.chunk.size - u64::from(align);
                    }
                }
            }
            frames = frames
                .checked_add(entry.frames)
                .ok_or(Error::Limit("wavl frame count overflow"))?;
        }
        self.frames = frames;
        if self.align.is_some() && frames != fact {
            return Err(Error::Invalid("wavl fact sample count mismatch"));
        }
        Ok(())
    }
    pub fn at(&self, frame: u64) -> Option<Entry> {
        self.entries
            .get(
                self.entries
                    .partition_point(|e| e.start + e.frames <= frame),
            )
            .copied()
    }
}

/// Expand only the caller's bounded buffer. Every byte, including float NaN
/// payloads and low unused valid bits, is repeated without numeric conversion.
pub(crate) fn repeat(out: &mut [u8], frame: &[u8]) {
    if out.is_empty() {
        return;
    }
    out[..frame.len()].copy_from_slice(frame);
    let mut filled = frame.len();
    while filled < out.len() {
        let n = filled.min(out.len() - filled);
        out.copy_within(..n, filled);
        filled += n;
    }
}
