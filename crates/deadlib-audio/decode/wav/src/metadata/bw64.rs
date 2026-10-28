use super::{count, length, output};
use crate::{
    Endian, Error, Limits,
    mux::{put16, put32},
};
const LE: Endian = Endian::Little;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioId<'a> {
    pub track_index: u16,
    pub uid: &'a [u8; 12],
    pub track_ref: &'a [u8; 14],
    pub pack_ref: &'a [u8; 11],
    pub padding: u8,
}
impl AudioId<'_> {
    pub const UNUSED: AudioId<'static> = AudioId {
        track_index: 0,
        uid: &[0; 12],
        track_ref: &[0; 14],
        pack_ref: &[0; 11],
        padding: 0,
    };
    pub fn validate_ids(self) -> Result<(), Error> {
        if self.track_index == 0 {
            if self
                .uid
                .iter()
                .chain(self.track_ref)
                .chain(self.pack_ref)
                .any(|&v| v != 0)
                || self.padding != 0
            {
                return Err(Error::Invalid("nonzero unused CHNA slot"));
            }
            return Ok(());
        }
        let hex = |b: &[u8]| b.iter().all(u8::is_ascii_hexdigit);
        if &self.uid[..4] != b"ATU_" || !hex(&self.uid[4..]) {
            return Err(Error::Invalid("CHNA track UID"));
        }
        if !((&self.track_ref[..3] == b"AT_" || &self.track_ref[..3] == b"AC_")
            && hex(&self.track_ref[3..11])
            && self.track_ref[11] == b'_'
            && hex(&self.track_ref[12..]))
            || (&self.track_ref[..3] == b"AC_" && &self.track_ref[12..] != b"00")
        {
            return Err(Error::Invalid("CHNA track reference"));
        }
        if !self.pack_ref.iter().all(|&v| v == 0)
            && !(&self.pack_ref[..3] == b"AP_" && hex(&self.pack_ref[3..]))
        {
            return Err(Error::Invalid("CHNA pack reference"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Chna<'a> {
    raw: &'a [u8],
}
impl<'a> Chna<'a> {
    pub fn parse(raw: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if raw.len() < 4 {
            return Err(Error::Truncated);
        }
        if !(raw.len() - 4).is_multiple_of(40) {
            return Err(Error::Invalid("CHNA slot size"));
        }
        let slots = (raw.len() - 4) / 40;
        count(slots, limits)?;
        let v = Self { raw };
        if usize::from(v.num_uids()) > slots {
            return Err(Error::Invalid("CHNA UID count"));
        }
        let mut used = 0usize;
        for id in v.ids() {
            if id.track_index == 0 {
                id.validate_ids()?;
            } else {
                used += 1;
            }
        }
        if used != usize::from(v.num_uids())
            || v.num_tracks() > v.num_uids()
            || (used != 0 && v.num_tracks() == 0)
        {
            return Err(Error::Invalid("CHNA header counts"));
        }
        Ok(v)
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn num_tracks(self) -> u16 {
        LE.u16(self.raw)
    }
    pub fn num_uids(self) -> u16 {
        LE.u16(&self.raw[2..])
    }
    /// All slots, including unused ones, in their physical order.
    pub fn ids(self) -> impl ExactSizeIterator<Item = AudioId<'a>> {
        self.raw[4..].as_chunks::<40>().0.iter().map(|b| AudioId {
            track_index: LE.u16(b),
            uid: b[2..14].try_into().unwrap(),
            track_ref: b[14..28].try_into().unwrap(),
            pack_ref: b[28..39].try_into().unwrap(),
            padding: b[39],
        })
    }
    /// Validate track bounds/counts and ID syntax using a caller-owned bitmap.
    /// This does not resolve ADM references or check global UID uniqueness.
    pub fn validate_assignments(self, channels: u16, scratch: &mut [u8]) -> Result<(), Error> {
        let needed = usize::from(channels).div_ceil(8);
        let bitmap = output(scratch, needed)?;
        bitmap.fill(0);
        let mut tracks = 0u16;
        for id in self.ids() {
            id.validate_ids()?;
            if id.track_index == 0 {
                continue;
            }
            if id.track_index > channels {
                return Err(Error::Invalid("CHNA track index exceeds channels"));
            }
            let i = usize::from(id.track_index - 1);
            let bit = 1u8 << (i & 7);
            if bitmap[i / 8] & bit == 0 {
                tracks += 1;
                bitmap[i / 8] |= bit;
            }
        }
        if tracks != self.num_tracks() {
            return Err(Error::Invalid("CHNA distinct track count"));
        }
        Ok(())
    }
}
pub fn write_chna(
    num_tracks: u16,
    ids: &[AudioId<'_>],
    reserved_slots: usize,
    out: &mut [u8],
) -> Result<usize, Error> {
    if reserved_slots < ids.len() {
        return Err(Error::Invalid("CHNA slot capacity"));
    }
    let mut used = 0u16;
    for &id in ids {
        id.validate_ids()?;
        if id.track_index != 0 {
            used = used.checked_add(1).ok_or(Error::Limit("CHNA UID count"))?;
        }
    }
    if num_tracks > used || (used != 0 && num_tracks == 0) {
        return Err(Error::Invalid("CHNA header counts"));
    }
    let len = length(4, reserved_slots, 40, 0)?;
    let b = output(out, len)?;
    b.fill(0);
    put16(b, num_tracks, LE);
    put16(&mut b[2..], used, LE);
    for (id, row) in ids.iter().zip(b[4..].as_chunks_mut::<40>().0) {
        put16(row, id.track_index, LE);
        row[2..14].copy_from_slice(id.uid);
        row[14..28].copy_from_slice(id.track_ref);
        row[28..39].copy_from_slice(id.pack_ref);
        row[39] = id.padding;
    }
    Ok(len)
}

#[derive(Clone, Copy, Debug)]
pub struct Bxml<'a> {
    raw: &'a [u8],
}
impl<'a> Bxml<'a> {
    pub fn parse(raw: &'a [u8]) -> Result<Self, Error> {
        if raw.len() < 2 {
            return Err(Error::Truncated);
        }
        Ok(Self { raw })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    /// 0 = uncompressed, 1 = gzip. Unknown methods are retained.
    pub fn compression(self) -> u16 {
        LE.u16(self.raw)
    }
    pub fn data(self) -> &'a [u8] {
        &self.raw[2..]
    }
}
pub fn write_bxml(compression: u16, data: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    let len = length(2, 0, 0, data.len())?;
    let b = output(out, len)?;
    put16(b, compression, LE);
    b[2..].copy_from_slice(data);
    Ok(len)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlignmentPoint {
    pub byte_offset: u64,
    pub sample_count: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct SubXmlChunk<'a> {
    pub byte_offset: u64,
    pub samples: u32,
    pub data: &'a [u8],
}
#[derive(Clone, Copy, Debug)]
pub struct Sxml<'a> {
    raw: &'a [u8],
    table_end: usize,
    subchunks: u32,
    align_start: usize,
}
impl<'a> Sxml<'a> {
    pub fn parse(raw: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if raw.len() < 14 {
            return Err(Error::Truncated);
        }
        let table = LE.u64(&raw[2..]);
        if table < 4 {
            return Err(Error::Invalid("SXML table size"));
        }
        let end = table
            .checked_add(10)
            .ok_or(Error::Invalid("SXML table overflow"))?;
        if end > raw.len() as u64 {
            return Err(Error::Truncated);
        }
        let table_end = end as usize;
        let subchunks = LE.u32(&raw[10..]);
        count(subchunks as usize, limits)?;
        let mut pos = 14usize;
        for _ in 0..subchunks {
            let head = raw
                .get(
                    pos..pos
                        .checked_add(8)
                        .ok_or(Error::Invalid("SXML offset overflow"))?,
                )
                .ok_or(Error::Truncated)?;
            pos = length(pos, 1, 8, LE.u32(head) as usize)?;
            if pos > table_end {
                return Err(Error::Truncated);
            }
        }
        if pos != table_end {
            return Err(Error::Invalid("SXML sub-chunk count or size"));
        }
        let align_start = if table_end == raw.len() {
            table_end
        } else {
            let head = raw.get(table_end..table_end + 4).ok_or(Error::Truncated)?;
            let n = LE.u32(head) as usize;
            count(n, limits)?;
            if length(table_end, 1, 4, length(0, n, 16, 0)?)? != raw.len() {
                return Err(Error::Invalid("SXML alignment count or size"));
            }
            table_end + 4
        };
        Ok(Self {
            raw,
            table_end,
            subchunks,
            align_start,
        })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn compression(self) -> u16 {
        LE.u16(self.raw)
    }
    pub fn subchunks(self) -> impl ExactSizeIterator<Item = SubXmlChunk<'a>> {
        let mut pos = 14usize;
        (0..self.subchunks).map(move |_| {
            let start = pos;
            let n = LE.u32(&self.raw[pos..]) as usize;
            pos += 8 + n;
            SubXmlChunk {
                byte_offset: start as u64,
                samples: LE.u32(&self.raw[start + 4..]),
                data: &self.raw[start + 8..pos],
            }
        })
    }
    pub fn alignment_points(self) -> impl ExactSizeIterator<Item = AlignmentPoint> + 'a {
        self.raw[self.align_start..]
            .as_chunks::<16>()
            .0
            .iter()
            .map(|b| AlignmentPoint {
                byte_offset: LE.u64(b),
                sample_count: LE.u64(&b[8..]),
            })
    }
    pub fn subchunk_at(self, offset: u64) -> Option<SubXmlChunk<'a>> {
        if offset < 14 || offset >= self.table_end as u64 {
            return None;
        }
        self.subchunks().find(|v| v.byte_offset == offset)
    }
}
/// Write packed SubXMLChunk records (no inner RIFF padding) and an alignment
/// table. XML bytes and compression are supplied by the caller without conversion.
pub fn write_sxml(
    compression: u16,
    subchunks: &[(u32, &[u8])],
    alignment: &[AlignmentPoint],
    out: &mut [u8],
) -> Result<usize, Error> {
    let n = u32::try_from(subchunks.len()).map_err(|_| Error::Limit("SXML sub-chunk count"))?;
    let align_count =
        u32::try_from(alignment.len()).map_err(|_| Error::Limit("SXML alignment count"))?;
    let mut table = 4usize;
    for (_, data) in subchunks {
        u32::try_from(data.len()).map_err(|_| Error::Limit("SXML sub-chunk size"))?;
        table = length(table, 1, 8, data.len())?;
    }
    let len = length(14, alignment.len(), 16, table)?;
    let b = output(out, len)?;
    put16(b, compression, LE);
    b[2..10].copy_from_slice(&(table as u64).to_le_bytes());
    put32(&mut b[10..], n, LE);
    let mut pos = 14;
    for &(samples, data) in subchunks {
        put32(&mut b[pos..], data.len() as u32, LE);
        put32(&mut b[pos + 4..], samples, LE);
        pos += 8;
        b[pos..pos + data.len()].copy_from_slice(data);
        pos += data.len();
    }
    put32(&mut b[pos..], align_count, LE);
    pos += 4;
    for p in alignment {
        b[pos..pos + 8].copy_from_slice(&p.byte_offset.to_le_bytes());
        b[pos + 8..pos + 16].copy_from_slice(&p.sample_count.to_le_bytes());
        pos += 16;
    }
    Ok(len)
}
