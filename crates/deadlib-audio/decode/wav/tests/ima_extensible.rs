use deadlib_audio_decode_wav::{
    Adpcm, AdpcmData, AdpcmForwardReader, AdpcmKind, AdpcmReader, AdpcmSpec, AdpcmWriter,
    Container, DataLength, Encoding, Endian, Error, Format, ForwardReader, Muxer, Wave,
    WriteOptions,
};
use std::cell::Cell;
use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::rc::Rc;

fn u16le(v: u16, e: Endian) -> [u8; 2] {
    if e == Endian::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}
fn u32le(v: u32, e: Endian) -> [u8; 4] {
    if e == Endian::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}
fn descriptor(bits: u16, channels: u16, mask: u32, e: Endian) -> Vec<u8> {
    let block = channels * if bits == 3 { 16 } else { 8 };
    let frames = if bits == 3 { 33 } else { 9 };
    let mut b = Vec::new();
    for value in [0xfffe, channels] {
        b.extend(u16le(value, e));
    }
    for value in [8000, 8000 * u32::from(block) / u32::from(frames)] {
        b.extend(u32le(value, e));
    }
    for value in [block, bits, 22, frames] {
        b.extend(u16le(value, e));
    }
    for value in [mask, 17] {
        b.extend(u32le(value, e));
    }
    for value in [0, 16] {
        b.extend(u16le(value, e));
    }
    b.extend([128, 0, 0, 170, 0, 56, 155, 113]);
    b
}
fn kind(bits: u16) -> AdpcmKind {
    if bits == 3 {
        AdpcmKind::Ima3
    } else {
        AdpcmKind::Ima
    }
}
fn opts(container: Container, force_extensible: bool) -> WriteOptions {
    WriteOptions {
        container,
        force_extensible,
        auto_rf64: false,
    }
}

#[test]
fn independent_extensible_ima_samples_union_and_mask_reach_every_reader() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let e = container.endian();
        for bits in [3, 4] {
            for channels in [1, 2, 6, 33] {
                let mask = if channels == 33 {
                    0x80000000
                } else {
                    (1 << channels) - 1
                };
                let header = descriptor(bits, channels, mask, e);
                let format = Format::parse(&header, e).unwrap();
                assert_eq!(format.encoding, Encoding::ImaAdpcm);
                let codec = Adpcm::from_format(format).unwrap();
                let frames = if bits == 3 { 33 } else { 9 };
                assert_eq!(codec.frames_per_block(), frames);
                assert_eq!(format.valid_bits as usize, frames); // Samples union, not precision.
                let mut block = vec![0; codec.block_bytes()];
                for c in 0..usize::from(channels) {
                    block[c * 4..c * 4 + 2]
                        .copy_from_slice(&u16le((-30000 + c as i16 * 100) as u16, e));
                }
                let count = frames * 2 - 3;
                let truth = (0..count)
                    .flat_map(|f| {
                        (0..usize::from(channels)).map(move |c| {
                            -30000
                                + c as i16 * 100
                                + if bits == 3 { (f % frames) as i16 } else { 0 }
                        })
                    })
                    .collect::<Vec<_>>();
                let mut mux = Muxer::with_lengths(
                    Vec::new(),
                    &header,
                    opts(container, false),
                    &[],
                    DataLength {
                        bytes: (block.len() * 2) as u64,
                        frames: count as u64,
                    },
                )
                .unwrap();
                mux.write_data(&block, frames as u64).unwrap();
                mux.write_data(&block, (frames - 3) as u64).unwrap();
                let bytes = mux.finish_stream().unwrap();
                let wave = Wave::parse(&bytes).unwrap();
                assert_eq!(wave.format.channel_mask, mask);
                assert_eq!(wave.format.raw, header);
                let data = AdpcmData::new(&wave).unwrap();
                let mut scratch = vec![0; codec.scratch_samples()];
                let mut file = AdpcmReader::new(Cursor::new(&bytes)).unwrap();
                for start in [0, 1, frames - 1, frames, count - 1, count] {
                    let mut a = vec![0; usize::from(channels) * 7 + 1];
                    let n = data
                        .read_frames(start as u64, &mut a, &mut scratch)
                        .unwrap();
                    let expected = &truth[start * usize::from(channels)..];
                    assert_eq!(&a[..n], &expected[..n]);
                    file.seek_frame(start as u64).unwrap();
                    let mut b = vec![0; a.len()];
                    assert_eq!(file.read_i16(&mut b).unwrap(), n);
                    assert_eq!(&b[..n], &a[..n]);
                }
                let raw = ForwardReader::new(bytes.as_slice()).unwrap();
                let mut forward = AdpcmForwardReader::from_reader(raw).unwrap();
                let mut got = Vec::new();
                let mut out = vec![0; usize::from(channels) * 3 + 1];
                loop {
                    let n = forward.read_i16(&mut out).unwrap();
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&out[..n]);
                }
                assert_eq!(got, truth);
                forward.finish().unwrap();
            }
        }
    }
}

