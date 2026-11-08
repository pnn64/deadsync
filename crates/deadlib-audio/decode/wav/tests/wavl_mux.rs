use deadlib_audio_decode_wav::{
    AdpcmKind, AdpcmSpec, Container, DataLength, Endian, Error, ForwardReader, RawChunk, Reader,
    SampleFormat, Spec, Stream, Wave, WavlMuxer, WavlWriter, WriteOptions, Writer,
};
use std::cell::{Cell, RefCell};
use std::io::{self, Cursor, Write};
use std::rc::Rc;

fn word(v: u16, e: Endian) -> [u8; 2] {
    if e == Endian::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}
fn dword(v: u32, e: Endian) -> [u8; 4] {
    if e == Endian::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}
fn ck(id: &[u8; 4], payload: &[u8], e: Endian, sentinel: bool) -> Vec<u8> {
    let mut b = id.to_vec();
    b.extend(dword(
        if sentinel {
            u32::MAX
        } else {
            payload.len() as u32
        },
        e,
    ));
    b.extend(payload);
    if payload.len() & 1 != 0 {
        b.push(0);
    }
    b
}
fn opaque(e: Endian, guid: bool) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [if guid { 0xfffe } else { 0x1234 }, 3] {
        b.extend(word(v, e));
    }
    for v in [8000, 913] {
        b.extend(dword(v, e));
    }
    for v in [7, 0, if guid { 25 } else { 1 }] {
        b.extend(word(v, e));
    }
    if guid {
        b.extend(word(0, e));
        b.extend(dword(7, e));
        b.extend([0x37; 16]);
        b.extend(b"odd");
    } else {
        b.push(0x81);
    }
    b
}
fn list(e: Endian) -> Vec<u8> {
    let mut b = b"wavl".to_vec();
    b.extend(ck(b"data", &[0x81, 0xfe, 0x20], e, false));
    b.extend(ck(b"slnt", &dword(5, e), e, false));
    b.extend(ck(b"JUNK", b"odd", e, false));
    b.extend(ck(b"data", &[], e, false));
    b.extend(ck(b"data", &[0, 1, 2, 3, 4], e, false));
    b.extend(ck(b"slnt", &dword(0, e), e, false));
    b
}
fn expected(
    container: Container,
    format: &[u8],
    list: &[u8],
    frames: u64,
    reserve: bool,
) -> Vec<u8> {
    let e = container.endian();
    let is64 = container.is_64();
    let mut body = ck(b"fmt ", format, e, false);
    body.extend(ck(
        b"fact",
        &dword(frames.min(u64::from(u32::MAX)) as u32, e),
        e,
        false,
    ));
    body.extend(ck(b"head", b"odd", e, false));
    body.extend(ck(b"LIST", list, e, is64));
    let size = 4 + if reserve { 48 } else { 0 } + body.len() as u64;
    let mut prefix = Vec::new();
    if reserve {
        let mut ds = vec![0; 40];
        if is64 {
            ds[..8].copy_from_slice(&size.to_le_bytes());
            if container == Container::Rf64 {
                ds[16..24].copy_from_slice(&frames.to_le_bytes());
            }
            ds[24..28].copy_from_slice(&1u32.to_le_bytes());
            ds[28..32].copy_from_slice(b"LIST");
            ds[32..].copy_from_slice(&(list.len() as u64).to_le_bytes());
        }
        prefix.extend(ck(if is64 { b"ds64" } else { b"JUNK" }, &ds, e, false));
    }
    [
        match container {
            Container::Riff => b"RIFF",
            Container::Rifx => b"RIFX",
            Container::Rf64 => b"RF64",
            Container::Bw64 => b"BW64",
        }
        .as_slice(),
        &dword(if is64 { u32::MAX } else { size as u32 }, e),
        b"WAVE",
        &prefix,
        &body,
    ]
    .concat()
}
fn write_sequence<W: Write>(m: &mut WavlMuxer<W>) {
    m.write_data(&[0x81, 0xfe, 0x20], 7).unwrap();
    m.write_silence(5).unwrap();
    m.write_chunk(RawChunk {
        id: *b"JUNK",
        data: b"odd",
    })
    .unwrap();
    m.write_data(&[], 0).unwrap();
    m.write_data(&[0, 1, 2, 3, 4], 11).unwrap();
    m.write_silence(0).unwrap();
}
#[derive(Clone, Default)]
struct Pipe {
    bytes: Rc<RefCell<Vec<u8>>>,
    fail_at: Rc<Cell<Option<usize>>>,
    calls: Rc<Cell<usize>>,
}
impl Write for Pipe {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get().is_multiple_of(11) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = b.len().min(3).min(
            self.fail_at
                .get()
                .map_or(usize::MAX, |n| n.saturating_sub(self.bytes.borrow().len())),
        );
        if n == 0 && !b.is_empty() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.bytes.borrow_mut().extend_from_slice(&b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn opaque_formats_children_pads_and_embedded_forms_match_independent_bytes() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let e = container.endian();
        for guid in [false, true] {
            for auto_rf64 in [false, true] {
                let format = opaque(e, guid);
                let payload = list(e);
                let length = DataLength {
                    bytes: payload.len() as u64,
                    frames: 23,
                };
                let options = WriteOptions {
                    container,
                    auto_rf64,
                    force_extensible: true,
                };
                let meta = [RawChunk {
                    id: *b"head",
                    data: b"odd",
                }];
                let pipe = Pipe::default();
                let observer = pipe.clone();
                let mut known =
                    WavlMuxer::with_lengths(pipe, &format, options, &meta, length).unwrap();
                assert_eq!(known.expected_length(), Some(length));
                write_sequence(&mut known);
                assert_eq!(known.frames_written(), 23);
                assert_eq!(known.list_bytes_written(), 74);
                known.finish_stream().unwrap();
                let bytes = observer.bytes.borrow();
                assert_eq!(
                    *bytes,
                    expected(container, &format, &payload, 23, container.is_64())
                );
                let wave = Wave::parse(&bytes).unwrap();
                assert_eq!(wave.format.raw, format);
                assert_eq!(wave.fact, Some(23));
                assert_eq!(wave.audio_chunk().data, payload);
                let reader = Reader::new(Cursor::new(bytes.as_slice())).unwrap();
                assert_eq!(reader.wavl_chunks().unwrap().count(), 6);
                ForwardReader::new(bytes.as_slice())
                    .unwrap()
                    .finish()
                    .unwrap();
                let mut sink = Cursor::new(b"prefix".to_vec());
                sink.set_position(6);
                let mut patch = WavlMuxer::new(sink, &format, options, &meta).unwrap();
                assert_eq!(patch.expected_length(), None);
                write_sequence(&mut patch);
                let output = patch.finish().unwrap().into_inner();
                assert_eq!(&output[..6], b"prefix");
                assert_eq!(
                    &output[6..],
                    expected(
                        container,
                        &format,
                        &payload,
                        23,
                        container.is_64() || (auto_rf64 && container == Container::Riff)
                    )
                );
            }
        }
    }
}

