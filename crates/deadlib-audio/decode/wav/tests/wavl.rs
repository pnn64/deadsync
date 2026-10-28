use deadlib_audio_decode_wav::{
    Container, DataLength, Endian, Format, ForwardReader, Limits, Pcm, RawChunk, Reader,
    SampleFormat, Spec, Wave, WavlWriter, WriteOptions,
};
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

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
fn ck(id: &[u8; 4], data: &[u8], e: Endian) -> Vec<u8> {
    let mut b = id.to_vec();
    b.extend(dword(data.len() as u32, e));
    b.extend(data);
    if data.len() & 1 != 0 {
        b.push(0);
    }
    b
}
fn fmt(sample: SampleFormat, channels: u16, valid: u16, e: Endian) -> Vec<u8> {
    let bits = sample.bits();
    let align = channels * (bits / 8);
    let tag: u16 = if sample.is_float() { 3 } else { 1 };
    let ext = channels > 2 || valid != bits;
    let mut b = Vec::new();
    b.extend(word(if ext { 0xfffe } else { tag }, e));
    b.extend(word(channels, e));
    b.extend(dword(48000, e));
    b.extend(dword(48000 * u32::from(align), e));
    b.extend(word(align, e));
    b.extend(word(bits, e));
    if ext {
        b.extend(word(22, e));
        b.extend(word(valid, e));
        b.extend(dword(if channels == 6 { 0x3f } else { 0 }, e));
        b.extend(dword(u32::from(tag), e));
        b.extend(word(0, e));
        b.extend(word(0x10, e));
        b.extend([128, 0, 0, 170, 0, 56, 155, 113]);
    } else if tag == 3 {
        b.extend(word(0, e));
    }
    b
}
fn audio(sample: SampleFormat, channels: u16, frames: usize, e: Endian) -> Vec<u8> {
    let mut b = Vec::new();
    for i in 0..frames * usize::from(channels) {
        match sample {
            SampleFormat::U8 => b.push([0, 128, 255, 73, 211][i % 5]),
            SampleFormat::I16 => b.extend(word([0, 32768, 65535, 32767, 17973][i % 5], e)),
            SampleFormat::I24 => {
                let x = [0, -8388608, -1, 8388607, 1234567i32][i % 5];
                b.extend(if e == Endian::Little {
                    x.to_le_bytes()[..3].to_vec()
                } else {
                    x.to_be_bytes()[1..].to_vec()
                });
            }
            SampleFormat::I32 => b.extend(dword(
                [0, 0x80000000, 0xffffffff, 0x7fffffff, 0x12345678][i % 5],
                e,
            )),
            SampleFormat::F32 => {
                b.extend(dword([0, 0x80000000, 0x7f812345, 1, 0xffc13456][i % 5], e))
            }
            SampleFormat::F64 => {
                let x = [
                    0,
                    0x8000000000000000,
                    0x7ff0123456789abc,
                    1,
                    0xfff87654321abcdeu64,
                ][i % 5];
                b.extend(if e == Endian::Little {
                    x.to_le_bytes()
                } else {
                    x.to_be_bytes()
                });
            }
        }
    }
    b
}
fn form(
    container: Container,
    format: &[u8],
    children: &[([u8; 4], Vec<u8>)],
    frames: u64,
    trailing_fact: bool,
    sentinel_list: bool,
) -> Vec<u8> {
    let e = container.endian();
    let mut list = b"wavl".to_vec();
    for (id, data) in children {
        list.extend(ck(id, data, e));
    }
    let info = [b"INFO".as_slice(), &ck(b"INAM", b"name\0", e)].concat();
    let fact = ck(
        b"fact",
        &dword(frames.min(u64::from(u32::MAX)) as u32, e),
        e,
    );
    let mut body = ck(b"fmt ", format, e);
    body.extend(ck(b"LIST", &info, e));
    if !trailing_fact {
        body.extend(&fact);
    }
    body.extend(ck(b"JUNK", b"odd", e));
    let mut list_chunk = ck(b"LIST", &list, e);
    if sentinel_list {
        list_chunk[4..8].fill(255);
    }
    body.extend(list_chunk);
    if trailing_fact {
        body.extend(fact);
    }
    body.extend(ck(b"LIST", &info, e));
    if container.is_64() {
        let mut ds = Vec::new();
        let table = if sentinel_list {
            [b"LIST".as_slice(), &(list.len() as u64).to_le_bytes()].concat()
        } else {
            Vec::new()
        };
        ds.extend((4 + 36 + table.len() as u64 + body.len() as u64).to_le_bytes());
        ds.extend(0u64.to_le_bytes());
        ds.extend(frames.to_le_bytes());
        ds.extend(u32::from(sentinel_list).to_le_bytes());
        ds.extend(table);
        [
            if container == Container::Bw64 {
                b"BW64".as_slice()
            } else {
                b"RF64".as_slice()
            },
            &[255; 4],
            b"WAVE",
            &ck(b"ds64", &ds, e),
            &body,
        ]
        .concat()
    } else {
        [
            if container == Container::Rifx {
                b"RIFX".as_slice()
            } else {
                b"RIFF".as_slice()
            },
            &dword(4 + body.len() as u32, e),
            b"WAVE",
            &body,
        ]
        .concat()
    }
}
fn fixture(
    container: Container,
    sample: SampleFormat,
    channels: u16,
    trailing_fact: bool,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let e = container.endian();
    let align = usize::from(channels) * usize::from(sample.bits() / 8);
    let first = audio(sample, channels, 7, e);
    let last = audio(sample, channels, 3, e);
    let children = vec![
        (*b"slnt", dword(3, e).to_vec()),
        (*b"data", first.clone()),
        (*b"JUNK", b"abcde".to_vec()),
        (*b"slnt", dword(5, e).to_vec()),
        (*b"data", vec![]),
        (*b"slnt", dword(0, e).to_vec()),
        (*b"ds64", b"opaque".to_vec()),
        (*b"data", last.clone()),
        (*b"slnt", dword(2, e).to_vec()),
    ];
    let mut truth = vec![if sample == SampleFormat::U8 { 128 } else { 0 }; 3 * align];
    truth.extend(&first);
    for _ in 0..5 {
        truth.extend(&first[first.len() - align..]);
    }
    truth.extend(&last);
    for _ in 0..2 {
        truth.extend(&last[last.len() - align..]);
    }
    let format = fmt(sample, channels, sample.bits(), e);
    (
        form(
            container,
            &format,
            &children,
            (truth.len() / align) as u64,
            trailing_fact,
            container.is_64(),
        ),
        format,
        truth,
    )
}
struct Short {
    inner: Cursor<Vec<u8>>,
    width: usize,
    calls: usize,
}
impl Short {
    fn new(b: &[u8], width: usize) -> Self {
        Self {
            inner: Cursor::new(b.to_vec()),
            width,
            calls: 0,
        }
    }
}
impl Read for Short {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(13) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = b.len().min(self.width);
        self.inner.read(&mut b[..n])
    }
}
impl Seek for Short {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.inner.seek(p)
    }
}

