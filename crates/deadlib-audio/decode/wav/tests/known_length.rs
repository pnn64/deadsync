use deadlib_audio_decode_wav::{
    Adpcm, AdpcmKind, AdpcmSpec, AdpcmWriter, Container, DataLength, Ds64, Endian, Format, Limits,
    Muxer, RawChunk, SampleFormat, Spec, Wave, WriteOptions, Writer,
};
use std::{
    cell::{Cell, RefCell},
    io::{self, Cursor, Write},
    rc::Rc,
};

/// Deliberately has no Seek implementation. Physical writes are short and
/// periodically Interrupted; handles observe output without recovering the sink.
#[derive(Clone, Default)]
struct Pipe {
    bytes: Rc<RefCell<Vec<u8>>>,
    calls: Rc<Cell<usize>>,
    flushes: Rc<Cell<usize>>,
    fail_at: Option<usize>,
    fail_flush: bool,
}
impl Write for Pipe {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get().is_multiple_of(11) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let len = self.bytes.borrow().len();
        if self.fail_at.is_some_and(|limit| len >= limit) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = b
            .len()
            .min(7)
            .min(self.fail_at.map_or(usize::MAX, |limit| limit - len));
        self.bytes.borrow_mut().extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes.set(self.flushes.get() + 1);
        if self.fail_flush {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}
fn options(container: Container) -> WriteOptions {
    WriteOptions {
        container,
        auto_rf64: false,
        force_extensible: true,
    }
}

fn native<W: Write>(
    w: &mut Writer<W>,
    kind: SampleFormat,
    channels: usize,
    start: usize,
    end: usize,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    // Independent expected packing is returned in little-endian order.
    let ints = [-128, -1, 0, 1, 127];
    let float32 = [0, 0x80000000, 0x7f800000, 0x7fc12345, 0x7f812345];
    let float64 = [
        0,
        0x8000000000000000,
        0x7ff0000000000000,
        0x7ff8123456789abc,
        0x7ff0123456789abc,
    ];
    match kind {
        SampleFormat::U8 | SampleFormat::I24 => {
            let samples = (start * channels..end * channels)
                .map(|i| ints[i % ints.len()])
                .collect::<Vec<i32>>();
            w.write_integers(&samples).unwrap();
            for v in samples {
                if kind == SampleFormat::U8 {
                    bytes.push((v + 128) as u8);
                } else {
                    bytes.extend_from_slice(&v.to_le_bytes()[..3]);
                }
            }
        }
        SampleFormat::I16 => {
            let samples = (start * channels..end * channels)
                .map(|i| (i as i16).wrapping_mul(19773))
                .collect::<Vec<_>>();
            w.write_i16(&samples).unwrap();
            for v in samples {
                bytes.extend(v.to_le_bytes());
            }
        }
        SampleFormat::I32 => {
            let samples = (start * channels..end * channels)
                .map(|i| (i as i32).wrapping_mul(192837465))
                .collect::<Vec<_>>();
            w.write_i32(&samples).unwrap();
            for v in samples {
                bytes.extend(v.to_le_bytes());
            }
        }
        SampleFormat::F32 => {
            let samples = (start * channels..end * channels)
                .map(|i| f32::from_bits(float32[i % 5]))
                .collect::<Vec<_>>();
            w.write_f32(&samples).unwrap();
            for v in samples {
                bytes.extend(v.to_bits().to_le_bytes());
            }
        }
        SampleFormat::F64 => {
            let samples = (start * channels..end * channels)
                .map(|i| f64::from_bits(float64[i % 5]))
                .collect::<Vec<_>>();
            w.write_f64(&samples).unwrap();
            for v in samples {
                bytes.extend(v.to_bits().to_le_bytes());
            }
        }
    }
    bytes
}

#[test]
fn write_only_native_output_matches_independent_bits_and_seekable_output() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        for kind in [
            SampleFormat::U8,
            SampleFormat::I16,
            SampleFormat::I24,
            SampleFormat::I32,
            SampleFormat::F32,
            SampleFormat::F64,
        ] {
            for channels in [1, 3, 33] {
                let mut spec = Spec::new(kind, channels, 48000);
                if !kind.is_float() {
                    spec.valid_bits = kind.bits() - 3;
                }
                if channels < 32 {
                    spec.channel_mask = (1 << channels) - 1;
                }
                let metadata = [RawChunk {
                    id: *b"odd!",
                    data: b"abc",
                }];
                let pipe = Pipe::default();
                pipe.bytes.borrow_mut().extend([9, 8, 7]);
                let mut w =
                    Writer::with_frames(pipe, spec, options(container), &metadata, 5).unwrap();
                let mut expected = native(&mut w, kind, usize::from(channels), 0, 2);
                expected.extend(native(&mut w, kind, usize::from(channels), 2, 5));
                let sink = w.finish_stream().unwrap();
                assert_eq!(sink.flushes.get(), 1);
                let actual = sink.bytes.borrow();
                let wave = Wave::parse(&actual[3..]).unwrap();
                if container.endian() == Endian::Big {
                    for sample in expected.chunks_exact_mut(usize::from(kind.bits() / 8)) {
                        sample.reverse();
                    }
                }
                assert_eq!(wave.data().unwrap(), expected);
                assert_eq!(wave.format.valid_bits, spec.valid_bits);
                let mut target = Cursor::new(vec![9, 8, 7]);
                target.set_position(3);
                let mut seekable =
                    Writer::new(target, spec, options(container), &metadata).unwrap();
                native(&mut seekable, kind, usize::from(channels), 0, 2);
                native(&mut seekable, kind, usize::from(channels), 2, 5);
                assert_eq!(*actual, seekable.finish().unwrap().into_inner());
            }
        }
    }
}