#[test]
fn native_data_counts_are_checked_and_the_wrapper_produces_identical_bytes() {
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
            for channels in [1, 6] {
                let spec = Spec::new(sample, channels, 8000);
                let options = WriteOptions {
                    container,
                    auto_rf64: false,
                    force_extensible: false,
                };
                let header = Writer::with_frames(Vec::new(), spec, options, &[], 0)
                    .unwrap()
                    .finish_stream()
                    .unwrap();
                let wave = Wave::parse(&header).unwrap();
                let format = wave.format.raw;
                let raw = (0..usize::from(channels) * usize::from(sample.bits() / 8) * 7)
                    .map(|i| (i * 73 + 11) as u8)
                    .collect::<Vec<_>>();
                let length = DataLength {
                    bytes: 24 + raw.len() as u64 + (raw.len() & 1) as u64,
                    frames: 10,
                };
                let mut mux =
                    WavlMuxer::with_lengths(Vec::new(), format, options, &[], length).unwrap();
                assert!(mux.write_data(&raw, 6).is_err());
                assert!(mux.write_data(&[], 1).is_err());
                assert_eq!(mux.list_bytes_written(), 4);
                assert_eq!(mux.frames_written(), 0);
                mux.write_data(&raw, 7).unwrap();
                mux.write_silence(3).unwrap();
                let bytes = mux.finish_stream().unwrap();
                let mut native =
                    WavlWriter::with_lengths(Vec::new(), spec, options, &[], length).unwrap();
                assert_eq!(native.expected_length(), Some(length));
                native.write_frames(&raw).unwrap();
                native.write_silence(3).unwrap();
                assert_eq!(native.finish_stream().unwrap(), bytes);
            }
        }
    }
}