#[test]
fn borrowed_seekable_and_forward_wavl_match_independent_native_bytes() {
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
            for channels in [1u16, 2, 6, 33] {
                let (bytes, format, truth) = fixture(container, sample, channels, false);
                let align = usize::from(channels) * usize::from(sample.bits() / 8);
                let wave = Wave::parse(&bytes).unwrap();
                let pcm = wave.pcm().unwrap();
                assert!(wave.data().is_none());
                assert!(wave.data_offset().is_none());
                assert!(pcm.as_bytes().is_none());
                assert!(
                    pcm.as_u8().is_none()
                        && pcm.as_i16().is_none()
                        && pcm.as_i32().is_none()
                        && pcm.as_f32().is_none()
                        && pcm.as_f64().is_none()
                );
                assert_eq!(wave.wavl_chunks().unwrap().count(), 9);
                assert_eq!(pcm.frames(), 20);
                assert_eq!(pcm.samples(), 20 * u128::from(channels));
                let mut output = vec![0xa5; truth.len() + align - 1];
                assert_eq!(pcm.read_frames(0, &mut output).unwrap(), 20);
                assert_eq!(&output[..truth.len()], &truth);
                assert!(output[truth.len()..].iter().all(|&v| v == 0xa5));
                let mut reader = Reader::new(Short::new(&bytes, 3)).unwrap();
                assert!(reader.data_chunk().is_none());
                assert_eq!(reader.frames().unwrap(), 20);
                for start in [0u64, 1, 2, 3, 4, 9, 10, 14, 15, 17, 19, 20] {
                    let mut expected = vec![0; 4 * align + align - 1];
                    let n = pcm.read_frames(start, &mut expected).unwrap();
                    reader.seek_frame(start).unwrap();
                    let mut actual = vec![0; expected.len()];
                    assert_eq!(reader.read_frames(&mut actual).unwrap(), n);
                    assert_eq!(&actual[..n * align], &expected[..n * align]);
                    let view = pcm.frame_range(start, n as u64).unwrap();
                    actual.fill(0);
                    assert_eq!(view.read_frames(0, &mut actual).unwrap(), n);
                    assert_eq!(&actual[..n * align], &expected[..n * align]);
                    let at = reader.position();
                    let c = reader.wavl_chunks().unwrap().nth(2).unwrap();
                    let mut text = [0; 5];
                    assert_eq!(reader.read_metadata(c, &mut text).unwrap(), 5);
                    assert_eq!(&text, b"abcde");
                    assert_eq!(reader.position(), at);
                }
                // Shared conversion dispatch must preserve the independent raw truth.
                let flat =
                    Pcm::new(&truth, Format::parse(&format, container.endian()).unwrap()).unwrap();
                if !sample.is_float() {
                    let mut a = vec![0; 20 * usize::from(channels)];
                    let mut b = a.clone();
                    pcm.read_i32(0, &mut a).unwrap();
                    flat.read_i32(0, &mut b).unwrap();
                    assert_eq!(a, b);
                    reader.seek_frame(0).unwrap();
                    reader.read_integers(&mut a).unwrap();
                    assert_eq!(a, b);
                } else if sample == SampleFormat::F32 {
                    let mut a = vec![0f32; 20 * usize::from(channels)];
                    pcm.read_f32(0, &mut a).unwrap();
                    let got = a
                        .iter()
                        .flat_map(|v| {
                            if container.endian() == Endian::Little {
                                v.to_bits().to_le_bytes()
                            } else {
                                v.to_bits().to_be_bytes()
                            }
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(got, truth);
                } else {
                    let mut a = vec![0f64; 20 * usize::from(channels)];
                    pcm.read_f64(0, &mut a).unwrap();
                    let got = a
                        .iter()
                        .flat_map(|v| {
                            if container.endian() == Endian::Little {
                                v.to_bits().to_le_bytes()
                            } else {
                                v.to_bits().to_be_bytes()
                            }
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(got, truth);
                }
                for width in [1, 7] {
                    let mut metadata = Vec::new();
                    let mut forward = ForwardReader::with_metadata(
                        Short::new(&bytes, width),
                        Limits::default(),
                        |c, p| {
                            let mut b = Vec::new();
                            p.read_to_end(&mut b)?;
                            assert_eq!(b.len() as u64, c.size);
                            metadata.push((c.id, b));
                            Ok(())
                        },
                    )
                    .unwrap();
                    let mut actual = Vec::<u8>::new();
                    let mut scratch = vec![0; 5 * align + align - 1];
                    loop {
                        let n = forward
                            .read_frames_with_metadata(&mut scratch, |c, p| {
                                let mut b = Vec::new();
                                p.read_to_end(&mut b)?;
                                metadata.push((c.id, b));
                                Ok(())
                            })
                            .unwrap();
                        if n == 0 {
                            break;
                        }
                        actual.extend(&scratch[..n * align]);
                    }
                    assert_eq!(actual, truth);
                    assert_eq!(forward.position().unwrap(), 20);
                    let inner = forward
                        .finish_with_metadata(|c, p| {
                            let mut b = Vec::new();
                            p.read_to_end(&mut b)?;
                            assert_eq!(b.len() as u64, c.size);
                            metadata.push((c.id, b));
                            Ok(())
                        })
                        .unwrap();
                    assert_eq!(inner.inner.position(), bytes.len() as u64);
                    assert_eq!(metadata.iter().filter(|(id, _)| id == b"LIST").count(), 2);
                    assert!(
                        metadata
                            .iter()
                            .filter(|(id, _)| id == b"LIST")
                            .all(|(_, b)| b.starts_with(b"INFO"))
                    );
                    assert!(
                        metadata
                            .iter()
                            .any(|(id, b)| id == b"JUNK" && b == b"abcde")
                    );
                }
            }
        }
    }
}

#[test]
fn trailing_fact_is_validated_without_requiring_a_forward_duration_up_front() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let (bytes, _, truth) = fixture(container, SampleFormat::I16, 2, true);
        let mut f = ForwardReader::new(Short::new(&bytes, 1)).unwrap();
        if !container.is_64() {
            assert!(f.frames().is_err());
        }
        let mut out = [0; 80];
        assert_eq!(f.read_frames(&mut out).unwrap(), 20);
        assert_eq!(out, truth.as_slice());
        assert_eq!(f.read_frames(&mut out).unwrap(), 0);
        assert_eq!(f.frames().unwrap(), 20);
        f.finish().unwrap();
        let mut f = ForwardReader::new(Short::new(&bytes, 1)).unwrap();
        let mut prefix = [0; 4];
        f.read_frames(&mut prefix).unwrap();
        f.finish().unwrap();
    }
}

