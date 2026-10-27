//! Structural validation CLI; does not decode PCM.
use deadlib_audio_decode_mp3::{
    tags::{Id3Tag, Id3v1, VbrHeader},
    *,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("inspect INPUT.mp3");
    let data = std::fs::read(path)?;
    let limits = Limits::default();
    let mut offset = 0;
    while data.get(offset..offset + 3) == Some(b"ID3".as_slice()) {
        offset += Id3Tag::parse(&data[offset..], limits)?.bytes;
    }
    let end = if Id3v1::parse(&data).is_some() {
        data.len() - 128
    } else {
        data.len()
    };
    let mut free = None;
    let mut first = true;
    let mut reservoir = Reservoir::new();
    while offset < end {
        let bytes = &data[offset..end];
        let header = FrameHeader::parse(bytes)?;
        if header.bitrate().is_none() && free.is_none() {
            free = Some(FreeFormat::infer(bytes, limits)?);
        }
        let frame = Frame::parse(bytes, free, true, limits)?;
        println!(
            "frame\t{offset}\t{}\t{}\t{}\t{}\t{}",
            header.bitrate().unwrap_or(0),
            header.sample_rate(),
            header.channels(),
            header.samples(),
            frame.data.len()
        );
        if first && let Some(VbrHeader::Xing(tag)) = VbrHeader::parse(header, frame.data)? {
            println!(
                "xing\t{}\t{}\t{}",
                tag.frames.unwrap_or(0),
                tag.gapless.map_or(0, |g| g.encoder_delay),
                tag.gapless.map_or(0, |g| g.encoder_padding)
            );
        }
        reservoir.push(
            frame.side_info.main_data_begin as usize,
            frame.main_data,
            frame.side_info.main_data_bits(header),
        )?;
        offset += frame.data.len();
        first = false;
    }
    Ok(())
}
