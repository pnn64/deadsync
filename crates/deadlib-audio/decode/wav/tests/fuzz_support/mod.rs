//! Shared property checks for deterministic mutation tests and libFuzzer.
//! Input and codec scratch are capped here, independently of production limits.
use deadlib_audio_decode_wav::{
    Adpcm, AdpcmData, AdpcmKind, AdpcmReader, AdpcmSpec, Container, Endian, Format, Limits, Pcm,
    RawChunk, Reader, SampleFormat, Spec, Stream, Wave, WriteOptions, Writer,
    metadata::{self, Metadata},
};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

pub const MAX_INPUT: usize = 32768;
const LIMITS: Limits = Limits {
    chunks: 64,
    size_table_entries: 16,
    format_bytes: 4096,
};
pub const PATHS: [&str; 15] = [
    "container",
    "pcm",
    "adpcm_file",
    "list",
    "cue",
    "sampler",
    "bext",
    "chna",
    "bxml",
    "sxml",
    "markers",
    "block",
    "block_encoded",
    "native_encoded",
    "raw_metadata",
];

/// Returns a bit for each successfully entered family, not a code-coverage count.
pub fn exercise(input: &[u8]) -> u64 {
    if input.is_empty() || input.len() > MAX_INPUT {
        return 0;
    }
    let data = &input[1..];
    match input[0] % 4 {
        0 => container(data),
        1 => metadata(data),
        2 => block(data),
        _ => encode(data),
    }
}

struct ShortReads<'a> {
    inner: Cursor<&'a [u8]>,
    width: usize,
    calls: usize,
}
impl<'a> ShortReads<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            inner: Cursor::new(data),
            width: 1 + data.last().copied().unwrap_or(0) as usize % 31,
            calls: 0,
        }
    }
}
impl Read for ShortReads<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(17) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = out.len().min(self.width);
        self.inner.read(&mut out[..n])
    }
}
impl Seek for ShortReads<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

fn container(data: &[u8]) -> u64 {
    let wave = Wave::with_limits(data, LIMITS);
    let reader = Reader::with_limits(ShortReads::new(data), LIMITS);
    assert_eq!(
        wave.is_ok(),
        reader.is_ok(),
        "borrowed/file container acceptance"
    );
    let (Ok(wave), Ok(mut reader)) = (wave, reader) else {
        // Even a rejected audio descriptor can have structurally valid chunks.
        if let Ok(mut stream) = Stream::with_limits(ShortReads::new(data), LIMITS) {
            let mut prefix = [0; 23];
            while let Ok(Some(_)) = stream.next_chunk() {
                if stream.read_chunk(&mut prefix).is_err() {
                    break;
                }
            }
        }
        return 0;
    };
    assert_eq!(wave.container, reader.container());
    assert_eq!(wave.format.raw, reader.format().raw);
    assert_eq!(wave.fact, reader.fact());
    assert_eq!(
        wave.ds64.map(|d| d.sample_count),
        reader.ds64_sample_count()
    );
    assert_eq!(wave.data_offset(), reader.data_chunk().offset);
    assert_eq!(wave.data().len() as u64, reader.data_chunk().size);
    let mut paths = 1;
    let mut stream = Stream::with_limits(ShortReads::new(data), LIMITS).unwrap();
    assert_eq!(stream.form_bytes(), wave.as_bytes().len() as u64);
    let chunks: Vec<_> = wave
        .chunks()
        .unwrap()
        .map(Result::unwrap)
        .filter(|c| c.id != *b"ds64")
        .collect();
    assert_eq!(chunks.len(), reader.chunks().len());
    for (i, c) in chunks.iter().enumerate() {
        let descriptor = reader.chunks()[i];
        assert_eq!(c.id, descriptor.id);
        assert_eq!(c.offset, descriptor.offset);
        assert_eq!(c.data.len() as u64, descriptor.size);
        let sc = stream.next_chunk().unwrap().unwrap();
        assert_eq!(sc, descriptor);
        let mut scratch = [0; 257];
        let n = stream.read_chunk(&mut scratch).unwrap();
        assert_eq!(&scratch[..n], &c.data[..n]);
        // next_chunk drains the remainder, including odd padding, through short reads.
        let n = reader.read_metadata(descriptor, &mut scratch).unwrap();
        assert_eq!(&scratch[..n], &c.data[..n]);
        if let Ok(view) = c.metadata(wave.format.endian, LIMITS) {
            assert_eq!(view.raw(), c.data);
            paths |= visit_metadata(view, wave.format.endian);
        }
    }
    assert!(stream.next_chunk().unwrap().is_none());
    assert_eq!(
        stream.into_inner().inner.position(),
        wave.as_bytes().len() as u64
    );
    if let Ok(pcm) = wave.pcm() {
        paths |= 1 << 1;
        pcm_ranges(pcm, &mut reader);
    } else if let Ok(audio) = AdpcmData::new(&wave) {
        let codec = audio.codec();
        if codec.scratch_samples() <= 8192 && codec.block_bytes() <= 4096 {
            paths |= 1 << 2;
            let mut file = AdpcmReader::from_reader(reader).unwrap();
            assert_eq!(file.frames(), audio.frames());
            let mut scratch = [0i16; 8192];
            for start in [
                0,
                audio.frames() / 2,
                audio.frames(),
                audio.frames().saturating_add(1),
                u64::MAX,
            ] {
                let mut a = [0i16; 1025];
                let mut b = [0i16; 1025];
                let expected = audio.read_frames(start, &mut a, &mut scratch);
                let actual = file.seek_frame(start).and_then(|()| file.read_i16(&mut b));
                assert_eq!(expected.is_ok(), actual.is_ok(), "ADPCM range acceptance");
                if let (Ok(a_len), Ok(b_len)) = (expected, actual) {
                    assert_eq!(a_len, b_len);
                    assert_eq!(&a[..a_len], &b[..b_len]);
                }
            }
        }
    }
    paths
}

