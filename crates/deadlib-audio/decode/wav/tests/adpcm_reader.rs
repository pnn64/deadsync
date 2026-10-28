use deadlib_audio_decode_wav::{
    AdpcmData, AdpcmKind, AdpcmReader, AdpcmSpec, AdpcmWriter, Container, Endian, Muxer, RawChunk,
    Wave, WavlMuxer, WriteOptions,
};
use std::{
    cell::Cell,
    io::{self, Cursor, Read, Seek, SeekFrom},
    rc::Rc,
};

#[test]
fn custom_microsoft_predictor_survives_cached_file_and_wavl_seeks() {
    for container in [
        Container::Riff,
        Container::Rifx,
        Container::Rf64,
        Container::Bw64,
    ] {
        let endian = if container == Container::Rifx {
            Endian::Big
        } else {
            Endian::Little
        };
        let word = |n: u16| match endian {
            Endian::Little => n.to_le_bytes(),
            Endian::Big => n.to_be_bytes(),
        };
        let spec = AdpcmSpec {
            kind: AdpcmKind::Microsoft,
            channels: 1,
            sample_rate: 8000,
            block_bytes: 8,
        };
        let mut format = [0; 54];
        assert_eq!(spec.format(endian, &mut format).unwrap(), 50);
        format[16..18].copy_from_slice(&word(36));
        format[20..22].copy_from_slice(&word(8));
        format[50..52].copy_from_slice(&word(256));
        // Predictor 7 uses the extra (256, 0) row, not a built-in table index.
        let mut block = vec![7];
        block.extend(word(16));
        block.extend(word(2000));
        block.extend(word((-1000i16) as u16));
        block.push(0x1f);
        let options = WriteOptions {
            container,
            ..WriteOptions::default()
        };
        for segmented in [false, true] {
            let bytes = if segmented {
                let mut mux =
                    WavlMuxer::new(Cursor::new(Vec::new()), &format, options, &[]).unwrap();
                for _ in 0..3 {
                    mux.write_data(&block, 4).unwrap();
                    mux.write_silence(2).unwrap();
                }
                mux.finish().unwrap().into_inner()
            } else {
                let mut mux = Muxer::new(Cursor::new(Vec::new()), &format, options, &[]).unwrap();
                mux.write_data(&block.repeat(3), 12).unwrap();
                mux.finish().unwrap().into_inner()
            };
            let group: &[i16] = if segmented {
                &[-1000, 2000, 2016, 2000, 2000, 2000]
            } else {
                &[-1000, 2000, 2016, 2000]
            };
            let expected = group.repeat(3);
            let mut reader = AdpcmReader::new(Cursor::new(bytes)).unwrap();
            let mut out = vec![0; expected.len()];
            assert_eq!(reader.read_i16(&mut out).unwrap(), expected.len());
            assert_eq!(out, expected);
            assert_eq!(reader.format().raw, format);
            for frame in (0..=expected.len()).rev() {
                reader.seek_frame(frame as u64).unwrap();
                let mut short = [12345; 3];
                let n = reader.read_i16(&mut short).unwrap();
                assert_eq!(n, (expected.len() - frame).min(short.len()));
                assert_eq!(&short[..n], &expected[frame..frame + n]);
                assert!(short[n..].iter().all(|&n| n == 12345));
            }
        }
    }
}

fn encoded(kind: AdpcmKind, container: Container, channels: u16) -> Vec<u8> {
    let spec = AdpcmSpec {
        kind,
        channels,
        sample_rate: 44100,
        block_bytes: 32 * channels,
    };
    let mut w = AdpcmWriter::new(
        Cursor::new(Vec::new()),
        spec,
        WriteOptions {
            container,
            ..WriteOptions::default()
        },
        &[RawChunk {
            id: *b"meta",
            data: b"hello",
        }],
    )
    .unwrap();
    let input: Vec<_> = (0..channels as usize * 173)
        .map(|i| (i * 7177 + 9123) as i16)
        .collect();
    w.write_i16(&input).unwrap();
    w.finish().unwrap().into_inner()
}

