//! Independent wire fixtures for RFC 1950 sections 2.2/2.3 and RFC 1951 3.2.5.
use deadlib_image_png::{
    Error, Samples, decode, decode_reader,
    zlib::{self, Inflater, StreamStatus},
};
use std::io::{Cursor, Read};
mod support;

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    partial: u8,
    used: u8,
}
impl Bits {
    fn put(&mut self, value: u32, count: u8) {
        for bit in 0..count {
            self.partial |= (((value >> bit) & 1) as u8) << self.used;
            self.used += 1;
            if self.used == 8 {
                self.bytes.push(self.partial);
                self.partial = 0;
                self.used = 0;
            }
        }
    }
    fn align(&mut self, ones: bool) {
        if self.used != 0 {
            let remaining = 8 - self.used;
            self.put(if ones { 255 } else { 0 }, remaining);
        }
    }
    fn stored(&mut self, bytes: &[u8], final_block: bool, ones: bool) {
        assert!(bytes.len() <= 65535);
        self.put(u32::from(final_block), 3);
        self.align(ones);
        self.put(bytes.len() as u32, 16);
        self.put((bytes.len() as u32) ^ 65535, 16);
        self.bytes.extend_from_slice(bytes);
    }
    fn fixed(&mut self, symbol: u32) {
        let (code, count): (u32, u8) = match symbol {
            0..=143 => (symbol + 48, 8),
            144..=255 => (symbol + 256, 9),
            256..=279 => (symbol - 256, 7),
            280..=287 => (symbol - 88, 8),
            _ => panic!("not a fixed symbol"),
        };
        self.put(code.reverse_bits() >> (32 - count), count);
    }
    fn distance(&mut self, distance: usize) {
        // RFC 1951 table, deliberately independent of the production mappings.
        let base = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025,
            1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
        ];
        let extra = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12,
            12, 13, 13,
        ];
        let code = base.iter().rposition(|&value| value <= distance).unwrap();
        assert!(distance <= 32768);
        self.put((code as u32).reverse_bits() >> 27, 5);
        self.put((distance - base[code]) as u32, extra[code]);
    }
    fn zlib(mut self, cmf: u8, flags: u8, output: &[u8], ones: bool) -> Vec<u8> {
        self.align(ones);
        let mut bytes = vec![cmf, flags];
        if flags & 32 != 0 {
            bytes.extend_from_slice(&1u32.to_be_bytes()); // Empty preset dictionary ID.
        }
        bytes.extend_from_slice(&self.bytes);
        let (mut a, mut b) = (1u32, 0u32);
        for &value in output {
            a = (a + u32::from(value)) % 65521;
            b = (b + a) % 65521;
        }
        bytes.extend_from_slice(&((b << 16) | a).to_be_bytes());
        bytes
    }
}

fn header_error(cmf: u8, flags: u8) -> Option<Error> {
    if cmf & 15 != 8 || cmf >> 4 > 7 || (u16::from(cmf) * 256 + u16::from(flags)) % 31 != 0 {
        Some(Error::Invalid("zlib header"))
    } else if flags & 32 != 0 {
        Some(Error::Invalid("zlib dictionary not permitted"))
    } else {
        None
    }
}

fn stored_stream(cmf: u8, flags: u8, raw: &[u8]) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.stored(raw, true, false);
    bits.zlib(cmf, flags, raw, false)
}