#[test]
fn rf64_wavl_duration_uses_fact_or_observed_frames_not_unused_ds64() {
    for container in [Container::Rf64, Container::Bw64] {
        for trailing in [false, true] {
            let (base, _, truth) = fixture(container, SampleFormat::I16, 2, trailing);
            let fact_at = Wave::parse(&base)
                .unwrap()
                .chunks()
                .unwrap()
                .map(Result::unwrap)
                .find(|c| c.id == *b"fact")
                .unwrap()
                .offset as usize;
            for ds_count in [0, 1, 19, 20, 21, u64::MAX] {
                let mut bytes = base.clone();
                bytes[36..44].copy_from_slice(&ds_count.to_le_bytes());
                assert_eq!(Wave::parse(&bytes).unwrap().pcm().unwrap().frames(), 20);
                assert_eq!(
                    Reader::new(Cursor::new(&bytes)).unwrap().frames().unwrap(),
                    20
                );
                let mut reader = ForwardReader::new(Short::new(&bytes, 1)).unwrap();
                assert_eq!(reader.ds64_sample_count(), Some(ds_count));
                if trailing {
                    assert!(reader.frames().is_err());
                } else {
                    assert_eq!(reader.frames().unwrap(), 20);
                }
                let mut out = [0; 80];
                assert_eq!(reader.read_frames(&mut out).unwrap(), 20);
                assert_eq!(out, truth.as_slice());
                assert_eq!(reader.read_frames(&mut out).unwrap(), 0);
                assert_eq!(reader.frames().unwrap(), 20);
                reader.finish().unwrap();
                // The same ds64 field is authoritative when fact uses its sentinel.
                bytes[fact_at..fact_at + 4].fill(255);
                let valid = container == Container::Rf64 && ds_count == 20;
                assert_eq!(Wave::parse(&bytes).is_ok(), valid);
                let result =
                    ForwardReader::new(Short::new(&bytes, 3)).and_then(ForwardReader::finish);
                assert_eq!(result.is_ok(), valid);
            }
        }
    }
}