#[test]
fn known_adpcm_counts_include_final_block_padding_and_trim_exactly() {
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
                    sample_rate: 22050,
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
                    codec.frames_per_block(),
                    codec.frames_per_block() + 3,
                ] {
                    let samples = (0..frames * usize::from(*channels))
                        .map(|i| (i as i16).wrapping_mul(123))
                        .collect::<Vec<_>>();
                    let mut known = AdpcmWriter::with_frames(
                        Pipe::default(),
                        spec,
                        options(container),
                        &[],
                        frames as u64,
                    )
                    .unwrap();
                    let mut seek =
                        AdpcmWriter::new(Cursor::new(Vec::new()), spec, options(container), &[])
                            .unwrap();
                    for part in samples.chunks(3 * usize::from(*channels)) {
                        known.write_i16(part).unwrap();
                        seek.write_i16(part).unwrap();
                    }
                    let sink = known.finish_stream().unwrap();
                    let bytes = sink.bytes.borrow();
                    assert_eq!(*bytes, seek.finish().unwrap().into_inner());
                    let wave = Wave::parse(&bytes).unwrap();
                    assert_eq!(wave.fact, Some(frames as u64));
                    assert_eq!(
                        wave.data().unwrap().len(),
                        frames.div_ceil(codec.frames_per_block()) * codec.block_bytes()
                    );
                    assert_eq!(
                        deadlib_audio_decode_wav::AdpcmData::new(&wave)
                            .unwrap()
                            .frames(),
                        frames as u64
                    );
                }
            }
        }
    }
}

#[test]
fn declared_limits_reject_overfill_before_io_and_underfill_before_finalization() {
    let sink = Pipe::default();
    let observer = sink.clone();
    let mut writer = Writer::with_frames(
        sink,
        Spec::new(SampleFormat::I16, 2, 48000),
        WriteOptions::default(),
        &[],
        3,
    )
    .unwrap();
    assert_eq!(&observer.bytes.borrow()[12..16], b"fmt "); // no speculative JUNK
    writer.write_i16(&[1, 2]).unwrap();
    let bytes = observer.bytes.borrow().len();
    assert!(writer.write_i16(&[3, 4, 5, 6, 7, 8]).is_err());
    assert_eq!(observer.bytes.borrow().len(), bytes);
    assert!(writer.write_i16(&[3]).is_err());
    assert_eq!(writer.frames_written(), 1);
    writer.write_i16(&[3, 4, 5, 6]).unwrap();
    writer.finish_stream().unwrap();
    assert_eq!(observer.flushes.get(), 1);

    let sink = Pipe::default();
    let observer = sink.clone();
    let mut writer = Writer::with_frames(
        sink,
        Spec::new(SampleFormat::U8, 1, 8000),
        WriteOptions::default(),
        &[],
        3,
    )
    .unwrap();
    writer.write_frames(&[128]).unwrap();
    let bytes = observer.bytes.borrow().len();
    assert!(writer.finish_stream().is_err());
    assert_eq!(observer.bytes.borrow().len(), bytes);
    assert_eq!(observer.flushes.get(), 0);

    let spec = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 32,
    };
    let sink = Pipe::default();
    let observer = sink.clone();
    let mut writer = AdpcmWriter::with_frames(sink, spec, WriteOptions::default(), &[], 3).unwrap();
    let bytes = observer.bytes.borrow().len();
    writer.write_i16(&[1]).unwrap();
    assert!(writer.write_i16(&[2, 3, 4]).is_err());
    assert_eq!(writer.frames_written(), 1);
    assert!(writer.finish_stream().is_err());
    assert_eq!(observer.bytes.borrow().len(), bytes);
    assert_eq!(observer.flushes.get(), 0);
}

