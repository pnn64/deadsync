use super::{count, length, nul_terminated, output};
use crate::{Endian, Error, Limits, mux::put32};
const LE: Endian = Endian::Little;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Marker<'a> {
    pub flags: u32,
    pub sample_offset: u64,
    pub byte_offset: u64,
    /// Physical DWORDs at entry offsets 20/24. The 2009 specification's field
    /// names and its low/high comment disagree; preserve these words verbatim.
    pub intra_sample_words: [u32; 2],
    pub label: &'a [u8; 256],
    pub label_chunk_id: u32,
    pub vendor_product_guid: &'a [u8; 16],
    pub user_data: [u32; 4],
}
impl<'a> Marker<'a> {
    pub fn is_valid(self) -> bool {
        self.flags & 1 != 0
    }
    pub fn valid_byte_offset(self) -> Option<u64> {
        (self.flags & 2 != 0).then_some(self.byte_offset)
    }
    pub fn inline_label(self) -> Option<&'a [u8]> {
        (self.flags & 8 == 0).then(|| nul_terminated(self.label))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Markers<'a> {
    raw: &'a [u8],
}
impl<'a> Markers<'a> {
    pub fn parse(raw: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if !raw.len().is_multiple_of(320) {
            return Err(Error::Invalid("RF64 marker entry size"));
        }
        count(raw.len() / 320, limits)?;
        Ok(Self { raw })
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = Marker<'a>> {
        self.raw.as_chunks::<320>().0.iter().map(|b| Marker {
            flags: LE.u32(b),
            sample_offset: LE.u64(&b[4..]),
            byte_offset: LE.u64(&b[12..]),
            intra_sample_words: [LE.u32(&b[20..]), LE.u32(&b[24..])],
            label: b[28..284].try_into().unwrap(),
            label_chunk_id: LE.u32(&b[284..]),
            vendor_product_guid: b[288..304].try_into().unwrap(),
            user_data: [
                LE.u32(&b[304..]),
                LE.u32(&b[308..]),
                LE.u32(&b[312..]),
                LE.u32(&b[316..]),
            ],
        })
    }
}
pub fn write_markers(markers: &[Marker<'_>], out: &mut [u8]) -> Result<usize, Error> {
    let len = length(0, markers.len(), 320, 0)?;
    for &m in markers {
        if m.is_valid() && m.flags & 8 == 0 {
            if !m.label.contains(&0) {
                return Err(Error::Invalid("RF64 marker label termination"));
            }
            if m.flags & 16 != 0 && std::str::from_utf8(nul_terminated(m.label)).is_err() {
                return Err(Error::Invalid("RF64 UTF-8 marker label"));
            }
        }
    }
    let b = output(out, len)?;
    for (m, row) in markers.iter().zip(b.as_chunks_mut::<320>().0) {
        put32(row, m.flags, LE);
        row[4..12].copy_from_slice(&m.sample_offset.to_le_bytes());
        row[12..20].copy_from_slice(&m.byte_offset.to_le_bytes());
        put32(&mut row[20..], m.intra_sample_words[0], LE);
        put32(&mut row[24..], m.intra_sample_words[1], LE);
        row[28..284].copy_from_slice(m.label);
        put32(&mut row[284..], m.label_chunk_id, LE);
        row[288..304].copy_from_slice(m.vendor_product_guid);
        for (v, slot) in m.user_data.iter().zip(row[304..].as_chunks_mut::<4>().0) {
            put32(slot, *v, LE);
        }
    }
    Ok(len)
}