#[test]
fn large_virtual_silence_seeks_remain_u64_and_forward_finish_does_not_expand_it() {
    for container in [Container::Rf64, Container::Bw64] {
        let e = Endian::Little;
        let format = fmt(SampleFormat::U8, 2, 8, e);
        let count = u64::from(u32::MAX);
        let children = [
            (*b"slnt", u32::MAX.to_le_bytes().to_vec()),
            (*b"data", vec![23, 247]),
            (*b"data", vec![]),
            (*b"slnt", u32::MAX.to_le_bytes().to_vec()),
            (*b"data", vec![2, 3]),
        ];
        let bytes = form(container, &format, &children, count * 2 + 2, false, true);
        if container == Container::Bw64 {
            // The RF64-only 64-bit fact extension cannot use BW64 dummy words.
            assert!(Wave::parse(&bytes).is_err());
            assert!(Reader::new(Cursor::new(&bytes)).is_err());
            assert!(
                ForwardReader::new(Short::new(&bytes, 1))
                    .unwrap()
                    .finish()
                    .is_err()
            );
            continue;
        }
        let wave = Wave::parse(&bytes).unwrap();
        let pcm = wave.pcm().unwrap();
        assert_eq!(pcm.frames(), count * 2 + 2);
        let mut reader = Reader::new(Short::new(&bytes, 1)).unwrap();
        for (at, want) in [
            (0, [128, 128]),
            (count - 1, [128, 128]),
            (count, [23, 247]),
            (count + 1, [23, 247]),
            (count * 2, [23, 247]),
            (count * 2 + 1, [2, 3]),
        ] {
            let mut a = [0; 2];
            assert_eq!(pcm.read_frames(at, &mut a).unwrap(), 1);
            assert_eq!(a, want);
            reader.seek_frame(at).unwrap();
            assert_eq!(reader.read_frames(&mut a).unwrap(), 1);
            assert_eq!(a, want);
        }
        reader.seek_frame(pcm.frames()).unwrap();
        assert!(reader.seek_frame(pcm.frames() + 1).is_err());
        assert_eq!(reader.position(), pcm.frames());
        let mut out = [0; 2];
        assert_eq!(reader.read_frames(&mut out).unwrap(), 0);
        let mut f = ForwardReader::new(Short::new(&bytes, 1)).unwrap();
        assert_eq!(f.frames().unwrap(), pcm.frames());
        f.read_frames(&mut out).unwrap();
        assert_eq!(out, [128, 128]);
        f.finish().unwrap();
    }
}