#[test]
fn file_backed_sequential_and_random_ranges_match_borrowed_audio() {
    for kind in [AdpcmKind::Microsoft, AdpcmKind::Ima] {
        for container in [
            Container::Riff,
            Container::Rifx,
            Container::Rf64,
            Container::Bw64,
        ] {
            for channels in [1u16, 2] {
                let bytes = encoded(kind, container, channels);
                let wave = Wave::parse(&bytes).unwrap();
                let view = AdpcmData::new(&wave).unwrap();
                let mut expected = vec![0; view.frames() as usize * channels as usize];
                view.read_frames(
                    0,
                    &mut expected,
                    &mut vec![0; view.codec().scratch_samples()],
                )
                .unwrap();
                let mut source = Cursor::new(vec![5; 7]);
                source.get_mut().extend(&bytes);
                source.set_position(7);
                let mut reader = AdpcmReader::new(source).unwrap();
                assert_eq!(reader.container(), container);
                assert_eq!(reader.frames(), 173);
                let mut actual: Vec<i16> = Vec::new();
                let mut out = vec![12345; channels as usize * 7 + 1];
                loop {
                    let n = reader.read_i16(&mut out).unwrap();
                    if n == 0 {
                        break;
                    }
                    actual.extend(&out[..n]);
                    if channels > 1 {
                        assert_eq!(out[out.len() - 1], 12345);
                    }
                }
                assert_eq!(actual, expected);
                for frame in [0, 1, 8, 9, 16, 17, 50, 51, 52, 57, 58, 170, 172, 173] {
                    reader.seek_frame(frame).unwrap();
                    let n = reader.read_i16(&mut out).unwrap();
                    assert_eq!(
                        &out[..n],
                        &expected[frame as usize * channels as usize
                            ..frame as usize * channels as usize + n]
                    );
                    let metadata = *reader.chunks().iter().find(|c| c.id == *b"meta").unwrap();
                    let pos = reader.position();
                    let mut data = [0; 5];
                    reader.read_metadata(metadata, &mut data).unwrap();
                    assert_eq!(&data, b"hello");
                    assert_eq!(reader.position(), pos);
                }
                assert!(reader.seek_frame(174).is_err());
            }
        }
    }
}

struct Observed {
    inner: Cursor<Vec<u8>>,
    read: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}
impl Read for Observed {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.fail.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let limit = b.len().min(3);
        let n = self.inner.read(&mut b[..limit])?;
        self.read.set(self.read.get() + n);
        Ok(n)
    }
}
impl Seek for Observed {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}

#[test]
fn block_cache_reuses_pcm_and_io_errors_require_explicit_recovery() {
    let bytes = encoded(AdpcmKind::Ima, Container::Riff, 2);
    let read = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(false));
    let source = Observed {
        inner: Cursor::new(bytes.clone()),
        read: read.clone(),
        fail: fail.clone(),
    };
    let mut r = AdpcmReader::new(source).unwrap();
    let opening = read.get();
    let mut out = [0; 2];
    r.read_i16(&mut out).unwrap();
    let one_block = read.get();
    assert_eq!(one_block - opening, 64);
    for _ in 0..20 {
        r.read_i16(&mut out).unwrap();
    }
    assert_eq!(read.get(), one_block);
    r.seek_frame(2).unwrap();
    r.read_i16(&mut out).unwrap();
    assert_eq!(read.get(), one_block);
    r.seek_frame(100).unwrap();
    fail.set(true);
    assert!(r.read_i16(&mut out).is_err());
    fail.set(false);
    assert!(r.read_i16(&mut out).is_err());
    r.seek_frame(100).unwrap();
    assert_eq!(r.read_i16(&mut out).unwrap(), 2);
    let w = Wave::parse(&bytes).unwrap();
    let v = AdpcmData::new(&w).unwrap();
    let mut expected = [0; 2];
    v.read_frames(
        100,
        &mut expected,
        &mut vec![0; v.codec().scratch_samples()],
    )
    .unwrap();
    assert_eq!(out, expected);
}

fn form(chunks: &[([u8; 4], &[u8])]) -> Vec<u8> {
    let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
    for (id, data) in chunks {
        b.extend(id);
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(*data);
        if data.len() & 1 != 0 {
            b.push(0);
        }
    }
    let size = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&size.to_le_bytes());
    b
}

