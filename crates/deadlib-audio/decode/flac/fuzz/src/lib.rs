use deadlib_audio_decode_flac::*;
use std::io::{Cursor, Read};
mod ogg;
pub use ogg::{ogg, repair_ogg};
mod encode;
pub use encode::{encode, encode_input};
pub const MAX_INPUT: usize = 131072;
pub fn options() -> DecodeOptions {
    DecodeOptions {
        limits: Limits {
            metadata_bytes: MAX_INPUT,
            metadata_blocks: 256,
            frame_bytes: MAX_INPUT,
        },
        ..Default::default()
    }
}
#[derive(Debug, PartialEq)]
pub struct OwnedFrame {
    pub start: u64,
    pub header: FrameHeader,
    pub planes: Vec<Vec<i32>>,
}
impl OwnedFrame {
    fn new(start: u64, header: FrameHeader, pcm: Pcm<'_>) -> Self {
        Self {
            start,
            header,
            planes: pcm.planes().map(|p| p.to_vec()).collect(),
        }
    }
    fn compare(&self, start: u64, header: FrameHeader, pcm: Pcm<'_>) {
        assert_eq!(header, self.header);
        let skip = (start - self.start) as usize;
        for (a, b) in pcm.planes().zip(&self.planes) {
            assert_eq!(a, &b[skip..]);
        }
        assert_eq!(pcm.channels() as usize, self.planes.len());
    }
}
pub fn primitives(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    if let Ok(metadata) = Metadata::parse(data, options().limits) {
        assert_eq!(metadata.encode().unwrap(), data[..metadata.audio_offset]);
        assert_eq!(
            StreamInfo::parse(&metadata.stream_info.encode().unwrap()).unwrap(),
            metadata.stream_info
        );
    }
    if let Ok((block, last, used)) = MetadataBlock::parse(data) {
        let _ = block.validate();
        let mut bytes = Vec::new();
        block.write(last, &mut bytes).unwrap();
        assert_eq!(bytes, data[..used]);
    }
    use bitstream::{BitReader, BitWriter};
    let mut reader = BitReader::new(data);
    let mut writer = BitWriter::new();
    let mut position = 0;
    for &control in data.iter().take(256) {
        let n = (control % 57) as usize;
        if position + n > data.len() * 8 {
            assert!(reader.read(n as u32).is_err());
            break;
        }
        let mut expected = 0;
        for p in position..position + n {
            expected = (expected << 1) | ((data[p / 8] >> (7 - p % 8)) & 1) as u64;
        }
        assert_eq!(reader.read(n as u32).unwrap(), expected);
        writer.write(expected, n as u32).unwrap();
        position += n;
        assert_eq!(reader.position(), position);
    }
    let complete = position / 8;
    assert_eq!(&writer.as_bytes()[..complete], &data[..complete]);
    let mut digest = checksum::Md5::new();
    digest.update(data);
    let expected = digest.finalize();
    let mut digest = checksum::Md5::new();
    for chunk in data.chunks(1 + data.first().copied().unwrap_or(0) as usize) {
        digest.update(chunk);
    }
    assert_eq!(digest.finalize(), expected);
}
pub fn frame(data: &[u8]) {
    if data.len() < 34 || data.len() > MAX_INPUT {
        return;
    }
    let info = StreamInfo::parse(&data[..34]).ok();
    let mut decoder = FrameDecoder::new(info).unwrap();
    let first = decoder
        .decode(&data[34..])
        .map(|f| (f.bytes, OwnedFrame::new(0, f.header, f.pcm)));
    let _ = decoder.decode(&[0]);
    let again = decoder
        .decode(&data[34..])
        .map(|f| (f.bytes, OwnedFrame::new(0, f.header, f.pcm)));
    assert_eq!(first, again);
    if let Ok((bytes, original)) = first {
        assert!(decoder.decode(&data[34..34 + bytes - 1]).is_err());
        if original.header.block_size <= 1024 {
            let h = original.header;
            let options = EncodeOptions {
                sample_rate: h.sample_rate,
                channels: h.channels,
                bits_per_sample: h.bits_per_sample,
                block_size: (h.block_size as u16).max(16),
                variable_block_size: h.variable_block_size,
                compression_level: 0,
                ..Default::default()
            };
            let mut encoder = FrameEncoder::new(options).unwrap();
            let planes: Vec<_> = original.planes.iter().map(Vec::as_slice).collect();
            let data = encoder.encode(&planes, h.number).unwrap();
            let mut decoder = FrameDecoder::new(Some(options.stream_info().unwrap())).unwrap();
            let decoded = decoder.decode(data).unwrap();
            for (a, b) in decoded.pcm.planes().zip(&original.planes) {
                assert_eq!(a, b);
            }
        }
    }
}
pub fn repair_frame(data: &mut [u8]) {
    if data.len() < 8 || data[0] != 255 || data[1] & 254 != 248 {
        return;
    }
    let count = if data[4] < 128 {
        1
    } else {
        data[4].leading_ones() as usize
    };
    if !(1..=7).contains(&count) {
        return;
    }
    let header = 4
        + count
        + match data[2] >> 4 {
            6 => 1,
            7 => 2,
            _ => 0,
        }
        + match data[2] & 15 {
            12 => 1,
            13 | 14 => 2,
            _ => 0,
        };
    if header + 3 >= data.len() {
        return;
    }
    data[header] = checksum::crc8(&data[..header]);
    let end = data.len() - 2;
    let crc = checksum::crc16(&data[..end]).to_be_bytes();
    data[end..].copy_from_slice(&crc);
}
struct Short<'a> {
    data: &'a [u8],
    max: usize,
}
impl Read for Short<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.max.min(out.len()).min(self.data.len());
        out[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}