#[test]
fn malformed_wavl_counts_boundaries_padding_and_limits_agree() {
    let (valid, format, _) = fixture(Container::Riff, SampleFormat::I16, 2, false);
    for length in 0..valid.len() {
        let b = &valid[..length];
        assert!(Wave::parse(b).is_err());
        assert!(Reader::new(Cursor::new(b)).is_err());
        assert!(
            ForwardReader::new(Cursor::new(b))
                .and_then(ForwardReader::finish)
                .is_err()
        );
    }
    for children in [
        vec![(*b"data", vec![1])],
        vec![(*b"slnt", vec![0; 3])],
        vec![(*b"slnt", vec![0; 5])],
    ] {
        let b = form(Container::Riff, &format, &children, 0, false, false);
        assert!(Wave::parse(&b).is_err());
        assert!(Reader::new(Cursor::new(&b)).is_err());
        assert!(
            ForwardReader::new(Cursor::new(&b))
                .and_then(ForwardReader::finish)
                .is_err()
        );
    }
    let wave = Wave::parse(&valid).unwrap();
    let total = wave.chunks().unwrap().count() + wave.wavl_chunks().unwrap().count();
    for (budget, good) in [(total, true), (total - 1, false)] {
        let limits = Limits {
            chunks: budget,
            ..Limits::default()
        };
        assert_eq!(Wave::with_limits(&valid, limits).is_ok(), good);
        assert_eq!(
            Reader::with_limits(Cursor::new(&valid), limits).is_ok(),
            good
        );
        assert_eq!(
            ForwardReader::with_limits(Cursor::new(&valid), limits)
                .and_then(ForwardReader::finish)
                .is_ok(),
            good
        );
    }
    let mut missing = valid.clone();
    let fact = wave
        .chunks()
        .unwrap()
        .map(Result::unwrap)
        .find(|c| c.id == *b"fact")
        .unwrap();
    missing[fact.offset as usize - 8..fact.offset as usize - 4].copy_from_slice(b"JUNK");
    for bad in [missing, {
        let mut b = valid.clone();
        b[fact.offset as usize..fact.offset as usize + 4].copy_from_slice(&19u32.to_le_bytes());
        b
    }] {
        assert!(Wave::parse(&bad).is_err());
        assert!(Reader::new(Cursor::new(&bad)).is_err());
        assert!(
            ForwardReader::new(Cursor::new(&bad))
                .and_then(ForwardReader::finish)
                .is_err()
        );
    }
}

fn copy_segments<W: Write>(writer: &mut WavlWriter<W>, wave: &Wave<'_>) {
    for child in wave.wavl_chunks().unwrap() {
        match &child.id {
            b"data" => writer.write_frames(child.data).unwrap(),
            b"slnt" => {
                let b: [u8; 4] = child.data.try_into().unwrap();
                let n = if wave.container.endian() == Endian::Little {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                };
                writer.write_silence(u64::from(n)).unwrap();
            }
            _ => writer
                .write_chunk(RawChunk {
                    id: child.id,
                    data: child.data,
                })
                .unwrap(),
        }
    }
}

#[test]
fn incremental_and_write_only_wavl_writers_preserve_native_frames_and_segments() {
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
            for channels in [1u16, 2, 6, 33] {
                let (bytes, _, truth) = fixture(container, sample, channels, false);
                let wave = Wave::parse(&bytes).unwrap();
                let spec = Spec {
                    sample_format: sample,
                    channels,
                    sample_rate: 48000,
                    valid_bits: sample.bits(),
                    channel_mask: wave.format.channel_mask,
                };
                let options = WriteOptions {
                    container,
                    auto_rf64: false,
                    force_extensible: wave.format.tag == 0xfffe,
                };
                let metadata = [RawChunk {
                    id: *b"JUNK",
                    data: b"odd",
                }];
                let mut seekable =
                    WavlWriter::new(Cursor::new(Vec::new()), spec, options, &metadata).unwrap();
                let mut sequential = WavlWriter::with_lengths(
                    Vec::new(),
                    spec,
                    options,
                    &metadata,
                    DataLength {
                        bytes: wave.audio_chunk().data.len() as u64,
                        frames: 20,
                    },
                )
                .unwrap();
                copy_segments(&mut seekable, &wave);
                copy_segments(&mut sequential, &wave);
                assert_eq!(seekable.frames_written(), 20);
                assert_eq!(
                    seekable.list_bytes_written(),
                    wave.audio_chunk().data.len() as u64
                );
                let encoded = seekable.finish().unwrap().into_inner();
                assert_eq!(sequential.finish_stream().unwrap(), encoded);
                let output = Wave::parse(&encoded).unwrap();
                assert_eq!(output.format.raw, wave.format.raw);
                assert_eq!(output.audio_chunk().data, wave.audio_chunk().data);
                let mut got = vec![0; truth.len()];
                assert_eq!(output.pcm().unwrap().read_frames(0, &mut got).unwrap(), 20);
                assert_eq!(got, truth);
                let mut file = Reader::new(Cursor::new(&encoded)).unwrap();
                let mut got = vec![0; truth.len()];
                file.read_frames(&mut got).unwrap();
                assert_eq!(got, truth);
            }
        }
    }
}

