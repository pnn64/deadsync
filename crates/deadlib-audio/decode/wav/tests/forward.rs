use deadlib_audio_decode_wav::{
    Adpcm, AdpcmData, AdpcmForwardReader, AdpcmKind, AdpcmSpec, AdpcmWriter, Container, Endian,
    Error, Format, ForwardReader, Limits, RawChunk, SampleFormat, Spec, Wave, WriteOptions, Writer,
};
use std::io::{self, Cursor, Read};

/// No Seek implementation; every physical read has an arbitrary small ceiling.
struct Input<'a> {
    inner: Cursor<&'a [u8]>,
    width: usize,
    calls: usize,
}
impl<'a> Input<'a> {
    fn new(bytes: &'a [u8], width: usize) -> Self {
        Self {
            inner: Cursor::new(bytes),
            width,
            calls: 0,
        }
    }
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(13) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = out.len().min(self.width);
        self.inner.read(&mut out[..n])
    }
}
fn options(container: Container) -> WriteOptions {
    WriteOptions {
        container,
        auto_rf64: false,
        force_extensible: false,
    }
}
fn append(bytes: &mut Vec<u8>, id: &[u8; 4], data: &[u8], endian: Endian) {
    bytes.extend(id);
    let size = data.len() as u32;
    bytes.extend(match endian {
        Endian::Little => size.to_le_bytes(),
        Endian::Big => size.to_be_bytes(),
    });
    bytes.extend(data);
    if data.len() & 1 != 0 {
        bytes.push(0x7e);
    }
}
fn footer(bytes: &mut Vec<u8>, container: Container) {
    append(bytes, b"tail", b"xyz", container.endian());
    let size = bytes.len() as u64 - 8;
    if container.is_64() {
        bytes[20..28].copy_from_slice(&size.to_le_bytes());
    } else {
        bytes[4..8].copy_from_slice(&match container.endian() {
            Endian::Little => (size as u32).to_le_bytes(),
            Endian::Big => (size as u32).to_be_bytes(),
        });
    }
    bytes.extend(b"outside");
}