#[test]
fn data_before_format_trailing_fact_short_last_blocks_and_bad_headers() {
    for kind in [AdpcmKind::Microsoft, AdpcmKind::Ima] {
        let bytes = encoded(kind, Container::Riff, 1);
        let w = Wave::parse(&bytes).unwrap();
        let reordered = form(&[
            (*b"data", w.data().unwrap()),
            (*b"odd!", &[42]),
            (*b"fmt ", w.format.raw),
            (*b"fact", &173u32.to_le_bytes()),
        ]);
        let mut r = AdpcmReader::new(Cursor::new(reordered)).unwrap();
        let mut out = [0; 173];
        assert_eq!(r.read_i16(&mut out).unwrap(), 173);
        let mut invalid = bytes.clone();
        let offset = w.data_offset().unwrap() as usize;
        invalid[offset + if kind == AdpcmKind::Ima { 2 } else { 0 }] = 255;
        let mut r = AdpcmReader::new(Cursor::new(invalid)).unwrap();
        assert!(r.read_i16(&mut out).is_err());
        assert!(r.read_i16(&mut out).is_err());
        let block_frames = AdpcmData::new(&w).unwrap().codec().frames_per_block() as u64;
        r.seek_frame(block_frames).unwrap();
        assert!(r.read_i16(&mut [0; 2]).is_ok());
    }
    let spec = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 1,
        sample_rate: 8000,
        block_bytes: 12,
    };
    let mut f = [0; 50];
    let n = spec.format(Endian::Little, &mut f).unwrap();
    let mut data = vec![0; 20];
    data[0..4].copy_from_slice(&[0xd2, 4, 0, 0]);
    data[12..16].copy_from_slice(&[0xd2, 4, 0, 0]);
    let bytes = form(&[
        (*b"fmt ", &f[..n]),
        (*b"fact", &26u32.to_le_bytes()),
        (*b"data", &data),
    ]);
    let mut r = AdpcmReader::new(Cursor::new(bytes)).unwrap();
    r.seek_frame(25).unwrap();
    let mut out = [0; 2];
    assert_eq!(r.read_i16(&mut out).unwrap(), 1);
    assert_eq!(out[0], 1234);
    let bytes = form(&[
        (*b"fmt ", &f[..n]),
        (*b"fact", &27u32.to_le_bytes()),
        (*b"data", &data),
    ]);
    assert!(AdpcmReader::new(Cursor::new(bytes)).is_err());
}

struct RepeatedFile {
    prefix: Vec<u8>,
    blocks: u64,
    pos: u64,
    reads: Rc<Cell<usize>>,
}
impl RepeatedFile {
    fn len(&self) -> u64 {
        self.prefix.len() as u64 + self.blocks * 8
    }
}
impl Read for RepeatedFile {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = ((out.len() as u64).min(self.len().saturating_sub(self.pos))) as usize;
        for (i, b) in out[..n].iter_mut().enumerate() {
            let p = self.pos + i as u64;
            *b = if p < self.prefix.len() as u64 {
                self.prefix[p as usize]
            } else {
                [0xd2, 4, 0, 0, 0x11, 0x11, 0x11, 0x11]
                    [((p - self.prefix.len() as u64) % 8) as usize]
            };
        }
        self.pos += n as u64;
        self.reads.set(self.reads.get() + n);
        Ok(n)
    }
}
impl Seek for RepeatedFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.pos = match from {
            SeekFrom::Start(n) => n,
            SeekFrom::Current(n) => self
                .pos
                .checked_add_signed(n)
                .ok_or(io::ErrorKind::InvalidInput)?,
            SeekFrom::End(n) => self
                .len()
                .checked_add_signed(n)
                .ok_or(io::ErrorKind::InvalidInput)?,
        };
        Ok(self.pos)
    }
}

#[test]
fn compressed_seeks_cross_four_gib_and_use_ds64_without_reading_audio() {
    for include_fact in [false, true] {
        let blocks = (1u64 << 29) + 7;
        let total_frames = blocks * 9;
        let payload = blocks * 8;
        let prefix_len = if include_fact { 96 } else { 84 };
        let mut prefix = b"RF64\xff\xff\xff\xffWAVEds64\x1c\0\0\0".to_vec();
        prefix.extend((payload + prefix_len - 8).to_le_bytes());
        prefix.extend(payload.to_le_bytes());
        prefix.extend(total_frames.to_le_bytes());
        prefix.extend(0u32.to_le_bytes());
        let spec = AdpcmSpec {
            kind: AdpcmKind::Ima,
            channels: 1,
            sample_rate: 8000,
            block_bytes: 8,
        };
        let mut f = [0; 50];
        let n = spec.format(Endian::Little, &mut f).unwrap();
        prefix.extend(b"fmt ");
        prefix.extend((n as u32).to_le_bytes());
        prefix.extend(&f[..n]);
        if include_fact {
            prefix.extend(b"fact\x04\0\0\0\xff\xff\xff\xff");
        }
        prefix.extend(b"data\xff\xff\xff\xff");
        assert_eq!(prefix.len() as u64, prefix_len);
        let reads = Rc::new(Cell::new(0));
        let source = RepeatedFile {
            prefix,
            blocks,
            pos: 0,
            reads: reads.clone(),
        };
        let mut r = AdpcmReader::new(source).unwrap();
        assert_eq!(r.frames(), total_frames);
        for frame in [total_frames - 1, (1u64 << 32) + 1, 9, 0] {
            r.seek_frame(frame).unwrap();
            let mut out = [0];
            assert_eq!(r.read_i16(&mut out).unwrap(), 1);
            assert_eq!(out[0], 1234 + (frame % 9) as i16);
        }
        assert!(reads.get() < 512, "must skip multi-GiB audio using Seek");
    }
}
