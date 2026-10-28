use deadlib_audio_decode_wav::{AdpcmForwardReader, Container, ForwardReader, Limits};
use std::{
    cell::Cell,
    io::{self, Cursor, Read},
    rc::Rc,
};
const FORMAT: [u8; 20] = [
    17, 0, 1, 0, 0x40, 0x1f, 0, 0, 0xc7, 0x1b, 0, 0, 8, 0, 4, 0, 2, 0, 9, 0,
];
fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut b = id.to_vec();
    b.extend((data.len() as u32).to_le_bytes());
    b.extend(data);
    if data.len() & 1 != 0 {
        b.push(0x81);
    }
    b
}
fn form(
    container: Container,
    list: &[u8],
    fact: Option<u32>,
    trailing: bool,
    hint: u64,
) -> Vec<u8> {
    let fact = fact.map_or(Vec::new(), |n| chunk(b"fact", &n.to_le_bytes()));
    let mut audio = chunk(b"LIST", list);
    if container.is_64() {
        audio[4..8].fill(255);
    }
    let mut body = chunk(b"head", b"odd");
    body.extend(chunk(b"fmt ", &FORMAT));
    if !trailing {
        body.extend(&fact);
    }
    body.extend(audio);
    if trailing {
        body.extend(fact);
    }
    body.extend(chunk(b"LIST", b"INFOtail"));
    let size = 4 + body.len() as u64 + if container.is_64() { 48 } else { 0 };
    let mut out = match container {
        Container::Riff => b"RIFF",
        Container::Rf64 => b"RF64",
        Container::Bw64 => b"BW64",
        _ => unreachable!(),
    }
    .to_vec();
    out.extend(
        if container.is_64() {
            u32::MAX
        } else {
            size as u32
        }
        .to_le_bytes(),
    );
    out.extend(b"WAVE");
    if container.is_64() {
        let mut ds = Vec::new();
        ds.extend(size.to_le_bytes());
        ds.extend(0u64.to_le_bytes());
        ds.extend(hint.to_le_bytes());
        ds.extend(1u32.to_le_bytes());
        ds.extend(b"LIST");
        ds.extend((list.len() as u64).to_le_bytes());
        out.extend(chunk(b"ds64", &ds));
    }
    out.extend(body);
    out
}
struct Short {
    inner: Cursor<Vec<u8>>,
    width: usize,
    calls: usize,
    read: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}