#[test]
fn forward_native_frames_preserve_bits_metadata_and_form_boundaries() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        for sample in [
            SampleFormat::U8,
            SampleFormat::I16,
            SampleFormat::I24,
            SampleFormat::I32,
            SampleFormat::F32,
            SampleFormat::F64,
        ] {
            for channels in [1, 3, 4097] {
                let mut spec = Spec::new(sample, channels, 48000);
                if !sample.is_float() {
                    spec.valid_bits = sample.bits() - 2;
                }
                let align = usize::from(channels) * usize::from(sample.bits() / 8);
                let mut audio = (0..7 * align)
                    .map(|i| (i * 73 + 181) as u8)
                    .collect::<Vec<_>>();
                if sample == SampleFormat::F32 {
                    for (i, b) in audio.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let bits = [
                            0u32, 0x80000000, 0x7f800000, 0xff800000, 0x7fc12345, 0x7f812345, 1,
                        ][i % 7];
                        *b = match container.endian() {
                            Endian::Little => bits.to_le_bytes(),
                            Endian::Big => bits.to_be_bytes(),
                        };
                    }
                } else if sample == SampleFormat::F64 {
                    for (i, b) in audio.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                        let bits = [
                            0u64,
                            0x8000000000000000,
                            0x7ff0000000000000,
                            0xfff0000000000000,
                            0x7ff8123456789abc,
                            0x7ff0123456789abc,
                            1,
                        ][i % 7];
                        *b = match container.endian() {
                            Endian::Little => bits.to_le_bytes(),
                            Endian::Big => bits.to_be_bytes(),
                        };
                    }
                }
                let mut writer = Writer::with_frames(
                    Vec::new(),
                    spec,
                    options(container),
                    &[RawChunk {
                        id: *b"lead",
                        data: b"abc",
                    }],
                    7,
                )
                .unwrap();
                writer.write_frames(&audio).unwrap();
                let mut bytes = writer.finish_stream().unwrap();
                footer(&mut bytes, container);
                let wave = Wave::parse(&bytes).unwrap();
                let pcm = wave.pcm().unwrap();
                for width in [1, 7, 31] {
                    let mut seen = Vec::new();
                    let mut reader = ForwardReader::with_metadata(
                        Input::new(&bytes, width),
                        Limits::default(),
                        |c, data| {
                            assert_eq!(c.id, *b"lead");
                            assert_eq!(data.remaining(), 3);
                            let mut b = [0];
                            data.read_exact(&mut b)?;
                            assert_eq!(data.remaining(), 2);
                            seen.push(b[0]);
                            Ok(())
                        },
                    )
                    .unwrap();
                    assert_eq!(seen, [b'a']);
                    assert_eq!(reader.frames().unwrap(), 7);
                    assert_eq!(reader.position().unwrap(), 0);
                    let mut integer = vec![0i32; usize::from(channels) * 3 + 1];
                    let mut integer_expected = integer.clone();
                    let mut f32s = vec![0f32; integer.len()];
                    let mut f32_expected = f32s.clone();
                    let mut f64s = vec![0f64; integer.len()];
                    let mut f64_expected = f64s.clone();
                    assert_eq!(reader.read_frames(&mut []).unwrap(), 0);
                    if channels > 1 {
                        assert_eq!(reader.read_frames(&mut vec![0; align - 1]).unwrap(), 0);
                    }
                    let mut position = 0;
                    while position < 7 {
                        let n = match sample {
                            SampleFormat::F32 => {
                                let n = reader.read_f32(&mut f32s).unwrap();
                                let m = pcm.read_f32(position, &mut f32_expected).unwrap();
                                assert_eq!(n, m);
                                assert!(
                                    f32s[..n]
                                        .iter()
                                        .zip(&f32_expected[..m])
                                        .all(|(a, b)| a.to_bits() == b.to_bits())
                                );
                                n
                            }
                            SampleFormat::F64 => {
                                let n = reader.read_f64(&mut f64s).unwrap();
                                let m = pcm.read_f64(position, &mut f64_expected).unwrap();
                                assert_eq!(n, m);
                                assert!(
                                    f64s[..n]
                                        .iter()
                                        .zip(&f64_expected[..m])
                                        .all(|(a, b)| a.to_bits() == b.to_bits())
                                );
                                n
                            }
                            _ => {
                                let n = if sample == SampleFormat::I32 {
                                    reader.read_i32(&mut integer)
                                } else {
                                    reader.read_integers(&mut integer)
                                }
                                .unwrap();
                                let m = pcm.read_i32(position, &mut integer_expected).unwrap();
                                assert_eq!(n, m);
                                assert_eq!(&integer[..n], &integer_expected[..m]);
                                n
                            }
                        };
                        position += (n / usize::from(channels)) as u64;
                        assert_eq!(reader.position().unwrap(), position);
                    }
                    assert_eq!(reader.read_frames(&mut [0; 32]).unwrap(), 0);
                    assert_eq!(reader.data_bytes_read(), audio.len() as u64);
                    let inner = reader
                        .finish_with_metadata(|c, data| {
                            assert_eq!(c.id, *b"tail");
                            let mut text = Vec::new();
                            data.read_to_end(&mut text)?;
                            assert_eq!(text, b"xyz");
                            seen.push(text[0]);
                            Ok(())
                        })
                        .unwrap();
                    assert_eq!(seen, [b'a', b'x']);
                    assert_eq!(inner.inner.position(), wave.as_bytes().len() as u64);
                }
            }
        }
    }
}