#[test]
fn every_cmf_and_flg_value_buffered_and_fragmented() {
    let mut counts = [0usize; 3];
    for cmf in 0..=255 {
        for flags in 0..=255 {
            let stream = stored_stream(cmf, flags, &[]);
            let error = header_error(cmf, flags);
            let expected = error.clone().map_or_else(|| Ok(Vec::new()), Err);
            assert_eq!(
                zlib::decompress(&stream, 0),
                expected,
                "{cmf:02x} {flags:02x}"
            );
            let mut inflater = Inflater::new(0).unwrap();
            let first = inflater.process(&stream[..1], &mut []).unwrap();
            assert_eq!(
                (first.consumed, first.written, first.status),
                (1, 0, StreamStatus::NeedInput)
            );
            if let Some(error) = error {
                assert_eq!(inflater.process(&stream[1..2], &mut []), Err(error.clone()));
                assert_eq!(inflater.total_in(), 2);
                assert_eq!(inflater.total_out(), 0);
                assert_eq!(inflater.process(&stream[2..], &mut []), Err(error.clone()));
                assert_eq!(inflater.finish(), Err(error.clone()));
                counts[usize::from(error == Error::Invalid("zlib header")) + 1] += 1;
            } else {
                counts[0] += 1;
                for (index, byte) in stream[1..].iter().enumerate() {
                    let progress = inflater
                        .process(std::slice::from_ref(byte), &mut [])
                        .unwrap();
                    assert_eq!((progress.consumed, progress.written), (1, 0));
                    assert_eq!(
                        progress.status,
                        if index + 2 == stream.len() {
                            StreamStatus::Done
                        } else {
                            StreamStatus::NeedInput
                        }
                    );
                }
                inflater.finish().unwrap();
                assert_eq!(inflater.total_in(), stream.len());
                let mut reference = Vec::new();
                flate2::read::ZlibDecoder::new(stream.as_slice())
                    .read_to_end(&mut reference)
                    .unwrap();
                assert!(reference.is_empty());
                let mut prefix = stream.clone();
                prefix.extend_from_slice(b"untouched");
                assert_eq!(
                    zlib::decompress_prefix(&prefix, 0).unwrap(),
                    (vec![], stream.len())
                );
                assert_eq!(
                    support::inflate(&prefix, 64, 64, 0).unwrap(),
                    (vec![], stream.len())
                );
                assert_eq!(
                    zlib::decompress(&prefix, 0),
                    Err(Error::Invalid("trailing zlib data"))
                );
            }
        }
    }
    // FCHECK=0 and 31 both work for two dictionary-flagged combinations.
    assert_eq!(counts, [32, 34, 65470]);
}

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let mut crc = !0u32;
    for &byte in &out[start..] {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}
fn png(stream: &[u8]) -> Vec<u8> {
    png_width(stream, 1)
}
fn png_width(stream: &[u8], width: u32) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = width.to_be_bytes().to_vec();
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", stream);
    chunk(&mut out, b"IEND", &[]);
    out
}
struct OneByte<R>(R);
impl<R: Read> Read for OneByte<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let count = out.len().min(1);
        self.0.read(&mut out[..count])
    }
}

#[test]
fn every_png_zlib_header_value() {
    let mut accepted = 0;
    for cmf in 0..=255 {
        for flags in 0..=255 {
            let bytes = png(&stored_stream(cmf, flags, &[0, 197]));
            let buffered = decode(&bytes);
            let streamed = decode_reader(bytes.as_slice(), &Default::default());
            if let Some(error) = header_error(cmf, flags) {
                assert_eq!(buffered, Err(error.clone()));
                assert_eq!(streamed, Err(error));
            } else {
                accepted += 1;
                let actual = buffered.unwrap();
                assert_eq!(actual.image.samples, Samples::U8(vec![197]));
                assert_eq!(streamed.unwrap(), actual);
                assert_eq!(
                    decode_reader(OneByte(bytes.as_slice()), &Default::default()).unwrap(),
                    actual
                );
                let mut reader = png::Decoder::new(Cursor::new(&bytes)).read_info().unwrap();
                let mut pixels = [0];
                let info = reader.next_frame(&mut pixels).unwrap();
                assert_eq!(info.buffer_size(), 1);
                assert_eq!(pixels, [197]);
                reader.finish().unwrap();
            }
        }
    }
    assert_eq!(accepted, 32);
}