fn pcm_ranges(pcm: Pcm<'_>, reader: &mut Reader<ShortReads<'_>>) {
    assert_eq!(pcm.frames(), reader.frames().unwrap());
    for start in [
        0,
        pcm.frames() / 2,
        pcm.frames(),
        pcm.frames().saturating_add(1),
        u64::MAX,
    ] {
        let mut raw = [0; 1031];
        let align = usize::from(pcm.format().block_align);
        let range = pcm.frame_range(
            start,
            pcm.frames()
                .saturating_sub(start)
                .min((raw.len() / align) as u64),
        );
        let actual = reader
            .seek_frame(start)
            .and_then(|()| reader.read_frames(&mut raw));
        assert_eq!(range.is_ok(), actual.is_ok(), "native range acceptance");
        if let (Ok(view), Ok(frames)) = (range, actual) {
            assert_eq!(view.frames() as usize, frames);
            assert_eq!(view.as_bytes(), &raw[..frames * align]);
        }
        let mut integers = [0i32; 257];
        let mut file_integers = [0i32; 257];
        let a = pcm.read_i32(start, &mut integers);
        let b = reader
            .seek_frame(start)
            .and_then(|()| reader.read_integers(&mut file_integers));
        assert_eq!(a.is_ok(), b.is_ok());
        if let (Ok(n), Ok(m)) = (a, b) {
            assert_eq!(n, m);
            assert_eq!(&integers[..n], &file_integers[..m]);
        }
        match pcm.sample_format() {
            SampleFormat::F32 => {
                let mut a = [0f32; 257];
                let mut b = [0f32; 257];
                let a_len = pcm.read_f32(start, &mut a);
                let b_len = reader
                    .seek_frame(start)
                    .and_then(|()| reader.read_f32(&mut b));
                assert_eq!(a_len.is_ok(), b_len.is_ok());
                if let (Ok(n), Ok(m)) = (a_len, b_len) {
                    assert_eq!(n, m);
                    assert!(
                        a[..n]
                            .iter()
                            .zip(&b[..m])
                            .all(|(a, b)| a.to_bits() == b.to_bits())
                    );
                }
            }
            SampleFormat::F64 => {
                let mut a = [0f64; 257];
                let mut b = [0f64; 257];
                let a_len = pcm.read_f64(start, &mut a);
                let b_len = reader
                    .seek_frame(start)
                    .and_then(|()| reader.read_f64(&mut b));
                assert_eq!(a_len.is_ok(), b_len.is_ok());
                if let (Ok(n), Ok(m)) = (a_len, b_len) {
                    assert_eq!(n, m);
                    assert!(
                        a[..n]
                            .iter()
                            .zip(&b[..m])
                            .all(|(a, b)| a.to_bits() == b.to_bits())
                    );
                }
            }
            _ => {}
        }
    }
    // Misaligned input must either reject the direct view or expose identical bits.
    if let Some(values) = pcm.as_i16() {
        let mut out = vec![0i16; values.len()];
        assert_eq!(pcm.read_i16(0, &mut out).unwrap(), values.len());
        assert_eq!(values, out);
    }
    if let Some(values) = pcm.as_i32() {
        let mut out = vec![0i32; values.len()];
        assert_eq!(pcm.read_i32(0, &mut out).unwrap(), values.len());
        assert_eq!(values, out);
    }
    if let Some(values) = pcm.as_f32() {
        let mut out = vec![0f32; values.len()];
        assert_eq!(pcm.read_f32(0, &mut out).unwrap(), values.len());
        assert!(
            values
                .iter()
                .zip(out)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
    if let Some(values) = pcm.as_f64() {
        let mut out = vec![0f64; values.len()];
        assert_eq!(pcm.read_f64(0, &mut out).unwrap(), values.len());
        assert!(
            values
                .iter()
                .zip(out)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
}

fn metadata(data: &[u8]) -> u64 {
    if data.len() < 2 {
        return 0;
    }
    let ids = [
        *b"LIST", *b"cue ", *b"smpl", *b"bext", *b"chna", *b"bxml", *b"sxml", *b"r64m", *b"axml",
        *b"JUNK",
    ];
    let endian = if data[1] & 1 == 0 {
        Endian::Little
    } else {
        Endian::Big
    };
    let chunk = deadlib_audio_decode_wav::Chunk {
        id: ids[data[0] as usize % ids.len()],
        offset: 8,
        data: &data[2..],
        padding: &[],
    };
    match chunk.metadata(endian, LIMITS) {
        Ok(view) => {
            assert_eq!(view.raw(), chunk.data);
            visit_metadata(view, endian)
        }
        Err(_) => 0,
    }
}

fn visit_metadata(view: Metadata<'_>, endian: Endian) -> u64 {
    let mut out = vec![0xa5; view.raw().len() + 4];
    match view {
        Metadata::List(v) => {
            let entries: Vec<_> = v.entries().map(Result::unwrap).collect();
            for e in &entries {
                let _ = e.text_bytes();
                let _ = e.associated(endian);
            }
            let raw: Vec<_> = entries
                .iter()
                .map(|e| RawChunk {
                    id: e.id,
                    data: e.data,
                })
                .collect();
            let n = metadata::write_list(v.kind(), &raw, endian, &mut out).unwrap();
            let copy = metadata::List::parse(&out[..n], endian, LIMITS).unwrap();
            assert_eq!(copy.kind(), v.kind());
            for (a, b) in entries.iter().zip(copy.entries()) {
                let b = b.unwrap();
                assert_eq!(a.id, b.id);
                assert_eq!(a.data, b.data);
            }
            1 << 3
        }
        Metadata::Cues(v) => {
            let n = metadata::write_cues(&v.iter().collect::<Vec<_>>(), endian, &mut out).unwrap();
            assert_eq!(&out[..n], v.raw());
            1 << 4
        }
        Metadata::Sampler(v) => {
            let n = metadata::write_sampler(
                v.header(),
                &v.loops().collect::<Vec<_>>(),
                v.specific_data(),
                endian,
                &mut out,
            )
            .unwrap();
            assert_eq!(&out[..n], v.raw());
            1 << 5
        }
        Metadata::Bext(v) => {
            let spec = metadata::BextSpec {
                description: v.description(),
                originator: v.originator(),
                originator_reference: v.originator_reference(),
                date: *v.date(),
                time: *v.time(),
                time_reference: v.time_reference(),
                version: v.version(),
                umid: v.umid().copied().unwrap_or([0; 64]),
                loudness: v.loudness().unwrap_or(metadata::Loudness::UNKNOWN),
                coding_history: v.coding_history(),
            };
            let _ = v.reserved();
            let _ = v.loudness().map(|v| v.values());
            if let Ok(n) = spec.write(&mut out) {
                let copy = metadata::Bext::parse(&out[..n]).unwrap();
                assert_eq!(copy.description(), v.description());
                assert_eq!(copy.time_reference(), v.time_reference());
                assert_eq!(copy.umid(), v.umid());
                assert_eq!(copy.loudness(), v.loudness());
                assert_eq!(copy.coding_history(), v.coding_history());
            } else {
                assert!(out.iter().all(|&v| v == 0xa5));
            }
            1 << 6
        }
        Metadata::Chna(v) => {
            let ids = v.ids().collect::<Vec<_>>();
            let _ = v.validate_assignments(u16::MAX, &mut [0; 8192]);
            if let Ok(n) = metadata::write_chna(v.num_tracks(), &ids, ids.len(), &mut out) {
                assert_eq!(&out[..n], v.raw());
            } else {
                assert!(out.iter().all(|&v| v == 0xa5));
            }
            1 << 7
        }
        Metadata::Bxml(v) => {
            let n = metadata::write_bxml(v.compression(), v.data(), &mut out).unwrap();
            assert_eq!(&out[..n], v.raw());
            1 << 8
        }
        Metadata::Sxml(v) => {
            let sub = v.subchunks().collect::<Vec<_>>();
            for c in &sub {
                assert_eq!(v.subchunk_at(c.byte_offset).unwrap().data, c.data);
            }
            let _ = v.subchunk_at(u64::MAX);
            let args = sub.iter().map(|c| (c.samples, c.data)).collect::<Vec<_>>();
            let align = v.alignment_points().collect::<Vec<_>>();
            let n = metadata::write_sxml(v.compression(), &args, &align, &mut out).unwrap();
            // An absent optional alignment table is canonically written as count=0.
            let copy = metadata::Sxml::parse(&out[..n], LIMITS).unwrap();
            assert_eq!(copy.compression(), v.compression());
            assert_eq!(copy.alignment_points().collect::<Vec<_>>(), align);
            assert_eq!(
                copy.subchunks()
                    .map(|c| (c.samples, c.data))
                    .collect::<Vec<_>>(),
                args
            );
            1 << 9
        }
        Metadata::Markers(v) => {
            let entries = v.iter().collect::<Vec<_>>();
            for m in &entries {
                let _ = m.is_valid();
                let _ = m.valid_byte_offset();
                let _ = m.inline_label();
            }
            if let Ok(n) = metadata::write_markers(&entries, &mut out) {
                assert_eq!(&out[..n], v.raw());
            } else {
                assert!(out.iter().all(|&v| v == 0xa5));
            }
            1 << 10
        }
        _ => 1 << 14,
    }
}

fn block(data: &[u8]) -> u64 {
    if data.len() < 3 {
        return 0;
    }
    let endian = if data[0] & 1 == 0 {
        Endian::Little
    } else {
        Endian::Big
    };
    let n = (u16::from_le_bytes([data[1], data[2]]) as usize)
        .min(data.len() - 3)
        .min(4096);
    let Ok(f) = Format::parse(&data[3..3 + n], endian) else {
        return 0;
    };
    let Ok(codec) = Adpcm::from_format(f) else {
        return 0;
    };
    if codec.scratch_samples() > 8192 || codec.block_bytes() > 4096 {
        return 0;
    }
    let encoded = &data[3 + n..];
    let mut out = [0i16; 8192];
    let result = codec.decode_block(encoded, &mut out);
    if let Ok(frames) = result {
        assert_eq!(frames, codec.block_frames(encoded.len()).unwrap());
        let mut too_small = [0i16; 1];
        if frames * codec.channels() > 1 {
            assert!(codec.decode_block(encoded, &mut too_small).is_err());
        }
        // Test the encoder as a state machine property; lossy samples are not expected to round-trip.
        let mut bytes = [0; 4096];
        let n = codec
            .encode_block(&out[..frames * codec.channels()], &mut bytes)
            .unwrap();
        assert_eq!(n, codec.block_bytes());
        assert_eq!(
            codec.decode_block(&bytes[..n], &mut out).unwrap(),
            codec.frames_per_block()
        );
        return (1 << 11) | (1 << 12);
    }
    1 << 11
}

fn encode(data: &[u8]) -> u64 {
    if data.len() < 10 {
        return 0;
    }
    let options = WriteOptions {
        container: [
            Container::Riff,
            Container::Rifx,
            Container::Rf64,
            Container::Bw64,
        ][data[0] as usize % 4],
        auto_rf64: data[1] & 1 != 0,
        force_extensible: data[1] & 2 != 0,
    };
    let channels = 1 + u16::from(data[2] % 16);
    let sample = [
        SampleFormat::U8,
        SampleFormat::I16,
        SampleFormat::I24,
        SampleFormat::I32,
        SampleFormat::F32,
        SampleFormat::F64,
    ][data[3] as usize % 6];
    let mut spec = Spec::new(
        sample,
        channels,
        1 + u32::from_le_bytes(data[4..8].try_into().unwrap()) % 384000,
    );
    if !sample.is_float() {
        spec.valid_bits = 1 + u16::from(data[8]) % sample.bits();
    }
    let align = usize::from(channels) * usize::from(sample.bits() / 8);
    let payload = &data[10..10 + (data.len() - 10) / align * align];
    let mut writer = Writer::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
    let frames = payload.len() / align;
    let split = if frames == 0 {
        0
    } else {
        usize::from(data[9]) % (frames + 1)
    } * align;
    writer.write_frames(&payload[..split]).unwrap();
    writer.write_frames(&payload[split..]).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    assert_eq!(wave.data(), payload);
    assert_eq!(wave.format.valid_bits, spec.valid_bits);
    let mut paths = (1 << 13) | container(&bytes);
    // Independently generated block parameters expose both encoder geometry checks and sample packing.
    let block_spec = AdpcmSpec {
        kind: if data[3] & 1 == 0 {
            AdpcmKind::Ima
        } else {
            AdpcmKind::Microsoft
        },
        channels,
        sample_rate: spec.sample_rate,
        block_bytes: 16 + u16::from(data[8]) * 8,
    };
    let mut fmt = [0; 50];
    if let Ok(n) = block_spec.format(options.container.endian(), &mut fmt) {
        let codec =
            Adpcm::from_format(Format::parse(&fmt[..n], options.container.endian()).unwrap())
                .unwrap();
        if codec.scratch_samples() <= 8192 {
            let source = data[10..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .take(codec.scratch_samples())
                .collect::<Vec<_>>();
            let n = source.len() / usize::from(channels) * usize::from(channels);
            if n != 0 {
                let mut encoded = [0; 4096];
                let mut decoded = [0; 8192];
                let len = codec.encode_block(&source[..n], &mut encoded).unwrap();
                assert_eq!(
                    codec.decode_block(&encoded[..len], &mut decoded).unwrap(),
                    codec.frames_per_block()
                );
                paths |= 1 << 12;
            }
        }
    }
    paths
}
