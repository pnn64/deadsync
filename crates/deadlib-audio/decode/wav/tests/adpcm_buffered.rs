use deadlib_audio_decode_wav::{AdpcmReader, RawChunk, WavlMuxer, WriteOptions};
use std::{
    cell::Cell,
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    rc::Rc,
};

const FORMAT: [u8; 20] = [
    17, 0, 1, 0, 0x40, 0x1f, 0, 0, 0xc7, 0x1b, 0, 0, 8, 0, 4, 0, 2, 0, 9, 0,
];
#[derive(Default)]
struct Counts {
    bytes: Cell<usize>,
    seeks: Cell<usize>,
    fail_at: Cell<Option<u64>>,
}
struct Source {
    inner: Cursor<Vec<u8>>,
    counts: Rc<Counts>,
}
impl Read for Source {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = if let Some(at) = self.counts.fail_at.get() {
            if self.inner.position() >= at {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            out.len().min((at - self.inner.position()) as usize)
        } else {
            out.len()
        };
        let n = self.inner.read(&mut out[..n])?;
        self.counts.bytes.set(self.counts.bytes.get() + n);
        Ok(n)
    }
}
impl Seek for Source {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.counts.seeks.set(self.counts.seeks.get() + 1);
        self.inner.seek(to)
    }
}
fn flat(data: &[u8], frames: u32) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    for (id, payload) in [
        (*b"fmt ", FORMAT.as_slice()),
        (*b"fact", frames.to_le_bytes().as_slice()),
        (*b"data", data),
        (*b"note", b"metadata".as_slice()),
    ] {
        bytes.extend(id);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        if payload.len() & 1 != 0 {
            bytes.push(0);
        }
    }
    let size = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn buffered_blocks_preserve_read_ahead_and_cursor_state_through_seeks_and_errors() {
    let mut data = Vec::new();
    let mut samples = Vec::new();
    for block in 0..1025 {
        let predictor = (block * 971 - 30000) as i16;
        let header = predictor.to_le_bytes();
        data.extend([header[0], header[1], 0, 0]);
        let frames = if block == 1024 { 1 } else { 9 };
        if frames == 9 {
            data.extend([0; 4]);
        }
        samples.extend(std::iter::repeat_n(predictor, frames));
    }
    for segmented in [false, true] {
        let (bytes, expected) = if segmented {
            let split = 512 * 8;
            let mut mux = WavlMuxer::new(
                Cursor::new(Vec::new()),
                &FORMAT,
                WriteOptions::default(),
                &[],
            )
            .unwrap();
            mux.write_data(&data[..split], 512 * 9).unwrap();
            mux.write_silence(3).unwrap();
            mux.write_chunk(RawChunk {
                id: *b"note",
                data: b"metadata",
            })
            .unwrap();
            mux.write_data(&data[split..], 512 * 9 + 1).unwrap();
            let mut expected = samples[..512 * 9].to_vec();
            expected.extend([samples[512 * 9 - 1]; 3]);
            expected.extend(&samples[512 * 9..]);
            (mux.finish().unwrap().into_inner(), expected)
        } else {
            (flat(&data, samples.len() as u32), samples.clone())
        };
        // A nonzero source base must not be confused with form-relative cursors.
        let base = 37;
        let mut prefixed = vec![0x35; base];
        prefixed.extend(bytes);
        let mut inner = Cursor::new(prefixed);
        inner.set_position(base as u64);
        let counts = Rc::new(Counts::default());
        let source = Source {
            inner,
            counts: Rc::clone(&counts),
        };
        let mut reader = AdpcmReader::new(BufReader::with_capacity(128, source)).unwrap();
        let first_data = if segmented {
            reader
                .wavl_chunks()
                .unwrap()
                .find(|c| c.id == *b"data")
                .unwrap()
        } else {
            *reader.chunks().iter().find(|c| c.id == *b"data").unwrap()
        };
        let metadata = if segmented {
            reader
                .wavl_chunks()
                .unwrap()
                .find(|c| c.id == *b"note")
                .unwrap()
        } else {
            *reader.chunks().iter().find(|c| c.id == *b"note").unwrap()
        };
        let opening = (counts.bytes.get(), counts.seeks.get());
        let mut out = vec![0; expected.len()];
        assert_eq!(reader.read_i16(&mut out).unwrap(), expected.len());
        assert_eq!(out, expected);
        let children = if segmented { 2 } else { 1 };
        assert_eq!(counts.seeks.get() - opening.1, children);
        assert!(counts.bytes.get() - opening.0 <= data.len() + 128 * children);
        let end = (counts.bytes.get(), counts.seeks.get());
        assert_eq!(reader.read_i16(&mut out).unwrap(), 0);
        assert_eq!(reader.read_i16(&mut []).unwrap(), 0);
        assert_eq!((counts.bytes.get(), counts.seeks.get()), end);

        // An explicit seek positions an uncached block; loading it needs no
        // second seek. A later cache hit after a rewind must not fake a read.
        reader.seek_frame(99 * 9).unwrap();
        let seeks = counts.seeks.get();
        assert_eq!(reader.read_i16(&mut out[..1]).unwrap(), 1);
        assert_eq!(out[0], expected[99 * 9]);
        assert_eq!(counts.seeks.get(), seeks);
        reader.seek_frame(99 * 9 + 1).unwrap();
        let seeks = counts.seeks.get();
        assert_eq!(reader.read_i16(&mut out[..17]).unwrap(), 17);
        assert_eq!(&out[..17], &expected[99 * 9 + 1..101 * 9]);
        assert_eq!(counts.seeks.get(), seeks + 1);

        // Metadata restores the physical byte cursor, so the following block
        // can continue without seeking again even though read-ahead was lost.
        let mut note = [0; 8];
        reader.read_metadata(metadata, &mut note).unwrap();
        assert_eq!(&note, b"metadata");
        let seeks = counts.seeks.get();
        reader.read_i16(&mut out[..1]).unwrap();
        assert_eq!(out[0], expected[101 * 9]);
        assert_eq!(counts.seeks.get(), seeks);

        // Fail after a partial encoded read below the buffer. Neither a retry
        // nor successful metadata access can recover the poisoned audio state.
        reader.seek_frame(200 * 9).unwrap();
        counts
            .fail_at
            .set(Some(base as u64 + first_data.offset + 200 * 8 + 3));
        assert!(reader.read_i16(&mut out[..1]).is_err());
        counts.fail_at.set(None);
        assert!(reader.read_i16(&mut out[..1]).is_err());
        reader.read_metadata(metadata, &mut note).unwrap();
        assert!(reader.read_i16(&mut out[..1]).is_err());
        reader.seek_frame(200 * 9).unwrap();
        let seeks = counts.seeks.get();
        assert_eq!(reader.read_i16(&mut out[..17]).unwrap(), 17);
        assert_eq!(&out[..17], &expected[200 * 9..200 * 9 + 17]);
        assert_eq!(counts.seeks.get(), seeks);
    }
}