#[test]
fn forward_adpcm_buffers_partial_reads_and_trims_final_padding() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        for kind in [AdpcmKind::Ima, AdpcmKind::Microsoft] {
            for channels in if kind == AdpcmKind::Ima {
                &[1, 2, 5][..]
            } else {
                &[1, 2][..]
            } {
                let spec = AdpcmSpec {
                    kind,
                    channels: *channels,
                    sample_rate: 8000,
                    block_bytes: channels * 32,
                };
                let mut fmt = [0; 50];
                let n = spec.format(container.endian(), &mut fmt).unwrap();
                let codec =
                    Adpcm::from_format(Format::parse(&fmt[..n], container.endian()).unwrap())
                        .unwrap();
                for frames in [
                    0,
                    1,
                    codec.frames_per_block() - 1,
                    codec.frames_per_block() + 3,
                ] {
                    let source = (0..frames * usize::from(*channels))
                        .map(|i| (i as i16).wrapping_mul(1337))
                        .collect::<Vec<_>>();
                    let mut writer = AdpcmWriter::with_frames(
                        Vec::new(),
                        spec,
                        options(container),
                        &[],
                        frames as u64,
                    )
                    .unwrap();
                    writer.write_i16(&source).unwrap();
                    let mut bytes = writer.finish_stream().unwrap();
                    footer(&mut bytes, container);
                    let wave = Wave::parse(&bytes).unwrap();
                    let audio = AdpcmData::new(&wave).unwrap();
                    let mut expected = vec![0; source.len()];
                    let mut scratch = vec![0; codec.scratch_samples()];
                    audio.read_frames(0, &mut expected, &mut scratch).unwrap();
                    for width in [1, 17] {
                        let mut reader =
                            AdpcmForwardReader::new(Input::new(&bytes, width)).unwrap();
                        let mut decoded = Vec::new();
                        let mut out = vec![0; usize::from(*channels) * 3 + 1];
                        assert_eq!(reader.frames(), frames as u64);
                        assert_eq!(reader.read_i16(&mut []).unwrap(), 0);
                        if *channels > 1 {
                            assert_eq!(reader.read_i16(&mut [0]).unwrap(), 0);
                        }
                        loop {
                            let n = reader.read_i16(&mut out).unwrap();
                            if n == 0 {
                                break;
                            }
                            decoded.extend_from_slice(&out[..n]);
                        }
                        assert_eq!(decoded, expected);
                        assert_eq!(reader.position(), frames as u64);
                        let inner = reader.finish().unwrap();
                        assert_eq!(inner.inner.position(), wave.as_bytes().len() as u64);
                    }
                }
            }
        }
    }
}

fn compressed(fact: Option<u32>, trailing: bool) -> Vec<u8> {
    let spec = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 32,
    };
    let mut fmt = [0; 50];
    let n = spec.format(Endian::Little, &mut fmt).unwrap();
    let mut body = b"WAVE".to_vec();
    append(&mut body, b"fmt ", &fmt[..n], Endian::Little);
    if !trailing && let Some(n) = fact {
        append(&mut body, b"fact", &n.to_le_bytes(), Endian::Little);
    }
    let mut data = vec![0; 32];
    data[2] = 20;
    data.extend([0, 0, 20, 0, 0x12, 0x34, 0x56, 0x78]); // complete short last IMA group
    append(&mut body, b"data", &data, Endian::Little);
    if trailing && let Some(n) = fact {
        append(&mut body, b"fact", &n.to_le_bytes(), Endian::Little);
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(body);
    bytes
}

#[test]
fn trailing_fact_requires_explicit_duration_and_is_checked_at_finish() {
    let bytes = compressed(Some(65), true);
    let wave = Wave::parse(&bytes).unwrap();
    let audio = AdpcmData::new(&wave).unwrap();
    assert_eq!(audio.frames(), 65);
    assert!(matches!(
        AdpcmForwardReader::new(Input::new(&bytes, 3)),
        Err(Error::Unsupported(_))
    ));
    let reader = ForwardReader::new(Input::new(&bytes, 3)).unwrap();
    let mut reader = AdpcmForwardReader::from_reader_with_frames(reader, 65).unwrap();
    let mut out = [0; 65];
    assert_eq!(reader.read_i16(&mut out).unwrap(), 65);
    let mut expected = [0; 65];
    audio.read_frames(0, &mut expected, &mut [0; 57]).unwrap();
    assert_eq!(out, expected);
    reader.finish().unwrap();
    let reader = ForwardReader::new(Input::new(&bytes, 3)).unwrap();
    let reader = AdpcmForwardReader::from_reader_with_frames(reader, 64).unwrap();
    assert!(reader.finish().is_err());
    let absent = compressed(None, false);
    let reader = ForwardReader::new(Input::new(&absent, 7)).unwrap();
    let reader = AdpcmForwardReader::from_reader_with_frames(reader, 65).unwrap();
    assert!(reader.finish().is_err());
    let leading = compressed(Some(65), false);
    let reader = ForwardReader::new(Input::new(&leading, 7)).unwrap();
    assert!(AdpcmForwardReader::from_reader_with_frames(reader, 64).is_err());
}