#[test]
fn encoded_u64_counts_promote_rf64_and_bw64_keeps_literal_fact_limits() {
    let format = opaque(Endian::Little, false);
    let frames = u64::from(u32::MAX) + 17;
    let metadata = [RawChunk {
        id: *b"head",
        data: b"odd",
    }];
    let mut patch = WavlMuxer::new(
        Cursor::new(Vec::new()),
        &format,
        WriteOptions::default(),
        &metadata,
    )
    .unwrap();
    patch.write_data(&[], frames).unwrap();
    let bytes = patch.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    assert_eq!(wave.container, Container::Rf64);
    assert_eq!(wave.fact, Some(frames));
    let payload = [b"wavl".as_slice(), &ck(b"data", &[], Endian::Little, false)].concat();
    assert_eq!(
        bytes,
        expected(Container::Rf64, &format, &payload, frames, true)
    );
    let mut known = WavlMuxer::with_lengths(
        Vec::new(),
        &format,
        WriteOptions::default(),
        &metadata,
        DataLength { bytes: 12, frames },
    )
    .unwrap();
    known.write_data(&[], frames).unwrap();
    assert_eq!(known.finish_stream().unwrap(), bytes);
    for container in [Container::Riff, Container::Rifx, Container::Bw64] {
        let format = opaque(container.endian(), false);
        let options = WriteOptions {
            container,
            auto_rf64: false,
            force_extensible: false,
        };
        let sink = Pipe::default();
        let observer = sink.clone();
        assert!(
            WavlMuxer::with_lengths(
                sink,
                &format,
                options,
                &[],
                DataLength { bytes: 12, frames }
            )
            .is_err()
        );
        assert!(observer.bytes.borrow().is_empty());
        let mut mux = WavlMuxer::new(Cursor::new(Vec::new()), &format, options, &[]).unwrap();
        assert!(mux.write_data(&[], frames).is_err());
        assert_eq!(mux.frames_written(), 0);
        mux.finish().unwrap();
    }
    let mut max = WavlMuxer::new(
        Cursor::new(Vec::new()),
        &format,
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    max.write_data(&[], u64::MAX).unwrap();
    assert!(max.write_data(&[], 1).is_err());
    let bytes = max.finish().unwrap().into_inner();
    assert_eq!(Wave::parse(&bytes).unwrap().fact, Some(u64::MAX));
    let options = WriteOptions {
        container: Container::Bw64,
        ..WriteOptions::default()
    };
    let mut bw = WavlMuxer::new(Cursor::new(Vec::new()), &format, options, &[]).unwrap();
    bw.write_data(&[], u64::from(u32::MAX)).unwrap();
    assert!(bw.write_silence(1).is_err());
    let bytes = bw.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    assert_eq!(wave.fact, Some(u64::from(u32::MAX)));
    assert_eq!(wave.ds64.unwrap().sample_count, 0);
    let list_bytes = u64::from(u32::MAX) + 17;
    let header = WavlMuxer::with_lengths(
        Vec::new(),
        &format,
        WriteOptions::default(),
        &[],
        DataLength {
            bytes: list_bytes,
            frames: 2,
        },
    )
    .unwrap()
    .into_inner();
    assert!(header.len() < 128);
    let mut stream = Stream::new(header.as_slice()).unwrap();
    assert_eq!(stream.container(), Container::Rf64);
    while let Some(chunk) = stream.next_chunk().unwrap() {
        if chunk.id == *b"LIST" {
            assert_eq!(chunk.size, list_bytes);
            assert_eq!(stream.read_chunk(&mut [0; 4]).unwrap(), 4);
            break;
        }
    }
}

#[test]
fn preencoded_adpcm_and_extensible_ima_descriptors_need_no_muxer_codec_dispatch() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let e = container.endian();
        for kind in [AdpcmKind::Microsoft, AdpcmKind::Ima, AdpcmKind::Ima3] {
            for channels in [1u16, 2, 6, 33] {
                if kind == AdpcmKind::Microsoft && channels > 2 {
                    continue;
                }
                for extensible in [false, true] {
                    if extensible && kind == AdpcmKind::Microsoft {
                        continue;
                    }
                    let per_channel = match kind {
                        AdpcmKind::Microsoft => 15,
                        AdpcmKind::Ima => 8,
                        AdpcmKind::Ima3 => 16,
                    };
                    let frames = match kind {
                        AdpcmKind::Microsoft => 18,
                        AdpcmKind::Ima => 9,
                        AdpcmKind::Ima3 => 33,
                    };
                    let spec = AdpcmSpec {
                        kind,
                        channels,
                        sample_rate: 8000,
                        block_bytes: channels * per_channel,
                    };
                    let mut fmt = [0; 50];
                    let n = if extensible {
                        spec.format_extensible(
                            e,
                            if channels < 32 {
                                (1 << channels) - 1
                            } else {
                                0
                            },
                            &mut fmt,
                        )
                        .unwrap()
                    } else {
                        spec.format(e, &mut fmt).unwrap()
                    };
                    let mut block = vec![0; usize::from(spec.block_bytes)];
                    for c in 0..usize::from(channels) {
                        if kind == AdpcmKind::Microsoft {
                            block[usize::from(channels) + c * 2..usize::from(channels) + c * 2 + 2]
                                .copy_from_slice(&word(16, e));
                            block[usize::from(channels) * 3 + c * 2
                                ..usize::from(channels) * 3 + c * 2 + 2]
                                .copy_from_slice(&word(12345 + c as u16, e));
                            block[usize::from(channels) * 5 + c * 2
                                ..usize::from(channels) * 5 + c * 2 + 2]
                                .copy_from_slice(&word((-12345i16 - c as i16) as u16, e));
                        } else {
                            block[c * 4..c * 4 + 2].copy_from_slice(&word(12345 + c as u16, e));
                        }
                    }
                    let length = DataLength {
                        bytes: 4 + 24 + 2 * (8 + block.len() as u64 + (block.len() & 1) as u64),
                        frames: frames * 2 + 6,
                    };
                    let options = WriteOptions {
                        container,
                        auto_rf64: false,
                        force_extensible: false,
                    };
                    let mut known =
                        WavlMuxer::with_lengths(Vec::new(), &fmt[..n], options, &[], length)
                            .unwrap();
                    let mut patch =
                        WavlMuxer::new(Cursor::new(Vec::new()), &fmt[..n], options, &[]).unwrap();
                    known.write_silence(2).unwrap();
                    known.write_data(&block, frames).unwrap();
                    known.write_data(&block, frames).unwrap();
                    known.write_silence(4).unwrap();
                    patch.write_silence(2).unwrap();
                    patch.write_data(&block, frames).unwrap();
                    patch.write_data(&block, frames).unwrap();
                    patch.write_silence(4).unwrap();
                    let bytes = known.finish_stream().unwrap();
                    assert_eq!(patch.finish().unwrap().into_inner(), bytes);
                    let wave = Wave::parse(&bytes).unwrap();
                    assert_eq!(wave.format.raw, &fmt[..n]);
                    assert_eq!(wave.fact, Some(length.frames));
                    assert_eq!(
                        wave.wavl_chunks()
                            .unwrap()
                            .filter(|c| c.id == *b"data")
                            .map(|c| c.data)
                            .collect::<Vec<_>>(),
                        [&block[..], &block[..]]
                    );
                }
            }
        }
    }
}