#[test]
fn typed_wavl_writes_preserve_float_bits_and_pack_frames_larger_than_stack_scratch() {
    for sample in [
        SampleFormat::U8,
        SampleFormat::I16,
        SampleFormat::I24,
        SampleFormat::I32,
        SampleFormat::F32,
        SampleFormat::F64,
    ] {
        for container in [Container::Riff, Container::Rifx] {
            let channels = if sample == SampleFormat::F64 {
                4000
            } else {
                10000
            };
            let format = fmt(sample, channels, sample.bits(), container.endian());
            let bytes = audio(sample, channels, 3, container.endian());
            let pcm =
                Pcm::new(&bytes, Format::parse(&format, container.endian()).unwrap()).unwrap();
            let spec = Spec::new(sample, channels, 48000);
            let options = WriteOptions {
                container,
                auto_rf64: false,
                force_extensible: true,
            };
            let mut writer = WavlWriter::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
            match sample {
                SampleFormat::F32 => {
                    let mut samples = vec![0f32; pcm.samples() as usize];
                    pcm.read_f32(0, &mut samples).unwrap();
                    writer.write_f32(&samples).unwrap();
                }
                SampleFormat::F64 => {
                    let mut samples = vec![0f64; pcm.samples() as usize];
                    pcm.read_f64(0, &mut samples).unwrap();
                    writer.write_f64(&samples).unwrap();
                }
                SampleFormat::I16 => {
                    let mut samples = vec![0i16; pcm.samples() as usize];
                    pcm.read_i16(0, &mut samples).unwrap();
                    writer.write_i16(&samples).unwrap();
                }
                _ => {
                    let mut samples = vec![0i32; pcm.samples() as usize];
                    pcm.read_i32(0, &mut samples).unwrap();
                    writer.write_integers(&samples).unwrap();
                }
            }
            writer.write_silence(2).unwrap();
            let encoded = writer.finish().unwrap().into_inner();
            let wave = Wave::parse(&encoded).unwrap();
            let decoded = wave.pcm().unwrap();
            let align = usize::from(channels) * usize::from(sample.bits() / 8);
            let mut truth = bytes.clone();
            truth.extend(&bytes[bytes.len() - align..]);
            truth.extend(&bytes[bytes.len() - align..]);
            let mut got = vec![0; truth.len()];
            assert_eq!(decoded.read_frames(0, &mut got).unwrap(), 5);
            assert_eq!(got, truth);
        }
    }
}

