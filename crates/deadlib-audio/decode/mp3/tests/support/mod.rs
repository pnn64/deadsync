use deadlib_audio_decode_mp3::{bits::BitWriter, *};
pub fn protect(mut bytes: Vec<u8>) -> Vec<u8> {
    let length = bytes.len();
    bytes.copy_within(4..length - 2, 6);
    bytes[1] &= 0xfe;
    let header = FrameHeader::parse(&bytes).unwrap();
    let crc = checksum::bytes(
        checksum::bytes(0xffff, &bytes[2..4]),
        &bytes[6..header.main_data_offset()],
    );
    bytes[4..6].copy_from_slice(&crc.to_be_bytes());
    bytes
}
pub fn frame(
    version: u32,
    rate: u32,
    channels: usize,
    gain: u8,
    negative: bool,
    back: u32,
) -> Vec<u8> {
    let word = 0xffe00000
        | version << 19
        | 1 << 17
        | 1 << 16
        | 9 << 12
        | rate << 10
        | if channels == 1 { 3 << 6 } else { 0 };
    let h = FrameHeader::parse(&word.to_be_bytes()).unwrap();
    let mut side = BitWriter::new();
    side.write(back, if version == 3 { 9 } else { 8 }).unwrap();
    side.write(
        0,
        match (version == 3, channels) {
            (true, 1) => 5,
            (true, _) => 3,
            (false, 1) => 1,
            _ => 2,
        },
    )
    .unwrap();
    if version == 3 {
        for _ in 0..channels {
            side.write(0, 4).unwrap();
        }
    }
    for granule in 0..h.granules() {
        for _ in 0..channels {
            for (v, n) in [
                (if granule == 0 { 3 } else { 0 }, 12),
                (u32::from(granule == 0), 9),
                (gain as u32, 8),
                (0, if version == 3 { 4 } else { 9 }),
                (0, 1),
            ] {
                side.write(v, n).unwrap();
            }
            for _ in 0..3 {
                side.write(1, 5).unwrap();
            }
            side.write(0, 4).unwrap();
            side.write(0, 3).unwrap();
            if version == 3 {
                side.write(0, 1).unwrap();
            }
            side.write(0, 1).unwrap();
            side.write(0, 1).unwrap();
        }
    }
    let mut bytes = vec![0; h.frame_bytes().unwrap()];
    bytes[..4].copy_from_slice(&word.to_be_bytes());
    bytes[4..h.main_data_offset()].copy_from_slice(side.as_bytes());
    let mut main = BitWriter::new();
    for c in 0..channels {
        main.write(1, 2).unwrap();
        main.write(u32::from(negative ^ (c == 1)), 1).unwrap();
    }
    bytes[h.main_data_offset()..h.main_data_offset() + main.as_bytes().len()]
        .copy_from_slice(main.as_bytes());
    bytes
}
pub fn info(
    version: u32,
    rate: u32,
    channels: usize,
    frames: u32,
    delay: u16,
    padding: u16,
) -> Vec<u8> {
    let mut bytes = frame(version, rate, channels, 206, false, 0);
    let h = FrameHeader::parse(&bytes).unwrap();
    bytes[4..].fill(0);
    let start = 4 + h.side_info_bytes();
    bytes[start..start + 4].copy_from_slice(b"Info");
    bytes[start + 4..start + 8].copy_from_slice(&1u32.to_be_bytes());
    bytes[start + 8..start + 12].copy_from_slice(&frames.to_be_bytes());
    let lame = start + 12;
    bytes[lame..lame + 9].copy_from_slice(b"LAME3.100");
    let value = (u32::from(delay) << 12) | u32::from(padding);
    bytes[lame + 21..lame + 24].copy_from_slice(&value.to_be_bytes()[1..]);
    bytes
}