#[test]
fn incomplete_forms_and_deferred_structural_errors_fail_during_finish() {
    let mut writer = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::U8, 1, 8000),
        WriteOptions::default(),
        &[],
        3,
    )
    .unwrap();
    writer.write_frames(&[0, 128, 255]).unwrap();
    let mut bytes = writer.finish_stream().unwrap();
    footer(&mut bytes, Container::Riff);
    let size = Wave::parse(&bytes).unwrap().as_bytes().len();
    for end in 0..size {
        let result =
            ForwardReader::new(Input::new(&bytes[..end], 1)).and_then(ForwardReader::finish);
        assert!(result.is_err(), "truncation {end}");
    }
    ForwardReader::new(Input::new(&bytes[..size], 1))
        .unwrap()
        .finish()
        .unwrap();
    for id in [*b"fmt ", *b"data"] {
        let mut bad = bytes[..size].to_vec();
        append(&mut bad, &id, &[], Endian::Little);
        let n = bad.len() as u32 - 8;
        bad[4..8].copy_from_slice(&n.to_le_bytes());
        assert!(
            ForwardReader::new(Input::new(&bad, 7))
                .unwrap()
                .finish()
                .is_err()
        );
    }
    let mut adpcm = compressed(Some(65), false);
    append(&mut adpcm, b"fact", &65u32.to_le_bytes(), Endian::Little);
    let n = adpcm.len() as u32 - 8;
    adpcm[4..8].copy_from_slice(&n.to_le_bytes());
    assert!(
        AdpcmForwardReader::new(Input::new(&adpcm, 7))
            .unwrap()
            .finish()
            .is_err()
    );
}

