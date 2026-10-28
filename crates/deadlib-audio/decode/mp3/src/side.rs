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
        let channels = header.channels();
        let private_width = match (v1, channels) {
            (true, 1) => 5,
            (true, _) => 3,
            (false, 1) => 1,
            _ => 2,
        };
        let scfsi_width = if v1 { channels as u32 * 4 } else { 0 };
        let prefix = bits.read(if v1 { 9 } else { 8 } + private_width + scfsi_width)?;
        let mut info = Self {
            main_data_begin: (prefix >> (private_width + scfsi_width)) as u16,
            private: ((prefix >> scfsi_width) & ((1 << private_width) - 1)) as u8,
            ..Self::default()
        };
        if v1 {
            for ch in 0..channels {
                let flags = prefix >> (4 * (channels - 1 - ch));
                info.scfsi[ch] = [
                    flags & 8 != 0,
                    flags & 4 != 0,
                    flags & 2 != 0,
                    flags & 1 != 0,
                ];
            }
        }
        for gr in 0..header.granules() {
            for ch in 0..channels {
                let g = &mut info.granules[gr][ch];
                // The complete side-info slice is present; every granule has
                // 59 MPEG-1 bits or 63 LSF bits, regardless of block shape.
                let prefix = bits.read(29)?;
                g.part2_3_bits = (prefix >> 17) as u16;
                g.big_values = ((prefix >> 8) & 511) as u16;
                if g.big_values > 288 {
                    return Err(Error::Invalid("Layer III big_values"));
                }
                g.global_gain = prefix as u8;
                let compression = bits.read(if v1 { 5 } else { 10 })?;
                g.scalefac_compress = (compression >> 1) as u16;
                let fields = bits.read(if v1 { 25 } else { 24 })?;
                let body = fields >> (2 + u32::from(v1));
                if compression & 1 != 0 {
                    g.block_type = (body >> 20) as u8;
                    g.mixed_block = body & (1 << 19) != 0;
                    if g.block_type == 0 {
                        return Err(Error::Invalid("Layer III switched block"));
                    }
                    g.table_select[..2]
                        .copy_from_slice(&[((body >> 14) & 31) as u8, ((body >> 9) & 31) as u8]);
                    g.subblock_gain = [
                        ((body >> 6) & 7) as u8,
                        ((body >> 3) & 7) as u8,
                        (body & 7) as u8,
                    ];
                    g.region0_count = if g.block_type == 2 && !g.mixed_block {
                        8
                    } else {
                        7
                    };
                    g.region1_count = 20 - g.region0_count;
                } else {
                    g.table_select = [
                        (body >> 17) as u8,
                        ((body >> 12) & 31) as u8,
                        ((body >> 7) & 31) as u8,
                    ];
                    g.region0_count = ((body >> 3) & 15) as u8;
                    g.region1_count = (body & 7) as u8;
                }
                // Table identifiers only matter for regions with coded pairs.
                // LAME's CRC-protected Info frame can put marker bytes in the
                // final side-info bits of an otherwise empty granule. Preserve
                // unused selectors; reject reserved books when actually used.
                g.preflag = v1 && fields & 4 != 0;
                g.scalefac_scale = fields & 2 != 0;
                g.count1_table = fields & 1 != 0;
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