#[test]
fn extensible_descriptor_serializer_is_exact_and_errors_preserve_output() {
    for e in [Endian::Little, Endian::Big] {
        for bits in [3, 4] {
            for channels in [1, 2, 6, 33] {
                for mask in [
                    0,
                    0x80000000,
                    if channels < 32 {
                        (1 << channels) - 1
                    } else {
                        0
                    },
                ] {
                    let spec = AdpcmSpec {
                        kind: kind(bits),
                        channels,
                        sample_rate: 8000,
                        block_bytes: channels * if bits == 3 { 16 } else { 8 },
                    };
                    let mut output = [0xa5; 50];
                    assert_eq!(spec.format_extensible(e, mask, &mut output).unwrap(), 40);
                    assert_eq!(&output[..40], descriptor(bits, channels, mask, e));
                    assert_eq!(&output[40..], &[0xa5; 10]);
                    for len in 0..40 {
                        let mut short = [0xa5; 40];
                        assert!(spec.format_extensible(e, mask, &mut short[..len]).is_err());
                        assert_eq!(short, [0xa5; 40]);
                    }
                    let mut output = [0xa5; 40];
                    let bad_mask = if channels == 1 { 3 } else { 1 };
                    assert!(spec.format_extensible(e, bad_mask, &mut output).is_err());
                    assert_eq!(output, [0xa5; 40]);
                    let bad = AdpcmSpec {
                        block_bytes: 1,
                        ..spec
                    };
                    assert!(bad.format_extensible(e, mask, &mut output).is_err());
                    assert_eq!(output, [0xa5; 40]);
                }
            }
        }
    }
}

#[test]
fn forced_ima_writers_preserve_encoded_blocks_and_both_finalization_paths() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        for bits in [3, 4] {
            for channels in [1, 2, 6, 33] {
                let spec = AdpcmSpec {
                    kind: kind(bits),
                    channels,
                    sample_rate: 8000,
                    block_bytes: channels * if bits == 3 { 16 } else { 8 },
                };
                for frames in [0usize, 1, 37] {
                    let samples = (0..frames * usize::from(channels))
                        .map(|n| (n as i16).wrapping_mul(1733))
                        .collect::<Vec<_>>();
                    let mut known = AdpcmWriter::with_frames(
                        Vec::new(),
                        spec,
                        opts(container, true),
                        &[],
                        frames as u64,
                    )
                    .unwrap();
                    let mut seek =
                        AdpcmWriter::new(Cursor::new(Vec::new()), spec, opts(container, true), &[])
                            .unwrap();
                    let mut legacy = AdpcmWriter::with_frames(
                        Vec::new(),
                        spec,
                        opts(container, false),
                        &[],
                        frames as u64,
                    )
                    .unwrap();
                    for part in samples.chunks(usize::from(channels) * 3) {
                        known.write_i16(part).unwrap();
                        seek.write_i16(part).unwrap();
                        legacy.write_i16(part).unwrap();
                    }
                    let bytes = known.finish_stream().unwrap();
                    assert_eq!(seek.finish().unwrap().into_inner(), bytes);
                    let legacy = legacy.finish_stream().unwrap();
                    let a = Wave::parse(&bytes).unwrap();
                    let b = Wave::parse(&legacy).unwrap();
                    assert_eq!(a.format.tag, 0xfffe);
                    assert_eq!(a.data(), b.data());
                    assert_eq!(a.fact, b.fact);
                    assert_eq!(AdpcmData::new(&a).unwrap().frames(), frames as u64);
                }
            }
        }
    }
}

#[derive(Clone, Default)]
struct NoIo(Rc<Cell<usize>>);
impl Write for NoIo {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + 1);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}
impl Seek for NoIo {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        self.0.set(self.0.get() + 1);
        Ok(0)
    }
}

#[test]
fn malformed_union_guid_geometry_and_unknown_ms_mapping_are_explicit() {
    let header = descriptor(4, 2, 3, Endian::Little);
    for n in 0..40 {
        assert!(
            Format::parse(&header[..n], Endian::Little)
                .and_then(Adpcm::from_format)
                .is_err()
        );
    }
    for (offset, value) in [
        (14, 0),
        (14, 8),
        (14, 16),
        (18, 0),
        (18, 8),
        (18, 10),
        (12, 15),
    ] {
        let mut bad = header.clone();
        bad[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
        assert!(
            Format::parse(&bad, Endian::Little)
                .and_then(Adpcm::from_format)
                .is_err()
        );
    }
    for n in 24..40 {
        let mut bad = header.clone();
        bad[n] ^= 0x80;
        assert!(
            Format::parse(&bad, Endian::Little)
                .and_then(Adpcm::from_format)
                .is_err()
        );
    }
    let mut extended = header.clone();
    extended.extend(b"opaque");
    extended[16..18].copy_from_slice(&28u16.to_le_bytes());
    let format = Format::parse(&extended, Endian::Little).unwrap();
    assert_eq!(&format.extra[22..], b"opaque");
    Adpcm::from_format(format).unwrap();
    let ms = AdpcmSpec {
        kind: AdpcmKind::Microsoft,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 32,
    };
    let mut output = [0xa5; 80];
    assert!(matches!(
        ms.format_extensible(Endian::Little, 0, &mut output),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(output, [0xa5; 80]);
    let sink = NoIo::default();
    assert!(matches!(
        AdpcmWriter::new(sink.clone(), ms, opts(Container::Riff, true), &[]),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        AdpcmWriter::with_frames(sink.clone(), ms, opts(Container::Riff, true), &[], 1),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(sink.0.get(), 0);
    let mut ms_header = header.clone();
    ms_header[24..28].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        Adpcm::from_format(Format::parse(&ms_header, Endian::Little).unwrap()),
        Err(Error::Unsupported(_))
    ));
}
