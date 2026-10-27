use crate::{Error, Limits, SideInfo, checksum};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpegVersion {
    Mpeg1,
    Mpeg2,
    Mpeg25,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMode {
    Stereo,
    JointStereo,
    DualChannel,
    Mono,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    word: u32,
    version: MpegVersion,
    sample_rate: u32,
    bitrate: u32,
    mode: ChannelMode,
}
impl FrameHeader {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; 4] = data.get(..4).ok_or(Error::Truncated)?.try_into().unwrap();
        let word = u32::from_be_bytes(bytes);
        if word & 0xffe00000 != 0xffe00000 {
            return Err(Error::Invalid("MPEG synchronization"));
        }
        let (version, divisor) = match (word >> 19) & 3 {
            3 => (MpegVersion::Mpeg1, 1),
            2 => (MpegVersion::Mpeg2, 2),
            0 => (MpegVersion::Mpeg25, 4),
            _ => return Err(Error::Invalid("reserved MPEG version")),
        };
        match (word >> 17) & 3 {
            1 => (),
            0 => return Err(Error::Invalid("reserved MPEG layer")),
            _ => return Err(Error::Unsupported("only MPEG Audio Layer III")),
        }
        let rate = (word >> 10) & 3;
        let index = (word >> 12) & 15;
        if rate == 3 || index == 15 {
            return Err(Error::Invalid("reserved MPEG header field"));
        }
        let bitrates = if version == MpegVersion::Mpeg1 {
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ]
        } else {
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
        };
        let mode = [
            ChannelMode::Stereo,
            ChannelMode::JointStereo,
            ChannelMode::DualChannel,
            ChannelMode::Mono,
        ][((word >> 6) & 3) as usize];
        Ok(Self {
            word,
            version,
            sample_rate: [44100, 48000, 32000][rate as usize] / divisor,
            bitrate: bitrates[index as usize] * 1000,
            mode,
        })
    }
    pub fn to_bytes(self) -> [u8; 4] {
        self.word.to_be_bytes()
    }
    pub fn version(self) -> MpegVersion {
        self.version
    }
    pub fn sample_rate(self) -> u32 {
        self.sample_rate
    }
    pub fn bitrate(self) -> Option<u32> {
        (self.bitrate != 0).then_some(self.bitrate)
    }
    pub fn channel_mode(self) -> ChannelMode {
        self.mode
    }
    pub fn channels(self) -> usize {
        if self.mode == ChannelMode::Mono { 1 } else { 2 }
    }
    pub fn granules(self) -> usize {
        if self.version == MpegVersion::Mpeg1 {
            2
        } else {
            1
        }
    }
    pub fn samples(self) -> usize {
        self.granules() * 576
    }
    pub fn has_crc(self) -> bool {
        self.word & (1 << 16) == 0
    }
    pub fn padding(self) -> usize {
        ((self.word >> 9) & 1) as usize
    }
    pub fn mode_extension(self) -> u8 {
        ((self.word >> 4) & 3) as u8
    }
    pub fn emphasis(self) -> u8 {
        (self.word & 3) as u8
    }
    pub fn private(self) -> bool {
        self.word & (1 << 8) != 0
    }
    pub fn copyright(self) -> bool {
        self.word & 8 != 0
    }
    pub fn original(self) -> bool {
        self.word & 4 != 0
    }
    pub fn side_info_bytes(self) -> usize {
        match (self.version, self.channels()) {
            (MpegVersion::Mpeg1, 1) => 17,
            (MpegVersion::Mpeg1, _) => 32,
            (_, 1) => 9,
            _ => 17,
        }
    }
    pub fn main_data_offset(self) -> usize {
        4 + usize::from(self.has_crc()) * 2 + self.side_info_bytes()
    }
    pub fn frame_bytes(self) -> Option<usize> {
        self.bitrate().map(|rate| {
            ((if self.version == MpegVersion::Mpeg1 {
                144
            } else {
                72
            }) * rate
                / self.sample_rate) as usize
                + self.padding()
        })
    }
    pub fn compatible(self, other: Self) -> bool {
        self.version == other.version && self.sample_rate == other.sample_rate
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeFormat {
    unpadded_bytes: usize,
}
impl FreeFormat {
    pub fn new(unpadded_bytes: usize, limits: Limits) -> Result<Self, Error> {
        if unpadded_bytes < 13 {
            return Err(Error::Invalid("free-format frame size"));
        }
        if unpadded_bytes > limits.frame_bytes {
            return Err(Error::LimitExceeded("free-format frame"));
        }
        Ok(Self { unpadded_bytes })
    }
    pub fn unpadded_bytes(self) -> usize {
        self.unpadded_bytes
    }
    /// Infer a fixed unpadded length using three compatible free-format headers.
    /// A lone/final frame needs a size already established by the caller.
    pub fn infer(data: &[u8], limits: Limits) -> Result<Self, Error> {
        let first = FrameHeader::parse(data)?;
        if first.bitrate().is_some() {
            return Err(Error::Invalid("not free format"));
        }
        let end = data.len().saturating_sub(3).min(limits.frame_bytes);
        for next in first.main_data_offset()..end {
            if data[next] != 0xff {
                continue;
            }
            let Ok(second) = FrameHeader::parse(&data[next..]) else {
                continue;
            };
            if second.bitrate().is_some() || !first.compatible(second) {
                continue;
            }
            let base = next - first.padding();
            let third = next
                .checked_add(base)
                .and_then(|n| n.checked_add(second.padding()))
                .ok_or(Error::Invalid("free-format size overflow"))?;
            let Some(tail) = data.get(third..) else {
                continue;
            };
            let Ok(last) = FrameHeader::parse(tail) else {
                continue;
            };
            if last.bitrate().is_none() && first.compatible(last) {
                return Self::new(base, limits);
            }
        }
        if data.len() < limits.frame_bytes.saturating_mul(2).saturating_add(4) {
            Err(Error::Truncated)
        } else {
            Err(Error::Invalid("free-format synchronization"))
        }
    }
}

pub struct Frame<'a> {
    pub header: FrameHeader,
    pub side_info: SideInfo,
    pub data: &'a [u8],
    pub main_data: &'a [u8],
    pub crc: Option<u16>,
}
impl<'a> Frame<'a> {
    pub fn parse(
        data: &'a [u8],
        free: Option<FreeFormat>,
        verify_crc: bool,
        limits: Limits,
    ) -> Result<Self, Error> {
        let header = FrameHeader::parse(data)?;
        let length = match header.frame_bytes() {
            Some(bytes) => bytes,
            None => free
                .ok_or(Error::Unsupported("free-format frame length required"))?
                .unpadded_bytes
                .checked_add(header.padding())
                .ok_or(Error::Invalid("free-format size overflow"))?,
        };
        if length > limits.frame_bytes {
            return Err(Error::LimitExceeded("MPEG frame"));
        }
        if length < header.main_data_offset() {
            return Err(Error::Invalid("frame smaller than side information"));
        }
        let data = data.get(..length).ok_or(Error::Truncated)?;
        let start = 4 + usize::from(header.has_crc()) * 2;
        let side = &data[start..header.main_data_offset()];
        let crc = header
            .has_crc()
            .then(|| u16::from_be_bytes([data[4], data[5]]));
        if verify_crc && let Some(expected) = crc {
            let actual = checksum::bytes(checksum::update(0xffff, header.word & 0xffff, 16), side);
            if expected != actual {
                return Err(Error::CrcMismatch { expected, actual });
            }
        }
        Ok(Self {
            header,
            side_info: SideInfo::parse(header, side)?,
            data,
            main_data: &data[header.main_data_offset()..],
            crc,
        })
    }
    /// Finds a complete frame with checked side information and, when present,
    /// CRC. Layer III CRC does not protect Huffman/main data. No PCM is decoded.
    pub fn find(
        data: &'a [u8],
        free: Option<FreeFormat>,
        verify_crc: bool,
        limits: Limits,
    ) -> Result<(usize, Self), Error> {
        let end = data.len().saturating_sub(3).min(limits.sync_bytes);
        for offset in 0..end {
            if data[offset] != 0xff {
                continue;
            }
            if let Ok(frame) = Self::parse(&data[offset..], free, verify_crc, limits) {
                return Ok((offset, frame));
            }
        }
        if data.len() <= limits.sync_bytes {
            Err(Error::Truncated)
        } else {
            Err(Error::LimitExceeded("MPEG synchronization"))
        }
    }
}
