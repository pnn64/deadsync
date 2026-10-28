use deadlib_audio_decode_wav::{
    Container, Error, RawChunk, Reader, SampleFormat, Spec, Stream, Wave, WriteOptions, Writer,
};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

fn options(container: Container) -> WriteOptions {
    WriteOptions {
        container,
        ..WriteOptions::default()
    }
}

#[test]
fn native_integer_roundtrip_all_containers_and_packing() {
    for kind in [
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
        ] {
            let bits = sample.bits();
            let lo = if bits == 32 {
                i32::MIN
            } else {
                -(1i32 << (bits - 1))
            };
            let hi = if bits == 32 {
                i32::MAX
            } else {
                (1i32 << (bits - 1)) - 1
            };
            let input = [lo, -1, 0, 1, hi, 42];
            let mut sink = Cursor::new(vec![0xde, 0xad]);
            sink.set_position(2);
            let meta = [RawChunk {
                id: *b"zzzz",
                data: b"abc",
            }];
            let mut w =
                Writer::new(sink, Spec::new(sample, 2, 48_000), options(kind), &meta).unwrap();
            assert!(w.write_integers(&input[..3]).is_err());
            if bits < 32 {
                assert!(w.write_integers(&[hi + 1, 0]).is_err());
            }
            assert_eq!(w.frames_written(), 0);
            w.write_integers(&input[..2]).unwrap();
            w.write_integers(&input[2..]).unwrap();
            assert_eq!(w.frames_written(), 3);
            let sink = w.finish().unwrap();
            assert_eq!(sink.position() as usize, sink.get_ref().len());
            let b = sink.into_inner();
            assert_eq!(&b[..2], &[0xde, 0xad]);
            let wave = Wave::parse(&b[2..]).unwrap();
            let mut out = [0; 6];
            wave.pcm().unwrap().read_i32(0, &mut out).unwrap();
            assert_eq!(out, input);
            assert_eq!(wave.container, kind);
            let mut source = Cursor::new(&b);
            source.set_position(2);
            let mut r = Reader::new(source).unwrap();
            assert_eq!(r.frames().unwrap(), 3);
            let mut integers = [1234; 7];
            assert_eq!(r.read_integers(&mut integers).unwrap(), 6);
            assert_eq!(&integers[..6], &input);
            assert_eq!(integers[6], 1234);
            r.seek_frame(2).unwrap();
            let mut raw = vec![0; usize::from(bits / 8) * 2 + 1];
            assert_eq!(r.read_frames(&mut raw).unwrap(), 1);
            assert_eq!(
                &raw[..raw.len() - 1],
                wave.pcm().unwrap().frame_range(2, 1).unwrap().as_bytes()
            );
            assert_eq!(raw[raw.len() - 1], 0);
            let c = *r.chunks().iter().find(|c| c.id == *b"zzzz").unwrap();
            assert_eq!(r.read_metadata(c, &mut [0; 2]).unwrap(), 2);
            assert_eq!(r.position(), 3);
            assert_eq!(r.read_frames(&mut raw).unwrap(), 0);
            assert!(r.seek_frame(4).is_err());
        }
    }
}

#[test]
fn typed_streams_preserve_float_payloads_and_native_integers() {
    let bits32 = [0u32, 0x80000000, 1, 0x7fc12345, 0x7f801234, 0xff800000];
    let bits64 = [
        0u64,
        0x8000000000000000,
        1,
        0x7ff8000012345678,
        0x7ff0000000000123,
        0xfff0000000000000,
    ];
    for kind in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Spec::new(SampleFormat::F32, 2, 44100),
            options(kind),
            &[],
        )
        .unwrap();
        w.write_f32(&bits32.map(f32::from_bits)).unwrap();
        let b = w.finish().unwrap().into_inner();
        assert_eq!(Wave::parse(&b).unwrap().fact, Some(3));
        let mut r = Reader::new(Cursor::new(b)).unwrap();
        assert_eq!(r.fact(), Some(3));
        let mut out = [0.0; 7];
        assert_eq!(r.read_f32(&mut out).unwrap(), 6);
        assert_eq!(
            out[..6].iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            bits32
        );
        assert_eq!(out[6].to_bits(), 0);
        for frame in [2, 0, 1, 3] {
            r.seek_frame(frame).unwrap();
            assert_eq!(
                r.read_f32(&mut out[..2]).unwrap(),
                if frame == 3 { 0 } else { 2 }
            );
        }
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Spec::new(SampleFormat::F64, 1, 44100),
            options(kind),
            &[],
        )
        .unwrap();
        w.write_f64(&bits64.map(f64::from_bits)).unwrap();
        let mut r = Reader::new(Cursor::new(w.finish().unwrap().into_inner())).unwrap();
        let mut out = [0.0; 6];
        r.read_f64(&mut out).unwrap();
        assert_eq!(out.map(f64::to_bits), bits64);
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Spec::new(SampleFormat::I16, 2, 44100),
            options(kind),
            &[],
        )
        .unwrap();
        w.write_i16(&[i16::MIN, -1, 0, 1, i16::MAX, 7]).unwrap();
        let b = w.finish().unwrap().into_inner();
        let mut r = Reader::new(Cursor::new(b)).unwrap();
        let mut out = [0i16; 6];
        r.read_i16(&mut out).unwrap();
        assert_eq!(out, [i16::MIN, -1, 0, 1, i16::MAX, 7]);
    }
}

