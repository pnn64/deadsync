use super::{count, length, nul_terminated, output};
use crate::{
    Endian, Error, Limits, RawChunk,
    mux::{put16, put32},
};

#[derive(Clone, Copy, Debug)]
pub struct List<'a> {
    raw: &'a [u8],
    endian: Endian,
    limits: Limits,
}
impl<'a> List<'a> {
    pub fn parse(raw: &'a [u8], endian: Endian, limits: Limits) -> Result<Self, Error> {
        if raw.len() < 4 {
            return Err(Error::Truncated);
        }
        let v = Self {
            raw,
            endian,
            limits,
        };
        for e in v.entries() {
            e?;
        }
        Ok(v)
    }
    pub fn kind(self) -> [u8; 4] {
        self.raw[..4].try_into().unwrap()
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn entries(self) -> ListEntries<'a> {
        ListEntries {
            raw: &self.raw[4..],
            pos: 0,
            endian: self.endian,
            left: self.limits.chunks,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ListEntry<'a> {
    pub id: [u8; 4],
    pub data: &'a [u8],
    pub padding: &'a [u8],
}
impl<'a> ListEntry<'a> {
    /// INFO strings may use legacy code pages. No implicit UTF-8 conversion.
    pub fn text_bytes(self) -> &'a [u8] {
        nul_terminated(self.data)
    }
    pub fn associated(self, endian: Endian) -> Result<AssociatedText<'a>, Error> {
        match &self.id {
            b"labl" | b"note" => {
                if self.data.len() < 5 || self.data.last() != Some(&0) {
                    return Err(Error::Invalid("associated text termination"));
                }
                Ok(AssociatedText {
                    cue_id: endian.u32(self.data),
                    sample_length: None,
                    purpose: None,
                    locale: None,
                    text: &self.data[4..],
                })
            }
            b"ltxt" => {
                if self.data.len() < 20 {
                    return Err(Error::Truncated);
                }
                if self.data.len() > 20 && self.data.last() != Some(&0) {
                    return Err(Error::Invalid("associated text termination"));
                }
                Ok(AssociatedText {
                    cue_id: endian.u32(self.data),
                    sample_length: Some(endian.u32(&self.data[4..])),
                    purpose: Some(self.data[8..12].try_into().unwrap()),
                    locale: Some([
                        endian.u16(&self.data[12..]),
                        endian.u16(&self.data[14..]),
                        endian.u16(&self.data[16..]),
                        endian.u16(&self.data[18..]),
                    ]),
                    text: &self.data[20..],
                })
            }
            _ => Err(Error::Unsupported("associated data subtype")),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct AssociatedText<'a> {
    pub cue_id: u32,
    pub sample_length: Option<u32>,
    pub purpose: Option<[u8; 4]>,
    /// Country, language, dialect and code page, respectively.
    pub locale: Option<[u16; 4]>,
    pub text: &'a [u8],
}
pub struct ListEntries<'a> {
    raw: &'a [u8],
    pos: usize,
    endian: Endian,
    left: usize,
}
impl<'a> Iterator for ListEntries<'a> {
    type Item = Result<ListEntry<'a>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.raw.len() {
            return None;
        }
        let result = (|| {
            if self.left == 0 {
                return Err(Error::Limit("LIST entry count"));
            }
            self.left -= 1;
            let head = self
                .raw
                .get(
                    self.pos
                        ..self
                            .pos
                            .checked_add(8)
                            .ok_or(Error::Invalid("LIST offset overflow"))?,
                )
                .ok_or(Error::Truncated)?;
            let n = self.endian.u32(&head[4..]) as usize;
            let start = self.pos + 8;
            let end = start
                .checked_add(n)
                .ok_or(Error::Invalid("LIST size overflow"))?;
            let next = end
                .checked_add(n & 1)
                .ok_or(Error::Invalid("LIST pad overflow"))?;
            let data = self.raw.get(start..end).ok_or(Error::Truncated)?;
            let padding = self.raw.get(end..next).ok_or(Error::Truncated)?;
            self.pos = next;
            Ok(ListEntry {
                id: head[..4].try_into().unwrap(),
                data,
                padding,
            })
        })();
        if result.is_err() {
            self.pos = self.raw.len();
        }
        Some(result)
    }
}
impl std::iter::FusedIterator for ListEntries<'_> {}
pub fn write_list(
    kind: [u8; 4],
    entries: &[RawChunk<'_>],
    endian: Endian,
    out: &mut [u8],
) -> Result<usize, Error> {
    let mut len = 4usize;
    for e in entries {
        u32::try_from(e.data.len()).map_err(|_| Error::Limit("LIST entry size"))?;
        len = length(len, 1, 8, e.data.len())?
            .checked_add(e.data.len() & 1)
            .ok_or(Error::Limit("LIST size"))?;
    }
    let b = output(out, len)?;
    b[..4].copy_from_slice(&kind);
    let mut pos = 4;
    for e in entries {
        b[pos..pos + 4].copy_from_slice(&e.id);
        put32(&mut b[pos + 4..], e.data.len() as u32, endian);
        pos += 8;
        b[pos..pos + e.data.len()].copy_from_slice(e.data);
        pos += e.data.len();
        if pos & 1 != 0 {
            b[pos] = 0;
            pos += 1;
        }
    }
    Ok(len)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CuePoint {
    pub id: u32,
    pub position: u32,
    pub data_chunk: [u8; 4],
    pub chunk_start: u32,
    pub block_start: u32,
    pub sample_offset: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Cues<'a> {
    raw: &'a [u8],
    endian: Endian,
}
impl<'a> Cues<'a> {
    pub fn parse(raw: &'a [u8], endian: Endian, limits: Limits) -> Result<Self, Error> {
        if raw.len() < 4 {
            return Err(Error::Truncated);
        }
        let n = endian.u32(raw) as usize;
        count(n, limits)?;
        let expected = length(4, n, 24, 0)?;
        if raw.len() != expected {
            return Err(Error::Invalid("cue count or size"));
        }
        Ok(Self { raw, endian })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = CuePoint> + 'a {
        self.raw[4..]
            .as_chunks::<24>()
            .0
            .iter()
            .map(move |b| CuePoint {
                id: self.endian.u32(b),
                position: self.endian.u32(&b[4..]),
                data_chunk: b[8..12].try_into().unwrap(),
                chunk_start: self.endian.u32(&b[12..]),
                block_start: self.endian.u32(&b[16..]),
                sample_offset: self.endian.u32(&b[20..]),
            })
    }
}
pub fn write_cues(points: &[CuePoint], endian: Endian, out: &mut [u8]) -> Result<usize, Error> {
    let n = u32::try_from(points.len()).map_err(|_| Error::Limit("cue count"))?;
    let len = length(4, points.len(), 24, 0)?;
    let b = output(out, len)?;
    put32(b, n, endian);
    for (p, row) in points.iter().zip(b[4..].as_chunks_mut::<24>().0) {
        put32(row, p.id, endian);
        put32(&mut row[4..], p.position, endian);
        row[8..12].copy_from_slice(&p.data_chunk);
        put32(&mut row[12..], p.chunk_start, endian);
        put32(&mut row[16..], p.block_start, endian);
        put32(&mut row[20..], p.sample_offset, endian);
    }
    Ok(len)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct SamplerHeader {
    pub manufacturer: u32,
    pub product: u32,
    pub sample_period: u32,
    pub midi_unity_note: u32,
    pub midi_pitch_fraction: u32,
    pub smpte_format: u32,
    pub smpte_offset: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampleLoop {
    pub cue_id: u32,
    pub kind: u32,
    pub start: u32,
    pub end: u32,
    pub fraction: u32,
    pub play_count: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Sampler<'a> {
    raw: &'a [u8],
    loops_end: usize,
    endian: Endian,
}
impl<'a> Sampler<'a> {
    pub fn parse(raw: &'a [u8], endian: Endian, limits: Limits) -> Result<Self, Error> {
        if raw.len() < 36 {
            return Err(Error::Truncated);
        }
        let n = endian.u32(&raw[28..]) as usize;
        count(n, limits)?;
        let loops_end = length(36, n, 24, 0)?;
        let len = length(loops_end, 0, 0, endian.u32(&raw[32..]) as usize)?;
        if len != raw.len() {
            return Err(Error::Invalid("sampler count or data size"));
        }
        Ok(Self {
            raw,
            loops_end,
            endian,
        })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn header(self) -> SamplerHeader {
        let b = self.raw;
        let e = self.endian;
        SamplerHeader {
            manufacturer: e.u32(b),
            product: e.u32(&b[4..]),
            sample_period: e.u32(&b[8..]),
            midi_unity_note: e.u32(&b[12..]),
            midi_pitch_fraction: e.u32(&b[16..]),
            smpte_format: e.u32(&b[20..]),
            smpte_offset: e.u32(&b[24..]),
        }
    }
    pub fn loops(self) -> impl ExactSizeIterator<Item = SampleLoop> + 'a {
        self.raw[36..self.loops_end]
            .as_chunks::<24>()
            .0
            .iter()
            .map(move |b| SampleLoop {
                cue_id: self.endian.u32(b),
                kind: self.endian.u32(&b[4..]),
                start: self.endian.u32(&b[8..]),
                end: self.endian.u32(&b[12..]),
                fraction: self.endian.u32(&b[16..]),
                play_count: self.endian.u32(&b[20..]),
            })
    }
    pub fn specific_data(self) -> &'a [u8] {
        &self.raw[self.loops_end..]
    }
}
pub fn write_sampler(
    header: SamplerHeader,
    loops: &[SampleLoop],
    specific: &[u8],
    endian: Endian,
    out: &mut [u8],
) -> Result<usize, Error> {
    let n = u32::try_from(loops.len()).map_err(|_| Error::Limit("sampler loop count"))?;
    let specific_len =
        u32::try_from(specific.len()).map_err(|_| Error::Limit("sampler data size"))?;
    let len = length(36, loops.len(), 24, specific.len())?;
    let b = output(out, len)?;
    let fields = [
        header.manufacturer,
        header.product,
        header.sample_period,
        header.midi_unity_note,
        header.midi_pitch_fraction,
        header.smpte_format,
        header.smpte_offset,
        n,
        specific_len,
    ];
    for (v, row) in fields.iter().zip(b[..36].as_chunks_mut::<4>().0) {
        put32(row, *v, endian);
    }
    let end = 36 + loops.len() * 24;
    for (p, row) in loops.iter().zip(b[36..end].as_chunks_mut::<24>().0) {
        for (v, slot) in [p.cue_id, p.kind, p.start, p.end, p.fraction, p.play_count]
            .into_iter()
            .zip(row.as_chunks_mut::<4>().0)
        {
            put32(slot, v, endian);
        }
    }
    b[end..].copy_from_slice(specific);
    Ok(len)
}

/// Encode labl/note text (caller supplies its code page's bytes).
pub fn write_associated_text(
    cue_id: u32,
    text: &[u8],
    endian: Endian,
    out: &mut [u8],
) -> Result<usize, Error> {
    let len = length(5, 0, 0, text.len())?;
    let b = output(out, len)?;
    put32(b, cue_id, endian);
    b[4..len - 1].copy_from_slice(text);
    b[len - 1] = 0;
    Ok(len)
}
/// Encode ltxt fields with optional NUL-terminated text.
pub fn write_region_text(
    cue_id: u32,
    sample_length: u32,
    purpose: [u8; 4],
    locale: [u16; 4],
    text: &[u8],
    endian: Endian,
    out: &mut [u8],
) -> Result<usize, Error> {
    let len = length(
        20,
        0,
        0,
        text.len()
            .checked_add(usize::from(!text.is_empty()))
            .ok_or(Error::Limit("region text size"))?,
    )?;
    let b = output(out, len)?;
    put32(b, cue_id, endian);
    put32(&mut b[4..], sample_length, endian);
    b[8..12].copy_from_slice(&purpose);
    for (v, row) in locale.into_iter().zip(b[12..20].as_chunks_mut::<2>().0) {
        put16(row, v, endian);
    }
    if !text.is_empty() {
        b[20..len - 1].copy_from_slice(text);
        b[len - 1] = 0;
    }
    Ok(len)
}