fn match_stream(cinfo: u8, history: &[u8], distance: usize, ones: bool) -> (Vec<u8>, Vec<u8>) {
    let mut bits = Bits::default();
    if history.is_empty() {
        bits.stored(&[], false, ones);
    }
    for block in history.chunks(32768) {
        bits.stored(block, false, ones);
    }
    bits.put(3, 3); // Final fixed block.
    bits.fixed(257); // Length 3.
    bits.distance(distance);
    bits.fixed(256);
    let mut expected = history.to_vec();
    for _ in 0..3 {
        let value = expected
            .len()
            .checked_sub(distance)
            .map_or(0, |at| expected[at]);
        expected.push(value);
    }
    let cmf = (cinfo << 4) | 8;
    let flags = ((31 - (u16::from(cmf) * 256) % 31) % 31) as u8;
    (bits.zlib(cmf, flags, &expected, ones), expected)
}

#[test]
fn advertised_window_distances_and_ring_wrap() {
    let mut cases = 0;
    for cinfo in 0..8 {
        let window = 1usize << (cinfo + 8);
        for history_len in [window + 1, 65537] {
            let history: Vec<_> = (0..history_len)
                .map(|n| ((n * 71) ^ (n / 13)) as u8)
                .collect();
            for distance in [window - 1, window, window + 1] {
                if distance > 32768 {
                    continue;
                }
                for ones in [false, true] {
                    cases += 1;
                    let (stream, expected) = match_stream(cinfo, &history, distance, ones);
                    let result = if distance <= window {
                        Ok(expected.clone())
                    } else {
                        Err(Error::Invalid("distance outside history window"))
                    };
                    assert_eq!(
                        zlib::decompress(&stream, expected.len()),
                        result,
                        "window={window} distance={distance}"
                    );
                    for (input, output) in [(1, 257), (257, 1), (7, 31), (32768, 4096)] {
                        assert_eq!(
                            support::inflate(&stream, input, output, expected.len()).map(|v| v.0),
                            result
                        );
                    }
                    if distance <= window {
                        let mut reference = Vec::new();
                        flate2::read::ZlibDecoder::new(stream.as_slice())
                            .read_to_end(&mut reference)
                            .unwrap();
                        assert_eq!(reference, expected);
                    }
                }
            }
        }
    }
    assert_eq!(cases, 92);
}

#[test]
fn distance_must_not_precede_actual_output() {
    let mut permissive_reference = 0;
    let mut permissive_png = 0;
    for cinfo in 0..8 {
        let window = 1usize << (cinfo + 8);
        for distance in [1, window - 1, window] {
            let mut history = vec![87; distance - 1];
            if let Some(first) = history.first_mut() {
                *first = 0;
            }
            let (stream, raw) = match_stream(cinfo, &history, distance, false);
            assert_eq!(
                zlib::decompress(&stream, window + 3),
                Err(Error::Invalid("distance outside history window"))
            );
            assert_eq!(
                support::inflate(&stream, 1, 1, window + 3),
                Err(Error::Invalid("distance outside history window"))
            );
            let mut reference = Vec::new();
            if flate2::read::ZlibDecoder::new(stream.as_slice())
                .read_to_end(&mut reference)
                .is_ok()
            {
                permissive_reference += 1;
            }
            let bytes = png_width(&stream, (raw.len() - 1) as u32);
            assert_eq!(
                decode(&bytes),
                Err(Error::Invalid("distance outside history window"))
            );
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()),
                Err(Error::Invalid("distance outside history window"))
            );
            if let Ok(mut reader) = png::Decoder::new(Cursor::new(&bytes)).read_info() {
                let mut pixels = vec![0; raw.len() - 1];
                if reader.next_frame(&mut pixels).is_ok() && reader.finish().is_ok() {
                    permissive_png += 1;
                    assert_eq!(pixels, raw[1..]);
                }
            }
        }
    }
    // miniz_oxide can recover these malformed references using zero-filled
    // history. RFC 1951 forbids them without a preset dictionary. Native strict
    // rejection is asserted above; C zlib independently checks it below.
    println!(
        "Invalid pre-output references accepted: flate2 {permissive_reference}/24, Rust png {permissive_png}/24"
    );
}

