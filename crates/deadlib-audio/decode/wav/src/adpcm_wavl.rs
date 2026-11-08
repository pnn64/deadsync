//! Borrowed block-codec wavl sequences with bounded held-frame expansion.
use crate::{
    Adpcm, Error, Wave, reserve,
    timeline::{Entry, repeat},
};

#[derive(Clone, Copy, Debug)]
struct Bound {
    end: u64,
    /// Index plus one of the preceding nonempty data child; zero means silence.
    hold: usize,
}

/// Borrowed Microsoft/IMA ADPCM wavl audio in physical child order. Each data
/// child begins a new block sequence. Interior encoder padding is audible;
/// the global fact count trims only the end of the complete sequence.
///
/// Construction adds one compact index allocation to the container's inventory.
/// Reading allocates nothing and uses the caller's one-block i16 scratch. Keep
/// the Wave alive while using this view. Flat data remains in [`crate::AdpcmData`].
#[derive(Debug)]
pub struct AdpcmWavl<'a> {
    codec: Adpcm<'a>,
    bytes: &'a [u8],
    entries: &'a [Entry],
    bounds: Vec<Bound>,
    frames: u64,
}

impl<'a> AdpcmWavl<'a> {
    pub fn new(wave: &'a Wave<'a>) -> Result<Self, Error> {
        let codec = Adpcm::from_format(wave.format)?;
        let entries = &wave
            .timeline()
            .ok_or(Error::Unsupported(
                "ADPCM wavl requires a LIST/wavl section",
            ))?
            .entries;
        let frames = wave
            .fact
            .ok_or(Error::Invalid("wavl requires fact sample count"))?;
        let mut bounds = Vec::new();
        reserve(&mut bounds, entries.len())?;
        let mut end = 0u64;
        let mut hold = 0;
        for (i, entry) in entries.iter().enumerate() {
            let n = if entry.chunk.id == *b"data" {
                codec.data_frames(entry.chunk.size)?
            } else if entry.chunk.id == *b"slnt" {
                entry.frames
            } else {
                0
            };
            end = end
                .checked_add(n)
                .ok_or(Error::Limit("ADPCM wavl frame count overflow"))?;
            bounds.push(Bound { end, hold });
            if entry.chunk.id == *b"data" && n != 0 {
                hold = i + 1;
            }
        }
        if frames > end {
            return Err(Error::Invalid("ADPCM fact exceeds wavl capacity"));
        }
        Ok(Self {
            codec,
            bytes: wave.as_bytes(),
            entries,
            bounds,
            frames,
        })
    }
    pub fn codec(&self) -> Adpcm<'a> {
        self.codec
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    /// Read complete interleaved frames at an exact logical position; returns
    /// samples. Initial silence is zero. Later silence holds the last decoded
    /// frame, including when reading starts inside that silence. Invalid block
    /// headers are reported when their samples (or held frame) are requested;
    /// the output may then contain a decoded prefix.
    pub fn read_frames(
        &self,
        start: u64,
        out: &mut [i16],
        scratch: &mut [i16],
    ) -> Result<usize, Error> {
        if start > self.frames {
            return Err(Error::Invalid("frame position"));
        }
        if scratch.len() < self.codec.scratch_samples() {
            return Err(Error::Limit("ADPCM scratch capacity"));
        }
        let channels = self.codec.channels();
        let frames = (self.frames - start).min((out.len() / channels) as u64) as usize;
        let mut index = self.bounds.partition_point(|b| b.end <= start);
        let mut done = 0;
        while done < frames {
            let bound = self.bounds[index];
            let position = start + done as u64;
            let n = (bound.end - position).min((frames - done) as u64) as usize;
            if n != 0 {
                let dest = &mut out[done * channels..(done + n) * channels];
                if self.entries[index].chunk.id == *b"data" {
                    let begin = self.begin(index);
                    self.codec
                        .read_data(self.data(index), position - begin, dest, scratch)?;
                } else if bound.hold == 0 {
                    dest.fill(0);
                } else {
                    let held = bound.hold - 1;
                    let last = self.bounds[held].end - self.begin(held) - 1;
                    let block = last / self.codec.frames_per_block() as u64;
                    let skip = (last % self.codec.frames_per_block() as u64) as usize;
                    let offset = block as usize * self.codec.block_bytes();
                    let data = self.data(held);
                    self.codec.decode_block(&data[offset..], scratch)?;
                    repeat(dest, &scratch[skip * channels..(skip + 1) * channels]);
                }
                done += n;
            }
            index += 1;
        }
        Ok(done * channels)
    }
    fn begin(&self, index: usize) -> u64 {
        if index == 0 {
            0
        } else {
            self.bounds[index - 1].end
        }
    }
    fn data(&self, index: usize) -> &'a [u8] {
        let c = self.entries[index].chunk;
        &self.bytes[c.offset as usize..(c.offset + c.size) as usize]
    }
}