impl Read for Short {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(17) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let n = b.len().min(self.width);
        let n = self.inner.read(&mut b[..n])?;
        self.read.set(self.read.get() + n);
        Ok(n)
    }
}
fn source(bytes: Vec<u8>, width: usize) -> Short {
    Short {
        inner: Cursor::new(bytes),
        width,
        calls: 0,
        read: Rc::new(Cell::new(0)),
        fail: Rc::new(Cell::new(false)),
    }
}
fn sequence() -> (Vec<u8>, Vec<i16>) {
    let mut list = b"wavl".to_vec();
    list.extend(chunk(b"slnt", &2u32.to_le_bytes()));
    list.extend(chunk(
        b"data",
        &[
            0xd2, 4, 0, 0, 0x11, 0x11, 0x11, 0x11, 0xe2, 0xff, 0, 0, 0x99, 0x99, 0x99, 0x99,
        ],
    ));
    list.extend(chunk(b"data", &[]));
    list.extend(chunk(b"slnt", &0u32.to_le_bytes()));
    list.extend(chunk(b"note", b"odd"));
    list.extend(chunk(b"slnt", &3u32.to_le_bytes()));
    list.extend(chunk(b"LIST", b"INFOchild"));
    list.extend(chunk(
        b"data",
        &3210i16
            .to_le_bytes()
            .into_iter()
            .chain([0, 0])
            .collect::<Vec<_>>(),
    ));
    list.extend(chunk(b"slnt", &2u32.to_le_bytes()));
    let mut truth = vec![0; 2];
    truth.extend(1234..=1242);
    truth.extend((-38..=-30).rev());
    truth.extend([-38; 3]);
    truth.extend([3210; 3]);
    (list, truth)
}
#[test]
fn forward_segment_callbacks_short_reads_and_partial_finish_preserve_form_boundary() {
    let (list, truth) = sequence();
    for container in [Container::Riff, Container::Rf64, Container::Bw64] {
        for width in [1, 3, 17] {
            for batch in [1, 7, 127] {
                let bytes = form(
                    container,
                    &list,
                    Some(truth.len() as u32),
                    true,
                    truth.len() as u64,
                );
                let size = bytes.len();
                let mut with_tail = bytes.clone();
                with_tail.extend(b"NEXT");
                let mut events = Vec::new();
                let raw = ForwardReader::with_metadata(
                    source(with_tail, width),
                    Limits::default(),
                    |c, p| {
                        let mut b = Vec::new();
                        p.read_to_end(&mut b)?;
                        events.push((c.id, b));
                        Ok(())
                    },
                )
                .unwrap();
                let mut audio =
                    AdpcmForwardReader::from_reader_with_frames(raw, truth.len() as u64).unwrap();
                assert_eq!(audio.read_i16(&mut []).unwrap(), 0);
                let mut all = Vec::new();
                let mut out = vec![77; batch];
                loop {
                    let n = audio
                        .read_i16_with_metadata(&mut out, |c, p| {
                            let mut b = Vec::new();
                            p.read_to_end(&mut b)?;
                            events.push((c.id, b));
                            Ok(())
                        })
                        .unwrap();
                    if n == 0 {
                        break;
                    }
                    all.extend_from_slice(&out[..n]);
                }
                assert_eq!(all, truth);
                assert_eq!(audio.position(), truth.len() as u64);
                let mut inner = audio
                    .finish_with_metadata(|c, p| {
                        let mut b = Vec::new();
                        p.read_to_end(&mut b)?;
                        events.push((c.id, b));
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(
                    events,
                    vec![
                        (*b"head", b"odd".to_vec()),
                        (*b"note", b"odd".to_vec()),
                        (*b"LIST", b"INFOchild".to_vec()),
                        (*b"LIST", b"INFOtail".to_vec())
                    ]
                );
                assert_eq!(inner.inner.position(), size as u64);
                let mut tail = [0; 4];
                inner.read_exact(&mut tail).unwrap();
                assert_eq!(&tail, b"NEXT");
                let raw = ForwardReader::new(source(bytes, width)).unwrap();
                let mut audio =
                    AdpcmForwardReader::from_reader_with_frames(raw, truth.len() as u64).unwrap();
                audio.read_i16(&mut [0; 3]).unwrap();
                assert_eq!(audio.finish().unwrap().inner.position(), size as u64);
            }
        }
    }
    // Callback errors and physical read errors both poison a forward-only reader.
    let bytes = form(Container::Riff, &list, Some(truth.len() as u32), false, 0);
    let mut audio = AdpcmForwardReader::new(source(bytes.clone(), 3)).unwrap();
    assert!(
        audio
            .read_i16_with_metadata(&mut [0; 128], |_, _| Err(io::Error::from(
                io::ErrorKind::BrokenPipe
            )
            .into()))
            .is_err()
    );
    assert!(audio.read_i16(&mut [0; 1]).is_err());
    assert!(audio.finish().is_err());
    let src = source(bytes, 3);
    let fail = src.fail.clone();
    let mut audio = AdpcmForwardReader::new(src).unwrap();
    fail.set(true);
    assert!(audio.read_i16(&mut [0; 128]).is_err());
    fail.set(false);
    assert!(audio.read_i16(&mut [0; 1]).is_err());
    assert!(audio.finish().is_err());
}

#[test]
fn forward_finish_checks_geometry_capacity_and_fact_but_skips_unread_block_headers() {
    for data in [&[0u8][..], &[0, 0, 0][..], &[0, 0, 0, 0, 0][..]] {
        let list = [b"wavl".as_slice(), &chunk(b"data", data)].concat();
        let bytes = form(Container::Riff, &list, Some(0), false, 0);
        let audio = AdpcmForwardReader::new(Cursor::new(bytes)).unwrap();
        assert!(audio.finish().is_err());
    }
    let list = [b"wavl".as_slice(), &chunk(b"data", &[0, 0, 89, 0])].concat();
    for frames in [0, 1] {
        let bytes = form(Container::Riff, &list, Some(frames), false, 0);
        assert!(
            AdpcmForwardReader::new(Cursor::new(&bytes))
                .unwrap()
                .finish()
                .is_ok()
        );
        let mut audio = AdpcmForwardReader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(audio.read_i16(&mut [0; 1]).is_ok(), frames == 0);
    }
    let list = [b"wavl".as_slice(), &chunk(b"data", &[0, 0, 0, 0])].concat();
    let bytes = form(Container::Riff, &list, Some(2), false, 0);
    assert!(
        AdpcmForwardReader::new(Cursor::new(&bytes))
            .unwrap()
            .finish()
            .is_err()
    );
    assert!(
        AdpcmForwardReader::new(Cursor::new(&bytes))
            .unwrap()
            .read_i16(&mut [0; 2])
            .is_err()
    );
    for container in [Container::Riff, Container::Rf64, Container::Bw64] {
        for hint in [0, 1, u64::MAX] {
            let bytes = form(container, &list, Some(1), true, hint);
            let raw = ForwardReader::new(Cursor::new(&bytes)).unwrap();
            assert_eq!(
                AdpcmForwardReader::from_reader(raw).is_ok(),
                container == Container::Rf64
            );
            let raw = ForwardReader::new(Cursor::new(&bytes)).unwrap();
            let mut audio = AdpcmForwardReader::from_reader_with_frames(raw, 1).unwrap();
            assert_eq!(audio.read_i16(&mut [0; 3]).unwrap(), 1);
            audio.finish().unwrap();
            if container == Container::Rf64 {
                let audio = AdpcmForwardReader::new(Cursor::new(&bytes)).unwrap();
                assert_eq!(audio.finish().is_ok(), hint == 1);
            }
        }
        let bytes = form(container, &list, None, true, 1);
        let raw = ForwardReader::new(Cursor::new(&bytes)).unwrap();
        let audio = AdpcmForwardReader::from_reader_with_frames(raw, 1).unwrap();
        assert!(audio.finish().is_err());
    }
}

#[test]
fn virtual_silence_over_u32_finishes_without_expanding_its_duration() {
    for container in [Container::Rf64, Container::Bw64] {
        let mut list = b"wavl".to_vec();
        list.extend(chunk(b"data", &[0xd2, 4, 0, 0]));
        for _ in 0..2 {
            list.extend(chunk(b"slnt", &u32::MAX.to_le_bytes()));
        }
        let capacity = 1 + u64::from(u32::MAX) * 2;
        let frames = if container == Container::Rf64 {
            capacity
        } else {
            u64::from(u32::MAX)
        };
        let bytes = form(
            container,
            &list,
            Some(u32::MAX),
            false,
            if container == Container::Rf64 {
                frames
            } else {
                u64::MAX
            },
        );
        let size = bytes.len();
        let src = source(bytes, 1);
        let reads = src.read.clone();
        let mut audio = AdpcmForwardReader::new(src).unwrap();
        assert_eq!(audio.frames(), frames);
        let mut out = [0; 7];
        assert_eq!(audio.read_i16(&mut out).unwrap(), 7);
        assert_eq!(out, [1234; 7]);
        assert_eq!(audio.position(), 7);
        let src = audio.finish().unwrap();
        assert_eq!(src.inner.position(), size as u64);
        assert!(reads.get() < 512);
    }
}