struct OracleCase {
    stream: Vec<u8>,
    output: Vec<u8>,
    required: Option<bool>,
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn c_zlib_header_and_history_oracle() {
    use std::{fs, process::Command};
    let strict = std::env::var_os("DEADLIB_ZLIB_STRICT_WINDOW").is_some_and(|value| value == "1");
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let mut cases = Vec::new();
    for cmf in 0..=255 {
        for flags in 0..=255 {
            cases.push(OracleCase {
                stream: stored_stream(cmf, flags, &[]),
                output: vec![],
                required: Some(header_error(cmf, flags).is_none()),
            });
        }
    }
    for cinfo in 0..8 {
        let window = 1usize << (cinfo + 8);
        for history_len in [window + 1, 65537] {
            let history: Vec<_> = (0..history_len)
                .map(|n| ((n * 71) ^ (n / 13)) as u8)
                .collect();
            for distance in [window - 1, window, window + 1] {
                if distance > 32768 {
                    continue;
                }
                for ones in [false, true] {
                    let (stream, output) = match_stream(cinfo, &history, distance, ones);
                    cases.push(OracleCase {
                        stream,
                        output,
                        required: (distance <= window).then_some(true),
                    });
                }
            }
        }
        for distance in [1, window - 1, window] {
            let history = vec![87; distance - 1];
            let (stream, output) = match_stream(cinfo, &history, distance, false);
            cases.push(OracleCase {
                stream,
                output,
                required: Some(false),
            });
        }
    }
    assert_eq!(cases.len(), 65652);
    let path = std::env::temp_dir().join(format!("deadlib-zlib-oracle-{}.bin", std::process::id()));
    let mut wire = Vec::new();
    for case in &cases {
        wire.extend_from_slice(&(case.stream.len() as u32).to_be_bytes());
        wire.extend_from_slice(&(case.output.len() as u32 + 1).to_be_bytes());
        wire.extend_from_slice(&case.stream);
    }
    fs::write(&path, wire).unwrap();
    let result = Command::new(&oracle)
        .arg("--zlib-batch")
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut remaining = result.stdout.as_slice();
    let mut beyond_window = 0;
    for (index, case) in cases.iter().enumerate() {
        assert!(remaining.len() >= 8, "missing oracle record {index}");
        let status = i32::from_be_bytes(remaining[..4].try_into().unwrap());
        let size = u32::from_be_bytes(remaining[4..8].try_into().unwrap()) as usize;
        remaining = &remaining[8..];
        assert!(remaining.len() >= size);
        if let Some(required) = case.required {
            assert_eq!(
                status == 0,
                required,
                "C zlib case={index}, status={status}"
            );
        } else {
            beyond_window += usize::from(status == 0);
            if strict {
                assert_ne!(
                    status, 0,
                    "strict C zlib accepted an over-window distance, case={index}"
                );
            }
        }
        if status == 0 {
            assert_eq!(
                &remaining[..size],
                case.output,
                "C zlib output case={index}"
            );
        } else {
            assert_eq!(size, 0);
        }
        remaining = &remaining[size..];
    }
    assert!(remaining.is_empty());
    println!(
        "C zlib checked {} cases; accepted {beyond_window}/28 over-advertised-window references",
        cases.len()
    );
    let png_path = std::env::temp_dir().join(format!(
        "deadlib-zlib-png-oracle-{}.png",
        std::process::id()
    ));
    let mut rejected = 0;
    for cinfo in 0..8 {
        let window = 1usize << (cinfo + 8);
        for distance in [1, window - 1, window] {
            let mut history = vec![87; distance - 1];
            if let Some(first) = history.first_mut() {
                *first = 0;
            }
            let (stream, raw) = match_stream(cinfo, &history, distance, false);
            fs::write(&png_path, png_width(&stream, (raw.len() - 1) as u32)).unwrap();
            let result = Command::new(&oracle).arg(&png_path).output().unwrap();
            assert!(!result.status.success());
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("distance too far back"),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            rejected += 1;
        }
    }
    fs::remove_file(png_path).unwrap();
    assert_eq!(rejected, 24);
    println!("C libpng rejected all 24 pre-output-reference PNGs at DEFLATE distance validation");
}