#[test]
fn wavl_silence_count_promotion_and_declared_limits_are_checked_before_io() {
    let spec = Spec::new(SampleFormat::I16, 1, 8000);
    let frames = u64::from(u32::MAX) + 17;
    let mut writer =
        WavlWriter::new(Cursor::new(Vec::new()), spec, WriteOptions::default(), &[]).unwrap();
    writer.write_silence(frames).unwrap();
    assert_eq!(writer.list_bytes_written(), 28);
    let bytes = writer.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    assert_eq!(wave.container, Container::Rf64);
    assert_eq!(wave.fact, Some(frames));
    assert_eq!(
        wave.wavl_chunks()
            .unwrap()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        [*b"slnt", *b"slnt"]
    );
    let pcm = wave.pcm().unwrap();
    let mut last = [42; 3];
    assert_eq!(pcm.read_i16(frames - 3, &mut last).unwrap(), 3);
    assert_eq!(last, [0; 3]);
    let mut known = WavlWriter::with_lengths(
        Vec::new(),
        spec,
        WriteOptions::default(),
        &[],
        DataLength { bytes: 28, frames },
    )
    .unwrap();
    known.write_silence(frames).unwrap();
    assert_eq!(known.finish_stream().unwrap(), bytes);
    let options = WriteOptions {
        auto_rf64: false,
        ..WriteOptions::default()
    };
    let mut writer = WavlWriter::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
    assert!(writer.write_silence(u64::from(u32::MAX)).is_err());
    assert_eq!(writer.frames_written(), 0);
    assert_eq!(writer.list_bytes_written(), 4);
    writer.finish().unwrap();
    struct Count(std::rc::Rc<std::cell::Cell<usize>>);
    impl Write for Count {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.set(self.0.get() + b.len());
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    assert!(
        WavlWriter::with_lengths(
            Count(count.clone()),
            spec,
            options,
            &[],
            DataLength {
                bytes: u64::MAX - 1,
                frames: 1
            }
        )
        .is_err()
    );
    assert_eq!(count.get(), 0);
    let mut writer = WavlWriter::with_lengths(
        Count(count.clone()),
        spec,
        options,
        &[],
        DataLength {
            bytes: 16,
            frames: 1,
        },
    )
    .unwrap();
    let before = count.get();
    assert!(writer.write_frames(&[1]).is_err());
    assert!(writer.write_silence(2).is_err());
    assert!(writer.write_f32(&[1.0]).is_err());
    assert!(
        writer
            .write_chunk(RawChunk {
                id: *b"data",
                data: &[]
            })
            .is_err()
    );
    assert_eq!(count.get(), before);
    assert!(writer.finish_stream().is_err());
    assert_eq!(count.get(), before);
}

#[test]
fn wavl_io_failures_poison_streams_and_seek_recovers_held_frames() {
    use std::{cell::Cell, rc::Rc};
    struct Fault {
        inner: Cursor<Vec<u8>>,
        fail: Rc<Cell<bool>>,
        flush_fail: bool,
        calls: usize,
    }
    impl Read for Fault {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            if self.fail.get() {
                return Err(io::Error::other("injected read failure"));
            }
            self.inner.read(b)
        }
    }
    impl Write for Fault {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.fail.get() {
                return Err(io::Error::other("injected write failure"));
            }
            self.calls += 1;
            if self.calls.is_multiple_of(13) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.inner.write(&b[..b.len().min(3)])
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.flush_fail {
                Err(io::Error::other("injected flush failure"))
            } else {
                Ok(())
            }
        }
    }
    impl Seek for Fault {
        fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
            self.inner.seek(p)
        }
    }
    let (bytes, _, truth) = fixture(Container::Riff, SampleFormat::I16, 2, false);
    let fail = Rc::new(Cell::new(false));
    let mut reader = Reader::new(Fault {
        inner: Cursor::new(bytes.clone()),
        fail: fail.clone(),
        flush_fail: false,
        calls: 0,
    })
    .unwrap();
    fail.set(true);
    assert!(reader.seek_frame(10).is_err());
    assert!(reader.read_frames(&mut [0; 4]).is_err());
    fail.set(false);
    reader.seek_frame(10).unwrap();
    let mut held = [0; 4];
    reader.read_frames(&mut held).unwrap();
    assert_eq!(held, truth[40..44]);
    let mut forward = ForwardReader::new(Fault {
        inner: Cursor::new(bytes),
        fail: fail.clone(),
        flush_fail: false,
        calls: 0,
    })
    .unwrap();
    fail.set(true);
    assert!(forward.read_frames(&mut held).is_err());
    fail.set(false);
    assert!(forward.read_frames(&mut held).is_err());
    assert!(forward.finish().is_err());
    let spec = Spec::new(SampleFormat::I16, 1, 8000);
    let options = WriteOptions {
        auto_rf64: false,
        ..WriteOptions::default()
    };
    let sink = || Fault {
        inner: Cursor::new(Vec::new()),
        fail: fail.clone(),
        flush_fail: false,
        calls: 0,
    };
    let mut writer = WavlWriter::new(sink(), spec, options, &[]).unwrap();
    writer.write_i16(&[123, -456]).unwrap();
    writer.write_silence(2).unwrap();
    let encoded = writer.finish().unwrap().inner.into_inner();
    assert_eq!(Wave::parse(&encoded).unwrap().pcm().unwrap().frames(), 4);
    let mut writer = WavlWriter::new(sink(), spec, options, &[]).unwrap();
    fail.set(true);
    assert!(writer.write_silence(1).is_err());
    fail.set(false);
    assert!(writer.write_i16(&[3]).is_err());
    assert!(writer.finish().is_err());
    let mut broken_flush = sink();
    broken_flush.flush_fail = true;
    let mut known = WavlWriter::with_lengths(
        broken_flush,
        spec,
        options,
        &[],
        DataLength {
            bytes: 16,
            frames: 1,
        },
    )
    .unwrap();
    known.write_silence(1).unwrap();
    assert!(known.finish_stream().is_err());
}

