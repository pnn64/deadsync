use deadlib_audio_decode_wav::{
    Adpcm, AdpcmForwardReader, AdpcmReader, AdpcmWavl, Container, Endian, Format, ForwardReader,
    Wave,
};
use std::io::Cursor;

fn w(n: u16, e: Endian) -> [u8; 2] {
    if e == Endian::Little {
        n.to_le_bytes()
    } else {
        n.to_be_bytes()
    }
}
fn d(n: u32, e: Endian) -> [u8; 4] {
    if e == Endian::Little {
        n.to_le_bytes()
    } else {
        n.to_be_bytes()
    }
}
fn chunk(id: &[u8; 4], payload: &[u8], e: Endian) -> Vec<u8> {
    let mut b = id.to_vec();
    b.extend(d(payload.len() as u32, e));
    b.extend(payload);
    if payload.len() & 1 != 0 {
        b.push(0x87);
    }
    b
}
// These fixtures deliberately do not call any crate serializer or encoder.
fn format(bits: u16, channels: usize, e: Endian, extensible: bool) -> Vec<u8> {
    let block = channels
        * if bits == 0 {
            9
        } else if bits == 3 {
            28
        } else {
            12
        };
    let frames = if bits == 0 {
        6
    } else if bits == 3 {
        65
    } else {
        17
    };
    let mut b = Vec::new();
    for n in [
        if extensible {
            0xfffe
        } else if bits == 0 {
            2
        } else {
            17
        },
        channels as u16,
    ] {
        b.extend(w(n, e));
    }
    b.extend(d(8000, e));
    b.extend(d((8000 * block / frames) as u32, e));
    b.extend(w(block as u16, e));
    b.extend(w(if bits == 0 { 4 } else { bits }, e));
    b.extend(w(
        if extensible {
            22
        } else if bits == 0 {
            32
        } else {
            2
        },
        e,
    ));
    b.extend(w(frames as u16, e));
    if extensible {
        b.extend(d(if channels == 2 { 3 } else { 0 }, e));
        b.extend(d(17, e));
        b.extend(w(0, e));
        b.extend(w(0x10, e));
        b.extend([0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
    } else if bits == 0 {
        b.extend(w(7, e));
        for (a, c) in [
            (256, 0),
            (512, -256),
            (0, 0),
            (192, 64),
            (240, 0),
            (460, -208),
            (392, -232),
        ] {
            b.extend(w(a as u16, e));
            b.extend(w(c as u16, e));
        }
    }
    b
}
fn block(
    bits: u16,
    channels: usize,
    groups: usize,
    variant: i16,
    e: Endian,
) -> (Vec<u8>, Vec<i16>) {
    let frames = if bits == 0 {
        2 + groups * 2
    } else if bits == 3 {
        1 + groups * 32
    } else {
        1 + groups * 8
    };
    let first = (0..channels)
        .map(|c| variant + c as i16 * 100)
        .collect::<Vec<_>>();
    let mut b = Vec::new();
    if bits == 0 {
        b.resize(channels, 0); // predictor (256, 0)
        for _ in 0..channels {
            b.extend(w(16, e));
        }
        for &n in &first {
            b.extend(w((n + 10) as u16, e));
        }
        for &n in &first {
            b.extend(w(n as u16, e));
        }
        b.resize(b.len() + groups * channels, 0); // zero correction
    } else {
        for &n in &first {
            b.extend(w(n as u16, e));
            b.extend([0, 0xf1]);
        }
        let width = if bits == 3 { 12 } else { 4 };
        let mut planes = Vec::new();
        for c in 0..channels {
            let code = if c & 1 == 0 {
                1u8
            } else if bits == 3 {
                5
            } else {
                9
            };
            let mut plane = vec![0; groups * width];
            for i in 0..frames - 1 {
                for bit in 0..bits as usize {
                    let at = i * bits as usize + bit;
                    plane[at / 8] |= ((code >> bit) & 1) << (at % 8);
                }
            }
            planes.push(plane);
        }
        for word in 0..groups * width / 4 {
            for plane in &planes {
                b.extend(&plane[word * 4..word * 4 + 4]);
            }
        }
    }
    let mut truth = Vec::new();
    for f in 0..frames {
        for (c, &n) in first.iter().enumerate() {
            truth.push(if bits == 0 {
                n + if f == 0 { 0 } else { 10 }
            } else {
                (i32::from(n)
                    + f as i32 * if bits == 3 { 4 } else { 1 } * if c & 1 == 0 { 1 } else { -1 })
                .clamp(-32768, 32767) as i16
            });
        }
    }
    (b, truth)
}
fn form(c: Container, fmt: &[u8], list: &[u8], frames: u64, trailing_fact: bool) -> Vec<u8> {
    let e = c.endian();
    let fact = chunk(b"fact", &d(frames.min(u64::from(u32::MAX)) as u32, e), e);
    let mut body = chunk(b"fmt ", fmt, e);
    if !trailing_fact {
        body.extend(&fact);
    }
    let mut audio = chunk(b"LIST", list, e);
    if c.is_64() {
        audio[4..8].fill(255);
    }
    body.extend(audio);
    if trailing_fact {
        body.extend(fact);
    }
    let size = 4 + body.len() as u64 + if c.is_64() { 48 } else { 0 };
    let mut b = match c {
        Container::Riff => b"RIFF",
        Container::Rifx => b"RIFX",
        Container::Rf64 => b"RF64",
        Container::Bw64 => b"BW64",
    }
    .to_vec();
    b.extend(d(if c.is_64() { u32::MAX } else { size as u32 }, e));
    b.extend(b"WAVE");
    if c.is_64() {
        let mut ds = Vec::new();
        ds.extend(size.to_le_bytes());
        ds.extend(0u64.to_le_bytes());
        ds.extend(
            if c == Container::Rf64 {
                frames
            } else {
                u64::MAX
            }
            .to_le_bytes(),
        );
        ds.extend(1u32.to_le_bytes());
        ds.extend(b"LIST");
        ds.extend((list.len() as u64).to_le_bytes());
        b.extend(chunk(b"ds64", &ds, e));
    }
    b.extend(body);
    b
}
fn silence(list: &mut Vec<u8>, truth: &mut Vec<i16>, frames: usize, channels: usize, e: Endian) {
    list.extend(chunk(b"slnt", &d(frames as u32, e), e));
    let held = if truth.is_empty() {
        vec![0; channels]
    } else {
        truth[truth.len() - channels..].to_vec()
    };
    for _ in 0..frames {
        truth.extend(&held);
    }
}

#[test]
fn independent_compressed_children_short_blocks_holds_and_exact_ranges() {
    for c in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let e = c.endian();
        for bits in [0, 3, 4] {
            for channels in [1, 2, 6, 33] {
                if bits == 0 && channels > 2 {
                    continue;
                }
                for extensible in [false, true] {
                    if bits == 0 && extensible {
                        continue;
                    }
                    let fmt = format(bits, channels, e, extensible);
                    let mut list = b"wavl".to_vec();
                    let mut truth = Vec::new();
                    silence(&mut list, &mut truth, 2, channels, e);
                    list.extend(chunk(b"JUNK", b"odd", e));
                    let (mut data, mut samples) = block(bits, channels, 2, 70, e);
                    let (tail, decoded) = block(bits, channels, 1, -123, e);
                    data.extend(tail);
                    samples.extend(decoded);
                    list.extend(chunk(b"data", &data, e));
                    truth.extend(samples);
                    silence(&mut list, &mut truth, 3, channels, e);
                    list.extend(chunk(b"data", &[], e));
                    silence(&mut list, &mut truth, 0, channels, e);
                    list.extend(chunk(b"JUNK", &[], e));
                    silence(&mut list, &mut truth, 4, channels, e);
                    let (data, decoded) = block(bits, channels, 0, 419, e);
                    list.extend(chunk(b"data", &data, e));
                    truth.extend(decoded);
                    silence(&mut list, &mut truth, 2, channels, e);
                    let (data, decoded) = block(bits, channels, 2, -333, e);
                    list.extend(chunk(b"data", &data.repeat(2), e));
                    truth.extend(decoded.repeat(2));
                    silence(&mut list, &mut truth, 1, channels, e);
                    let total = truth.len() / channels;
                    for frames in [0, 1, 2, 3, total / 2, total - 1, total] {
                        let bytes = form(c, &fmt, &list, frames as u64, frames & 1 != 0);
                        let wave = Wave::parse(&bytes).unwrap();
                        let audio = AdpcmWavl::new(&wave).unwrap();
                        let mut embedded = vec![0x81; 7];
                        embedded.extend(&bytes);
                        let mut source = Cursor::new(embedded);
                        source.set_position(7);
                        let mut file = AdpcmReader::new(source).unwrap();
                        assert_eq!(file.frames(), frames as u64);
                        assert_eq!(audio.frames(), frames as u64);
                        let mut scratch = vec![0; audio.codec().scratch_samples()];
                        for batch in [channels, 7 * channels + 1, truth.len() + 1] {
                            let raw = ForwardReader::new(Cursor::new(&bytes)).unwrap();
                            let mut forward =
                                AdpcmForwardReader::from_reader_with_frames(raw, frames as u64)
                                    .unwrap();
                            let mut position = 0;
                            let mut out = vec![0x1234; batch];
                            loop {
                                let n = forward.read_i16(&mut out).unwrap();
                                assert_eq!(&out[..n], &truth[position..position + n]);
                                assert!(out[n..].iter().all(|&n| n == 0x1234));
                                if n == 0 {
                                    break;
                                }
                                position += n;
                                out.fill(0x1234);
                            }
                            assert_eq!(position, frames * channels);
                            forward.finish().unwrap();
                        }
                        for start in 0..=frames {
                            for size in [
                                0,
                                channels - 1,
                                channels,
                                channels + 1,
                                17 * channels + 1,
                                truth.len() + 1,
                            ] {
                                let mut out = vec![0x1234; size];
                                let n = audio
                                    .read_frames(start as u64, &mut out, &mut scratch)
                                    .unwrap();
                                let expected = (frames - start).min(size / channels) * channels;
                                assert_eq!(n, expected);
                                assert_eq!(
                                    &out[..n],
                                    &truth[start * channels..start * channels + n],
                                    "{c:?} bits={bits} ch={channels} ext={extensible} fact={frames} start={start}"
                                );
                                assert!(out[n..].iter().all(|&n| n == 0x1234));
                                let mut actual = vec![0x1234; size];
                                file.seek_frame(start as u64).unwrap();
                                assert_eq!(file.read_i16(&mut actual).unwrap(), n);
                                assert_eq!(actual, out);
                            }
                        }
                        assert!(
                            audio
                                .read_frames(frames as u64 + 1, &mut [], &mut scratch)
                                .is_err()
                        );
                        assert!(audio.read_frames(0, &mut [], &mut []).is_err());
                    }
                    let bytes = form(c, &fmt, &list, total as u64 + 1, false);
                    assert!(AdpcmWavl::new(&Wave::parse(&bytes).unwrap()).is_err());
                    assert!(AdpcmReader::new(Cursor::new(bytes)).is_err());
                }
            }
        }
    }
}

#[test]
fn multiword_virtual_silence_empty_forms_and_literal_bw64_fact() {
    for c in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let e = c.endian();
        let fmt = format(4, 2, e, true);
        let bytes = form(c, &fmt, b"wavl", 0, false);
        let wave = Wave::parse(&bytes).unwrap();
        let a = AdpcmWavl::new(&wave).unwrap();
        let mut file = AdpcmReader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(a.read_frames(0, &mut [1; 2], &mut [0; 34]).unwrap(), 0);
        assert_eq!(file.read_i16(&mut [1; 2]).unwrap(), 0);
        let (data, truth) = block(4, 2, 2, 31000, e);
        let mut list = b"wavl".to_vec();
        list.extend(chunk(b"data", &data, e));
        for n in [u32::MAX, 37] {
            list.extend(chunk(b"slnt", &d(n, e), e));
        }
        let count = if c == Container::Rf64 {
            u64::from(u32::MAX) + 54
        } else {
            u64::from(u32::MAX)
        };
        let bytes = form(c, &fmt, &list, count, true);
        let wave = Wave::parse(&bytes).unwrap();
        let a = AdpcmWavl::new(&wave).unwrap();
        assert_eq!(a.frames(), count);
        let mut file = AdpcmReader::new(Cursor::new(&bytes)).unwrap();
        for start in [17, count / 2, count - 1, count] {
            let mut out = [12; 6];
            let n = a.read_frames(start, &mut out, &mut [0; 34]).unwrap();
            assert_eq!(n, (count - start).min(3) as usize * 2);
            for frame in out[..n].as_chunks::<2>().0 {
                assert_eq!(frame, &truth[truth.len() - 2..]);
            }
            assert!(out[n..].iter().all(|&n| n == 12));
            let mut actual = [12; 6];
            file.seek_frame(start).unwrap();
            assert_eq!(file.read_i16(&mut actual).unwrap(), n);
            assert_eq!(actual, out);
        }
    }
}