// Deliberately short and occasionally interrupted I/O exercises Read::read_exact
// across headers, ds64, format, data, odd padding and chunk boundaries.
struct Short<R> {
    inner: R,
    max: usize,
    calls: usize,
}
impl<R: Read> Read for Short<R> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(5) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = self.max.min(b.len());
        self.inner.read(&mut b[..n])
    }
}
#[test]
fn forward_only_stream_boundaries_and_complete_truncation_matrix() {
    for kind in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let metadata = [RawChunk {
            id: *b"zzzz",
            data: b"odd",
        }];
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Spec::new(SampleFormat::U8, 1, 22050),
            options(kind),
            &metadata,
        )
        .unwrap();
        w.write_frames(&[0, 128, 255]).unwrap();
        let b = w.finish().unwrap().into_inner();
        for max in 1..=32 {
            let mut s = Stream::new(Short {
                inner: Cursor::new(&b),
                max,
                calls: 0,
            })
            .unwrap();
            assert_eq!(s.container(), kind);
            while let Some(c) = s.next_chunk().unwrap() {
                if c.id == *b"data" {
                    let mut out = [42; 5];
                    assert_eq!(s.read_chunk(&mut out).unwrap(), 3);
                    assert_eq!(out, [0, 128, 255, 42, 42]);
                }
            }
        }
        for n in 0..b.len() {
            assert!(
                Stream::new(Cursor::new(&b[..n]))
                    .and_then(Stream::finish)
                    .is_err(),
                "prefix {n} {kind:?}"
            );
            assert!(Reader::new(Cursor::new(&b[..n])).is_err());
        }
    }
}

#[test]
fn extensible_high_channel_frames_exceed_stack_scratch() {
    for kind in [Container::Riff, Container::Rifx] {
        let mut spec = Spec::new(SampleFormat::I24, 4000, 8000);
        spec.valid_bits = 20;
        let values: Vec<_> = (0..8000).map(|i| (i - 4000) * 16).collect();
        let mut w = Writer::new(Cursor::new(Vec::new()), spec, options(kind), &[]).unwrap();
        w.write_integers(&values).unwrap();
        let b = w.finish().unwrap().into_inner();
        let wave = Wave::parse(&b).unwrap();
        assert_eq!(wave.format.channels, 4000);
        assert_eq!(wave.format.valid_bits, 20);
        let mut out = vec![0; values.len()];
        wave.pcm().unwrap().read_i32(0, &mut out).unwrap();
        assert_eq!(values, out);
        let mut spec = Spec::new(SampleFormat::I32, 4000, 8000);
        spec.channel_mask = 0;
        let mut w = Writer::new(Cursor::new(Vec::new()), spec, options(kind), &[]).unwrap();
        w.write_i32(&values).unwrap();
        let mut r = Reader::new(Cursor::new(w.finish().unwrap().into_inner())).unwrap();
        assert_eq!(r.read_i32(&mut out).unwrap(), 8000);
        assert_eq!(values, out);
    }
}