#[test]
fn wavl_index_and_held_frame_seeks_cross_physical_four_gib_offsets() {
    use std::{cell::Cell, rc::Rc};
    struct Virtual {
        position: u64,
        size: u64,
        runs: Vec<(u64, Vec<u8>)>,
        reads: Rc<Cell<usize>>,
    }
    impl Read for Virtual {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let n = (self.size.saturating_sub(self.position)).min(b.len() as u64) as usize;
            b[..n].fill(0);
            for (start, bytes) in &self.runs {
                let left = self.position.max(*start);
                let right = (self.position + n as u64).min(*start + bytes.len() as u64);
                if left < right {
                    b[(left - self.position) as usize..(right - self.position) as usize]
                        .copy_from_slice(&bytes[(left - start) as usize..(right - start) as usize]);
                }
            }
            self.position += n as u64;
            self.reads.set(self.reads.get() + n);
            Ok(n)
        }
    }
    impl Seek for Virtual {
        fn seek(&mut self, at: SeekFrom) -> io::Result<u64> {
            let at = match at {
                SeekFrom::Start(n) => i128::from(n),
                SeekFrom::End(n) => i128::from(self.size) + i128::from(n),
                SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            };
            self.position = u64::try_from(at).map_err(|_| io::ErrorKind::InvalidInput)?;
            Ok(self.position)
        }
    }
    let data_bytes = u64::from(u32::MAX) - 15;
    let data_frames = data_bytes / 4;
    let frames = data_frames + u64::from(u32::MAX) + 1;
    let first_offset = 116u64;
    let slnt_offset = first_offset + data_bytes;
    let last_offset = slnt_offset + 12 + 8;
    let file_size = last_offset + 4;
    let list_bytes = file_size - 104;
    let mut ds = Vec::new();
    ds.extend((file_size - 8).to_le_bytes());
    ds.extend(0u64.to_le_bytes());
    ds.extend(frames.to_le_bytes());
    ds.extend(1u32.to_le_bytes());
    ds.extend(b"LIST");
    ds.extend(list_bytes.to_le_bytes());
    let mut header = b"RF64\xff\xff\xff\xffWAVE".to_vec();
    header.extend(ck(b"ds64", &ds, Endian::Little));
    header.extend(ck(
        b"fmt ",
        &fmt(SampleFormat::I32, 1, 32, Endian::Little),
        Endian::Little,
    ));
    header.extend(ck(b"fact", &u32::MAX.to_le_bytes(), Endian::Little));
    header.extend(b"LIST\xff\xff\xff\xffwavldata");
    header.extend((data_bytes as u32).to_le_bytes());
    assert_eq!(header.len() as u64, first_offset);
    let mut ending = ck(b"slnt", &u32::MAX.to_le_bytes(), Endian::Little);
    ending.extend(ck(b"data", &0x87654321u32.to_le_bytes(), Endian::Little));
    let reads = Rc::new(Cell::new(0));
    let source = Virtual {
        position: 0,
        size: file_size,
        reads: reads.clone(),
        runs: vec![
            (0, header),
            (first_offset, 0x11223344u32.to_le_bytes().to_vec()),
            (slnt_offset - 4, 0x12345678u32.to_le_bytes().to_vec()),
            (slnt_offset, ending),
        ],
    };
    let mut reader = Reader::new(source).unwrap();
    assert_eq!(reader.frames().unwrap(), frames);
    for (at, expected) in [
        (0, 0x11223344u32),
        (data_frames - 1, 0x12345678),
        (data_frames, 0x12345678),
        (frames - 2, 0x12345678),
        (frames - 1, 0x87654321),
    ] {
        reader.seek_frame(at).unwrap();
        let mut out = [0i32; 1];
        assert_eq!(reader.read_i32(&mut out).unwrap(), 1);
        assert_eq!(out[0] as u32, expected);
    }
    reader.seek_frame(frames).unwrap();
    assert_eq!(reader.read_i32(&mut [0; 1]).unwrap(), 0);
    assert!(
        reads.get() < 1024,
        "audio extents must be skipped with Seek"
    );
}
