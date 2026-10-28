use crate::{Error, FrameHeader, MpegVersion, bits::BitReader};

#[derive(Clone, Copy, Debug, Default)]
pub struct GranuleChannel {
    pub part2_3_bits: u16,
    pub big_values: u16,
    pub global_gain: u8,
    pub scalefac_compress: u16,
    pub block_type: u8,
    pub mixed_block: bool,
    pub table_select: [u8; 3],
    pub subblock_gain: [u8; 3],
    pub region0_count: u8,
    pub region1_count: u8,
    pub preflag: bool,
    pub scalefac_scale: bool,
    pub count1_table: bool,
}
#[derive(Clone, Debug, Default)]
pub struct SideInfo {
    pub main_data_begin: u16,
    pub private: u8,
    pub scfsi: [[bool; 4]; 2],
    pub granules: [[GranuleChannel; 2]; 2],
}
impl SideInfo {
    pub fn parse(header: FrameHeader, data: &[u8]) -> Result<Self, Error> {
        let data = data
            .get(..header.side_info_bytes())
            .ok_or(Error::Truncated)?;
        let mut bits = BitReader::new(data);
        let v1 = header.version() == MpegVersion::Mpeg1;
        let mut info = Self {
            main_data_begin: bits.read(if v1 { 9 } else { 8 })? as u16,
            private: bits.read(match (v1, header.channels()) {
                (true, 1) => 5,
                (true, _) => 3,
                (false, 1) => 1,
                _ => 2,
            })? as u8,
            ..Self::default()
        };
        if v1 {
            for ch in 0..header.channels() {
                for flag in &mut info.scfsi[ch] {
                    *flag = bits.flag()?;
                }
            }
        }
        for gr in 0..header.granules() {
            for ch in 0..header.channels() {
                let g = &mut info.granules[gr][ch];
                g.part2_3_bits = bits.read(12)? as u16;
                g.big_values = bits.read(9)? as u16;
                if g.big_values > 288 {
                    return Err(Error::Invalid("Layer III big_values"));
                }
                g.global_gain = bits.read(8)? as u8;
                g.scalefac_compress = bits.read(if v1 { 4 } else { 9 })? as u16;
                if bits.flag()? {
                    g.block_type = bits.read(2)? as u8;
                    g.mixed_block = bits.flag()?;
                    if g.block_type == 0 {
                        return Err(Error::Invalid("Layer III switched block"));
                    }
                    for t in &mut g.table_select[..2] {
                        *t = bits.read(5)? as u8;
                    }
                    for s in &mut g.subblock_gain {
                        *s = bits.read(3)? as u8;
                    }
                    g.region0_count = if g.block_type == 2 && !g.mixed_block {
                        8
                    } else {
                        7
                    };
                    g.region1_count = 20 - g.region0_count;
                } else {
                    for t in &mut g.table_select {
                        *t = bits.read(5)? as u8;
                    }
                    g.region0_count = bits.read(4)? as u8;
                    g.region1_count = bits.read(3)? as u8;
                }
                // Table identifiers only matter for regions with coded pairs.
                // LAME's CRC-protected Info frame can put marker bytes in the
                // final side-info bits of an otherwise empty granule. Preserve
                // unused selectors; reject reserved books when actually used.
                g.preflag = v1 && bits.flag()?;
                g.scalefac_scale = bits.flag()?;
                g.count1_table = bits.flag()?;
            }
        }
        debug_assert_eq!(bits.remaining(), 0);
        Ok(info)
    }
    pub fn main_data_bits(&self, header: FrameHeader) -> usize {
        self.granules[..header.granules()]
            .iter()
            .flat_map(|g| &g[..header.channels()])
            .map(|g| g.part2_3_bits as usize)
            .sum()
    }
}