#[derive(Default)]
struct Sparse {
    pos: u64,
    len: u64,
    bytes: std::collections::BTreeMap<u64, u8>,
}
impl Write for Sparse {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        // Large writes are known zero audio; retain headers and tiny tail writes.
        if self.pos < 256 || b.len() < 16 {
            for (i, &v) in b
                .iter()
                .take(if b.len() < 16 {
                    b.len()
                } else {
                    (256 - self.pos) as usize
                })
                .enumerate()
            {
                self.bytes.insert(self.pos + i as u64, v);
            }
        }
        self.pos += b.len() as u64;
        self.len = self.len.max(self.pos);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for Sparse {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        let n = b.len().min((self.len - self.pos.min(self.len)) as usize);
        b[..n].fill(0);
        for (&p, &v) in self.bytes.range(self.pos..self.pos + n as u64) {
            b[(p - self.pos) as usize] = v;
        }
        self.pos += n as u64;
        Ok(n)
    }
}
impl Seek for Sparse {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.pos = match from {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(n) => self
                .pos
                .checked_add_signed(n)
                .ok_or(io::ErrorKind::InvalidInput)?,
            SeekFrom::End(n) => self
                .len
                .checked_add_signed(n)
                .ok_or(io::ErrorKind::InvalidInput)?,
        };
        Ok(self.pos)
    }
}
#[test]
fn rf64_promotion_and_seeks_above_four_gib_without_large_allocation() {
    let mut w = Writer::new(
        Sparse::default(),
        Spec::new(SampleFormat::I16, 1, 48000),
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    let zeros = vec![0; 1 << 20];
    for _ in 0..4096 {
        w.write_frames(&zeros).unwrap();
    }
    w.write_i16(&[-12345]).unwrap();
    let expected_frames = (1u64 << 31) + 1;
    assert_eq!(w.frames_written(), expected_frames);
    let mut sink = w.finish().unwrap();
    assert!(sink.bytes.len() < 300);
    sink.seek(SeekFrom::Start(0)).unwrap();
    let mut r = Reader::new(sink).unwrap();
    assert_eq!(r.container(), Container::Rf64);
    assert_eq!(r.frames().unwrap(), expected_frames);
    r.seek_frame(expected_frames - 1).unwrap();
    let mut out = [0i16; 1];
    assert_eq!(r.read_i16(&mut out).unwrap(), 1);
    assert_eq!(out, [-12345]);
    r.seek_frame(1u64 << 30).unwrap();
    r.read_i16(&mut out).unwrap();
    assert_eq!(out, [0]);
}

struct Broken(Cursor<Vec<u8>>);
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for Broken {
    fn seek(&mut self, f: SeekFrom) -> io::Result<u64> {
        self.0.seek(f)
    }
}
#[test]
fn invalid_specs_chunks_and_io_errors_are_reported() {
    assert!(
        Writer::new(
            Broken(Cursor::new(Vec::new())),
            Spec::new(SampleFormat::U8, 1, 8000),
            WriteOptions::default(),
            &[]
        )
        .is_err()
    );
    for spec in [
        Spec::new(SampleFormat::U8, 0, 48000),
        Spec::new(SampleFormat::I32, u16::MAX, 48000),
        Spec::new(SampleFormat::I32, 4, u32::MAX),
    ] {
        assert!(Writer::new(Cursor::new(Vec::new()), spec, WriteOptions::default(), &[]).is_err());
    }
    assert!(matches!(
        Writer::new(
            Cursor::new(Vec::new()),
            Spec::new(SampleFormat::U8, 1, 8000),
            WriteOptions::default(),
            &[RawChunk {
                id: *b"fmt ",
                data: &[]
            }]
        ),
        Err(Error::Invalid(_))
    ));
}

struct ArmedSink {
    inner: Cursor<Vec<u8>>,
    fail: std::rc::Rc<std::cell::Cell<bool>>,
}
impl Write for ArmedSink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.fail.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.inner.write(&b[..b.len().min(3)])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Read for ArmedSink {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.fail.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = b.len().min(3);
        self.inner.read(&mut b[..n])
    }
}
impl Seek for ArmedSink {
    fn seek(&mut self, f: SeekFrom) -> io::Result<u64> {
        self.inner.seek(f)
    }
}
#[test]
fn io_failures_poison_writers_and_readers_require_recovery_seek() {
    let fail = std::rc::Rc::new(std::cell::Cell::new(false));
    let sink = ArmedSink {
        inner: Cursor::new(Vec::new()),
        fail: fail.clone(),
    };
    let mut w = Writer::new(
        sink,
        Spec::new(SampleFormat::I16, 1, 8000),
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    w.write_i16(&[1, 2, 3, 4]).unwrap();
    let mut source = w.finish().unwrap();
    source.seek(SeekFrom::Start(0)).unwrap();
    let mut r = Reader::new(source).unwrap();
    fail.set(true);
    assert!(r.read_i16(&mut [0; 2]).is_err());
    fail.set(false);
    assert!(r.read_i16(&mut [0; 2]).is_err());
    r.seek_frame(1).unwrap();
    let mut out = [0; 2];
    r.read_i16(&mut out).unwrap();
    assert_eq!(out, [2, 3]);
    let mut sink = r.into_inner();
    sink.seek(SeekFrom::Start(0)).unwrap();
    let mut w = Writer::new(
        sink,
        Spec::new(SampleFormat::I16, 1, 8000),
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    fail.set(true);
    assert!(w.write_i16(&[1, 2]).is_err());
    fail.set(false);
    assert!(w.write_i16(&[1, 2]).is_err());
    assert!(w.finish().is_err());
}
