pub(crate) const PACKET_BYTES: u32 = 64 << 20;
pub(crate) const MAX_SAMPLES: usize = 1_000_000;

pub(crate) struct Track {
    pub(crate) codec: Codec,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) packets: Vec<Packet>,
    pub(crate) presentation_times: Vec<i64>,
    pub(crate) timescale: u32,
    pub(crate) duration: u64,
    pub(crate) reported_duration: Option<u64>,
    pub(crate) decode_duration: u64,
    pub(crate) color: Option<Color>,
}

pub(crate) enum Codec {
    Avc(Vec<u8>),
    AnnexBAvc(Vec<u8>),
    Hevc(Vec<u8>),
    Vp9,
    Mpeg1,
    Mpeg4 { config: Vec<u8>, avi: bool },
    MsMpeg4v2,
}

#[derive(Clone, Copy)]
pub(crate) struct Color {
    pub(crate) matrix: u16,
    pub(crate) full_range: Option<bool>,
}

#[derive(Clone, Copy)]
pub(crate) struct Packet {
    pub(crate) offset: u64,
    pub(crate) size: u32,
}