fn opaque() -> [u8; 18] {
    [
        0x34, 0x12, 1, 0, 0x40, 0x1f, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0,
    ]
}

#[test]
fn known_header_arithmetic_promotes_before_io_and_keeps_u64_counts() {
    let length = DataLength {
        bytes: 0,
        frames: u64::from(u32::MAX) + 23,
    };
    let mut mux = Muxer::with_lengths(
        Pipe::default(),
        &opaque(),
        WriteOptions::default(),
        &[],
        length,
    )
    .unwrap();
    assert_eq!(mux.expected_length(), Some(length));
    mux.write_data(&[], length.frames).unwrap();
    let sink = mux.finish_stream().unwrap();
    let data = sink.bytes.borrow();
    let wave = Wave::parse(&data).unwrap();
    assert_eq!(wave.container, Container::Rf64);
    assert_eq!(wave.fact, Some(length.frames));
    assert_eq!(wave.ds64.unwrap().sample_count, length.frames);

    for bytes in [
        u64::from(u32::MAX) - 37,
        u64::from(u32::MAX) - 36,
        u64::from(u32::MAX) + 7,
    ] {
        let w = Writer::with_frames(
            Pipe::default(),
            Spec::new(SampleFormat::U8, 1, 8000),
            WriteOptions::default(),
            &[],
            bytes,
        )
        .unwrap();
        let sink = w.into_inner();
        let header = sink.bytes.borrow();
        if bytes == u64::from(u32::MAX) - 37 {
            assert_eq!(&header[..4], b"RIFF");
        } else {
            assert_eq!(&header[..4], b"RF64");
            let ds = Ds64::parse(&header[20..48], Limits::default()).unwrap();
            assert_eq!(ds.data_size, bytes);
            assert_eq!(ds.sample_count, bytes);
            assert_eq!(ds.riff_size, header.len() as u64 - 8 + bytes + (bytes & 1));
        }
    }
    let sink = Pipe::default();
    let observer = sink.clone();
    assert!(
        Writer::with_frames(
            sink,
            Spec::new(SampleFormat::I32, 2, 48000),
            WriteOptions::default(),
            &[],
            u64::MAX
        )
        .is_err()
    );
    assert!(observer.bytes.borrow().is_empty());
    for container in [Container::Riff, Container::Rifx] {
        let sink = Pipe::default();
        let observer = sink.clone();
        assert!(
            Writer::with_frames(
                sink,
                Spec::new(SampleFormat::U8, 1, 8000),
                options(container),
                &[],
                u64::from(u32::MAX)
            )
            .is_err()
        );
        assert!(observer.bytes.borrow().is_empty());
    }
    let sink = Pipe::default();
    let observer = sink.clone();
    assert!(
        Muxer::with_lengths(
            sink,
            &opaque(),
            WriteOptions::default(),
            &[],
            DataLength {
                bytes: u64::MAX,
                frames: 0
            }
        )
        .is_err()
    );
    assert!(observer.bytes.borrow().is_empty());
    let sink = Pipe::default();
    let observer = sink.clone();
    assert!(
        Muxer::with_lengths(
            sink,
            &opaque(),
            WriteOptions::default(),
            &[RawChunk {
                id: *b"data",
                data: &[]
            }],
            DataLength {
                bytes: 0,
                frames: 0
            }
        )
        .is_err()
    );
    assert!(observer.bytes.borrow().is_empty());
}

#[test]
fn short_write_and_flush_failures_are_reported_and_poison_the_writer() {
    let sink = Pipe {
        fail_at: Some(46),
        ..Pipe::default()
    };
    let observer = sink.clone();
    let mut w = Writer::with_frames(
        sink,
        Spec::new(SampleFormat::I16, 1, 48000),
        WriteOptions::default(),
        &[],
        4,
    )
    .unwrap();
    assert!(w.write_i16(&[1, 2, 3, 4]).is_err());
    let calls = observer.calls.get();
    assert!(w.write_i16(&[]).is_err());
    assert_eq!(observer.calls.get(), calls);
    assert!(w.finish_stream().is_err());
    let sink = Pipe {
        fail_flush: true,
        ..Pipe::default()
    };
    let w = Writer::with_frames(
        sink,
        Spec::new(SampleFormat::U8, 1, 8000),
        WriteOptions::default(),
        &[],
        0,
    )
    .unwrap();
    assert!(w.finish_stream().is_err());
    let sink = Pipe {
        fail_at: Some(44),
        ..Pipe::default()
    };
    let mut w = Writer::with_frames(
        sink,
        Spec::new(SampleFormat::U8, 1, 8000),
        WriteOptions::default(),
        &[],
        1,
    )
    .unwrap();
    assert!(w.write_frames(&[128]).is_err());
}