#[test]
fn raw_packet_access_frame_alignment_limits_and_errors_are_explicit() {
    let mut writer = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::I16, 2, 8000),
        WriteOptions::default(),
        &[],
        2,
    )
    .unwrap();
    writer.write_i16(&[-1, 0, 1, 2]).unwrap();
    let bytes = writer.finish_stream().unwrap();
    let mut reader = ForwardReader::new(Input::new(&bytes, 1)).unwrap();
    let mut byte = [0];
    assert_eq!(reader.read_data(&mut byte).unwrap(), 1);
    assert!(reader.position().is_err());
    assert!(reader.read_i16(&mut [0; 4]).is_err());
    assert_eq!(reader.read_data(&mut [0; 3]).unwrap(), 3);
    assert_eq!(reader.position().unwrap(), 1);
    let mut out = [0; 2];
    assert_eq!(reader.read_i16(&mut out).unwrap(), 2);
    assert_eq!(out, [1, 2]);
    reader.finish().unwrap();
    assert!(
        ForwardReader::with_limits(
            Input::new(&bytes, 7),
            Limits {
                format_bytes: 15,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let mut bad = compressed(Some(65), false);
    let offset = Wave::parse(&bad).unwrap().data_offset().unwrap() as usize;
    bad[offset + 2] = 255;
    let mut reader = AdpcmForwardReader::new(Input::new(&bad, 7)).unwrap();
    assert!(reader.read_i16(&mut [0; 8]).is_err());
    assert!(reader.read_i16(&mut []).is_err());
    assert!(reader.finish().is_err());
    let mut reordered = b"WAVE".to_vec();
    append(&mut reordered, b"data", &[], Endian::Little);
    append(
        &mut reordered,
        b"fmt ",
        Wave::parse(&bytes).unwrap().format.raw,
        Endian::Little,
    );
    let mut form = b"RIFF".to_vec();
    form.extend((reordered.len() as u32).to_le_bytes());
    form.extend(reordered);
    assert!(Wave::parse(&form).is_ok());
    assert!(matches!(
        ForwardReader::new(Input::new(&form, 3)),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn forward_large_counts_remain_u64_and_physical_io_failures_poison_state() {
    let frames = (1u64 << 33) + 17;
    let writer = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::U8, 1, 8000),
        WriteOptions::default(),
        &[],
        frames,
    )
    .unwrap();
    let mut bytes = writer.into_inner();
    bytes.extend([10, 20, 30]);
    let mut reader = ForwardReader::new(Input::new(&bytes, 1)).unwrap();
    assert_eq!(reader.frames().unwrap(), frames);
    let mut out = [0; 3];
    assert_eq!(reader.read_frames(&mut out).unwrap(), 3);
    assert_eq!(out, [10, 20, 30]);
    assert_eq!(reader.position().unwrap(), 3);
    assert!(reader.read_frames(&mut [0]).is_err());
    assert!(reader.read_frames(&mut []).is_err());
    assert!(reader.finish().is_err());
    let spec = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 4,
    };
    let writer =
        AdpcmWriter::with_frames(Vec::new(), spec, WriteOptions::default(), &[], frames).unwrap();
    let mut bytes = writer.into_inner();
    bytes.extend([0x39, 0x30, 0, 0]);
    assert!(bytes.len() < 128);
    let mut reader = AdpcmForwardReader::new(Input::new(&bytes, 1)).unwrap();
    assert_eq!(reader.frames(), frames);
    let mut sample = [0];
    assert_eq!(reader.read_i16(&mut sample).unwrap(), 1);
    assert_eq!(sample, [12345]);
    assert!(reader.read_i16(&mut sample).is_err());
    assert!(reader.read_i16(&mut []).is_err());
    assert!(reader.finish().is_err());

    struct Broken<'a> {
        source: Input<'a>,
        fail_at: u64,
    }
    impl Read for Broken<'_> {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let pos = self.source.inner.position();
            if pos >= self.fail_at {
                return Err(io::ErrorKind::ConnectionReset.into());
            }
            let n = b.len().min((self.fail_at - pos) as usize);
            self.source.read(&mut b[..n])
        }
    }
    let mut w = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::I16, 2, 8000),
        WriteOptions::default(),
        &[],
        2,
    )
    .unwrap();
    w.write_i16(&[1, 2, 3, 4]).unwrap();
    let bytes = w.finish_stream().unwrap();
    let mut r = ForwardReader::new(Broken {
        source: Input::new(&bytes, 1),
        fail_at: 46,
    })
    .unwrap();
    assert!(r.read_i16(&mut [0; 4]).is_err());
    assert_eq!(r.data_bytes_read(), 0);
    assert!(r.read_i16(&mut []).is_err());
    assert!(r.finish().is_err());
}

#[test]
fn metadata_callbacks_are_bounded_before_format_and_can_abort() {
    let mut w = Writer::with_frames(
        Vec::new(),
        Spec::new(SampleFormat::I16, 1, 8000),
        WriteOptions::default(),
        &[],
        2,
    )
    .unwrap();
    w.write_i16(&[12345, -12345]).unwrap();
    let mut bytes = w.finish_stream().unwrap();
    let mut prefix = Vec::new();
    append(&mut prefix, b"head", b"odd", Endian::Little);
    bytes.splice(12..12, prefix);
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    let mut called = false;
    let mut r =
        ForwardReader::with_metadata(Input::new(&bytes, 1), Limits::default(), |c, payload| {
            assert_eq!(c.id, *b"head");
            let mut text = Vec::new();
            payload.read_to_end(&mut text)?;
            assert_eq!(text, b"odd");
            assert_eq!(payload.remaining(), 0);
            assert_eq!(payload.read(&mut [0; 32])?, 0);
            assert!(payload.read_exact(&mut [0]).is_err());
            called = true;
            Ok(())
        })
        .unwrap();
    assert!(called);
    let mut out = [0; 3];
    assert_eq!(r.read_i16(&mut out).unwrap(), 2);
    assert_eq!(&out[..2], &[12345, -12345]);
    r.finish().unwrap();
    let result = ForwardReader::with_metadata(Input::new(&bytes, 1), Limits::default(), |_, _| {
        Err(Error::Limit("caller metadata policy"))
    });
    assert!(matches!(
        result,
        Err(Error::Limit("caller metadata policy"))
    ));
}