#[test]
fn malformed_geometry_lazy_block_headers_and_independent_random_access() {
    let e = Endian::Little;
    for bits in [0, 3, 4] {
        let fmt = format(bits, 1, e, false);
        let codec = Adpcm::from_format(Format::parse(&fmt, e).unwrap()).unwrap();
        assert_eq!(codec.data_frames(0).unwrap(), 0);
        assert!(codec.data_frames(u64::MAX).is_err());
        for n in 1..if bits == 0 { 7 } else { 4 } {
            let mut list = b"wavl".to_vec();
            list.extend(chunk(b"data", &vec![0; n], e));
            let bytes = form(Container::Riff, &fmt, &list, 0, false);
            assert!(AdpcmWavl::new(&Wave::parse(&bytes).unwrap()).is_err());
        }
        let (mut bad, _) = block(bits, 1, 2, 10, e);
        if bits == 0 {
            bad[0] = 255;
        } else {
            bad[2] = 89;
        }
        let (good, expected) = block(bits, 1, 2, -30, e);
        let n = codec.frames_per_block() as u64;
        let mut list = b"wavl".to_vec();
        list.extend(chunk(b"data", &bad, e));
        list.extend(chunk(b"slnt", &d(3, e), e));
        list.extend(chunk(b"data", &good, e));
        let bytes = form(Container::Riff, &fmt, &list, n * 2 + 3, false);
        let wave = Wave::parse(&bytes).unwrap();
        let a = AdpcmWavl::new(&wave).unwrap();
        let mut file = AdpcmReader::new(Cursor::new(&bytes)).unwrap();
        let mut scratch = vec![0; codec.scratch_samples()];
        assert!(a.read_frames(0, &mut [0; 1], &mut scratch).is_err());
        assert!(a.read_frames(n, &mut [0; 1], &mut scratch).is_err());
        assert!(file.read_i16(&mut [0; 1]).is_err());
        file.seek_frame(n).unwrap();
        assert!(file.read_i16(&mut [0; 1]).is_err());
        assert!(file.read_i16(&mut [0; 1]).is_err());
        let mut out = vec![0; expected.len()];
        assert_eq!(
            a.read_frames(n + 3, &mut out, &mut scratch).unwrap(),
            expected.len()
        );
        assert_eq!(out, expected);
        file.seek_frame(n + 3).unwrap();
        assert_eq!(file.read_i16(&mut out).unwrap(), expected.len());
        assert_eq!(out, expected);
        // Capacity validation examines geometry even after the fact-trimmed end.
        list.extend(chunk(b"data", &[0], e));
        let bytes = form(Container::Riff, &fmt, &list, 0, false);
        assert!(AdpcmWavl::new(&Wave::parse(&bytes).unwrap()).is_err());
    }
}
