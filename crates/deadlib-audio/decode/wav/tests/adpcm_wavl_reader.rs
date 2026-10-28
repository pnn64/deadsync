use deadlib_audio_decode_wav::{AdpcmReader, Container, RawChunk, WavlMuxer, WriteOptions};
use std::{
    cell::Cell,
    io::{self, Cursor, Read, Seek, SeekFrom},
    rc::Rc,
};
const FORMAT: [u8; 20] = [
    17, 0, 1, 0, 0x40, 0x1f, 0, 0, 0xc7, 0x1b, 0, 0, 8, 0, 4, 0, 2, 0, 9, 0,
];
struct Observed {
    inner: Cursor<Vec<u8>>,
    reads: Rc<Cell<usize>>,
    seeks: Rc<Cell<usize>>,
    fail_read: Rc<Cell<bool>>,
    fail_seek: Rc<Cell<usize>>,
    calls: usize,
}
impl Read for Observed {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(17) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_read.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = b.len().min(3);
        let n = self.inner.read(&mut b[..n])?;
        self.reads.set(self.reads.get() + n);
        Ok(n)
    }
}
impl Seek for Observed {
    fn seek(&mut self, at: SeekFrom) -> io::Result<u64> {
        let n = self.seeks.get() + 1;
        self.seeks.set(n);
        if self.fail_seek.get() == n {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.inner.seek(at)
    }
}
#[test]
fn cache_uses_physical_block_identity_and_metadata_restores_position_or_poisons() {
    let mut mux = WavlMuxer::new(
        Cursor::new(Vec::new()),
        &FORMAT,
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    mux.write_silence(2).unwrap();
    mux.write_data(&[0xd2, 4, 0, 0, 0x11, 0x11, 0x11, 0x11], 9)
        .unwrap();
    mux.write_silence(4).unwrap();
    mux.write_chunk(RawChunk {
        id: *b"note",
        data: b"odd",
    })
    .unwrap();
    mux.write_data(&[], 0).unwrap();
    mux.write_silence(0).unwrap();
    mux.write_silence(2).unwrap();
    mux.write_data(&[0xe2, 0xff, 0, 0, 0x99, 0x99, 0x99, 0x99], 9)
        .unwrap();
    mux.write_silence(3).unwrap();
    let bytes = mux.finish().unwrap().into_inner();
    let reads = Rc::new(Cell::new(0));
    let seeks = Rc::new(Cell::new(0));
    let fail_read = Rc::new(Cell::new(false));
    let fail_seek = Rc::new(Cell::new(0));
    let source = Observed {
        inner: Cursor::new(bytes),
        reads: reads.clone(),
        seeks: seeks.clone(),
        fail_read: fail_read.clone(),
        fail_seek: fail_seek.clone(),
        calls: 0,
    };
    let mut file = AdpcmReader::new(source).unwrap();
    let opening = reads.get();
    assert_eq!(file.wavl_chunks().unwrap().count(), 9);
    let child = file
        .wavl_chunks()
        .unwrap()
        .find(|c| c.id == *b"note")
        .unwrap();
    let mut out = [99; 2];
    assert_eq!(file.read_i16(&mut out).unwrap(), 2);
    assert_eq!(out, [0, 0]);
    assert_eq!(reads.get(), opening);
    file.seek_frame(10).unwrap();
    assert_eq!(file.read_i16(&mut out[..1]).unwrap(), 1);
    assert_eq!(out[0], 1242);
    let block = reads.get();
    assert_eq!(block - opening, 8);
    for start in [11, 12, 13, 14, 15, 16, 2, 3, 10] {
        file.seek_frame(start).unwrap();
        file.read_i16(&mut out[..1]).unwrap();
        assert_eq!(reads.get(), block);
        assert_eq!(
            out[0],
            if start < 11 {
                1234 + (start - 2) as i16
            } else {
                1242
            }
        );
    }
    let position = file.position();
    let mut note = [0; 3];
    assert_eq!(file.read_metadata(child, &mut note).unwrap(), 3);
    assert_eq!(note, *b"odd");
    assert_eq!(file.position(), position);
    file.seek_frame(17).unwrap();
    file.read_i16(&mut out[..1]).unwrap();
    assert_eq!(out[0], -30);
    file.seek_frame(26).unwrap();
    file.read_i16(&mut out[..1]).unwrap();
    assert_eq!(out[0], -38);
    let block = reads.get();
    for start in [27, 28, 18, 26] {
        file.seek_frame(start).unwrap();
        file.read_i16(&mut out[..1]).unwrap();
        assert_eq!(reads.get(), block);
    }
    // stream_position, seek to metadata, then restoring seek: fail that third call.
    let position = file.position();
    fail_seek.set(seeks.get() + 3);
    assert!(file.read_metadata(child, &mut note).is_err());
    assert_eq!(file.position(), position);
    fail_seek.set(0);
    file.read_metadata(child, &mut note).unwrap();
    assert!(file.read_i16(&mut out).is_err());
    file.seek_frame(27).unwrap();
    file.read_i16(&mut out[..1]).unwrap();
    assert_eq!(out[0], -38);
    // A previously uncached dependent block fails; metadata success does not recover it.
    file.seek_frame(11).unwrap();
    fail_read.set(true);
    assert!(file.read_i16(&mut out[..1]).is_err());
    fail_read.set(false);
    assert!(file.read_i16(&mut out[..1]).is_err());
    file.read_metadata(child, &mut note).unwrap();
    assert!(file.read_i16(&mut out[..1]).is_err());
    file.seek_frame(11).unwrap();
    file.read_i16(&mut out[..1]).unwrap();
    assert_eq!(out[0], 1242);
    assert!(
        file.read_metadata(
            deadlib_audio_decode_wav::ChunkInfo {
                id: *b"fake",
                offset: 0,
                size: 1
            },
            &mut note
        )
        .is_err()
    );
}

struct Virtual {
    position: u64,
    size: u64,
    runs: Vec<(u64, Vec<u8>)>,
    reads: Rc<Cell<usize>>,
}
impl Read for Virtual {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        let n = self.size.saturating_sub(self.position).min(b.len() as u64) as usize;
        b[..n].fill(0);
        for (offset, data) in &self.runs {
            let left = self.position.max(*offset);
            let right = (self.position + n as u64).min(*offset + data.len() as u64);
            if left < right {
                b[(left - self.position) as usize..(right - self.position) as usize]
                    .copy_from_slice(&data[(left - offset) as usize..(right - offset) as usize]);
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
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.size) + i128::from(n),
        };
        self.position = u64::try_from(at).map_err(|_| io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}
fn chunk(id: &[u8; 4], b: &[u8]) -> Vec<u8> {
    let mut r = id.to_vec();
    r.extend((b.len() as u32).to_le_bytes());
    r.extend(b);
    if b.len() & 1 != 0 {
        r.push(0);
    }
    r
}

#[test]
fn child_offsets_above_four_gib_and_u64_held_silence_use_bounded_io() {
    for container in [Container::Rf64, Container::Bw64] {
        let mut header = if container == Container::Rf64 {
            b"RF64"
        } else {
            b"BW64"
        }
        .to_vec();
        header.extend(b"\xff\xff\xff\xffWAVE");
        header.extend(chunk(b"ds64", &[0; 40]));
        header.extend(chunk(b"fmt ", &FORMAT));
        header.extend(chunk(b"fact", &u32::MAX.to_le_bytes()));
        let list_offset = header.len() as u64 + 8;
        header.extend(b"LIST\xff\xff\xff\xffwavl");
        header.extend(chunk(b"slnt", &2u32.to_le_bytes()));
        header.extend(b"JUNK");
        header.extend((u32::MAX - 1).to_le_bytes());
        let end_offset = header.len() as u64 + u64::from(u32::MAX) - 1;
        let mut end = chunk(
            b"data",
            &[
                0xd2, 4, 0, 0, 0x11, 0x11, 0x11, 0x11, 0xe2, 0xff, 0, 0, 0x99, 0x99, 0x99, 0x99,
            ],
        );
        end.extend(chunk(b"slnt", &u32::MAX.to_le_bytes()));
        end.extend(chunk(b"data", &[0xd2, 4, 0, 0]));
        let size = end_offset + end.len() as u64;
        let frames = if container == Container::Rf64 {
            u64::from(u32::MAX) + 21
        } else {
            u64::from(u32::MAX)
        };
        let mut ds = Vec::new();
        ds.extend((size - 8).to_le_bytes());
        ds.extend(0u64.to_le_bytes());
        ds.extend(
            if container == Container::Rf64 {
                frames
            } else {
                u64::MAX
            }
            .to_le_bytes(),
        );
        ds.extend(1u32.to_le_bytes());
        ds.extend(b"LIST");
        ds.extend((size - list_offset).to_le_bytes());
        header[20..60].copy_from_slice(&ds);
        let reads = Rc::new(Cell::new(0));
        let source = Virtual {
            position: 0,
            size,
            runs: vec![(0, header), (end_offset, end)],
            reads: reads.clone(),
        };
        let mut file = AdpcmReader::new(source).unwrap();
        assert_eq!(file.frames(), frames);
        let data = file
            .wavl_chunks()
            .unwrap()
            .find(|c| c.id == *b"data")
            .unwrap();
        assert!(data.offset > u64::from(u32::MAX));
        for (at, expected) in [
            (0, 0),
            (1, 0),
            (2, 1234),
            (10, 1242),
            (11, -30),
            (19, -38),
            (20, -38),
            (frames - 2, -38),
            (
                frames - 1,
                if container == Container::Rf64 {
                    1234
                } else {
                    -38
                },
            ),
        ] {
            file.seek_frame(at).unwrap();
            let mut out = [99];
            assert_eq!(file.read_i16(&mut out).unwrap(), 1);
            assert_eq!(out[0], expected);
        }
        file.seek_frame(frames).unwrap();
        assert_eq!(file.read_i16(&mut [0; 3]).unwrap(), 0);
        assert!(
            reads.get() < 512,
            "must seek across physical extents and virtual silence"
        );
    }
}