pub fn native(data: &[u8]) {
    if data.len() > MAX_INPUT {
        return;
    }
    let Ok(mut decoder) = Decoder::with_options(data, options()) else {
        assert!(Reader::with_options(data, options()).is_err());
        return;
    };
    let mut reader = Reader::with_options(
        Short {
            data,
            max: 1 + data.last().copied().unwrap_or(0) as usize,
        },
        options(),
    )
    .unwrap();
    let mut frames = Vec::new();
    let mut elements = 0;
    let mut complete = false;
    for _ in 0..64 {
        let a = decoder.read_next();
        let b = reader.read_next();
        match (a, b) {
            (Ok(Some(a)), Ok(Some(b))) => {
                let owned = OwnedFrame::new(a.start_sample, a.header, a.pcm);
                assert_eq!(a.start_sample, b.start_sample);
                owned.compare(b.start_sample, b.header, b.pcm);
                elements += a.pcm.samples() * a.pcm.channels() as usize;
                frames.push(owned);
                if elements > 262144 {
                    break;
                }
            }
            (Ok(None), Ok(None)) => {
                complete = true;
                break;
            }
            (Err(_), Err(_)) => break,
            (a, b) => panic!("native path difference: {a:?} / {b:?}"),
        }
    }
    if !complete || frames.is_empty() {
        return;
    }
    assert_eq!(decoder.integrity(), reader.integrity());
    let total = decoder.position();
    for use_seektable in [false, true] {
        let mut seek = SeekReader::with_options(
            Cursor::new(data),
            SeekOptions {
                decode: options(),
                max_index_frames: 256,
                use_seektable,
            },
        )
        .unwrap();
        for sample in [total, 0, total / 2, total - 1] {
            let result = seek.seek(sample);
            if use_seektable && result.is_err() {
                continue;
            } // malformed untrusted hint may hit an I/O/resource policy
            result.unwrap();
            assert_eq!(seek.position(), sample);
            if sample == total {
                assert!(seek.read_next().unwrap().is_none());
                continue;
            }
            let expected = &frames[frames.partition_point(|f| f.start <= sample) - 1];
            let actual = seek.read_next().unwrap().unwrap();
            assert_eq!(actual.start_sample, sample);
            expected.compare(sample, actual.header, actual.pcm);
        }
    }
}