#[test]
fn format_reserved_id_declared_length_and_partial_io_errors_are_bounded() {
    let format = opaque(Endian::Little, false);
    let length = DataLength {
        bytes: 16,
        frames: 7,
    };
    for bad in [&format[..15], &[0u8; 18][..]] {
        let pipe = Pipe::default();
        let observer = pipe.clone();
        assert!(WavlMuxer::with_lengths(pipe, bad, WriteOptions::default(), &[], length).is_err());
        assert!(observer.bytes.borrow().is_empty());
    }
    for id in [*b"fmt ", *b"data", *b"fact", *b"ds64"] {
        let pipe = Pipe::default();
        let observer = pipe.clone();
        assert!(
            WavlMuxer::with_lengths(
                pipe,
                &format,
                WriteOptions::default(),
                &[RawChunk { id, data: &[] }],
                length
            )
            .is_err()
        );
        assert!(observer.bytes.borrow().is_empty());
    }
    let mut m =
        WavlMuxer::with_lengths(Vec::new(), &format, WriteOptions::default(), &[], length).unwrap();
    assert!(
        m.write_chunk(RawChunk {
            id: *b"data",
            data: &[]
        })
        .is_err()
    );
    assert!(
        m.write_chunk(RawChunk {
            id: *b"slnt",
            data: &[]
        })
        .is_err()
    );
    assert!(m.write_data(&[0; 3], 8).is_err());
    assert_eq!(m.list_bytes_written(), 4);
    m.write_data(&[0; 3], 7).unwrap();
    assert_eq!(m.finish_stream().unwrap().last(), Some(&0));
    let m =
        WavlMuxer::with_lengths(Vec::new(), &format, WriteOptions::default(), &[], length).unwrap();
    assert!(m.finish_stream().is_err());
    let pipe = Pipe::default();
    let observer = pipe.clone();
    let mut m =
        WavlMuxer::with_lengths(pipe, &format, WriteOptions::default(), &[], length).unwrap();
    let before = observer.bytes.borrow().len();
    observer.fail_at.set(Some(before + 9));
    assert!(matches!(m.write_data(&[1, 2, 3], 7), Err(Error::Io(_))));
    let failed = observer.bytes.borrow().len();
    assert_eq!(failed, before + 9);
    assert!(m.write_data(&[], 0).is_err());
    assert!(m.write_silence(0).is_err());
    assert!(
        m.write_chunk(RawChunk {
            id: *b"JUNK",
            data: &[]
        })
        .is_err()
    );
    assert_eq!(observer.bytes.borrow().len(), failed);
    assert_eq!(m.frames_written(), 0);
    assert_eq!(m.list_bytes_written(), 4);
    assert!(m.finish_stream().is_err());
}
