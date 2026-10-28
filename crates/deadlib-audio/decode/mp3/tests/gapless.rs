mod support;
use deadlib_audio_decode_mp3::*;
use std::{
    cell::Cell,
    io::{self, Cursor, Read},
    rc::Rc,
};
use support::{frame, info};

struct Tiny<'a> {
    data: &'a [u8],
    size: usize,
    calls: usize,
}
impl Read for Tiny<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(7) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = bytes.len().min(self.size).min(self.data.len());
        bytes[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}
fn collect<R: Read>(decoder: &mut GaplessDecoder<R>) -> Result<Vec<(u32, usize, u32)>, Error> {
    let mut values = Vec::new();
    let mut position = decoder.position();
    while let Some(out) = decoder.next_frame()? {
        assert_eq!(out.position, position);
        assert!(out.pcm.samples() > 0);
        position += out.pcm.samples() as u64;
        for n in 0..out.pcm.samples() {
            for channel in 0..out.pcm.channels() {
                values.push((
                    out.header.sample_rate(),
                    channel,
                    out.pcm.plane(channel).unwrap()[n].to_bits(),
                ));
            }
        }
    }
    assert_eq!(decoder.position(), position);
    assert!(decoder.next_frame()?.is_none());
    Ok(values)
}
fn indexed(data: &[u8], options: StreamOptions) -> Vec<(u32, usize, u32)> {
    let mut decoder = IndexedDecoder::with_options(
        Cursor::new(data),
        FileOptions {
            stream: options,
            samples: SampleMode::Gapless,
        },
    )
    .unwrap();
    let mut values = Vec::new();
    while let Some(out) = decoder.next_frame().unwrap() {
        for n in 0..out.pcm.samples() {
            for channel in 0..out.pcm.channels() {
                values.push((
                    out.header.sample_rate(),
                    channel,
                    out.pcm.plane(channel).unwrap()[n].to_bits(),
                ));
            }
        }
    }
    assert_eq!(decoder.position(), decoder.samples());
    values
}

#[test]
fn streaming_gapless_matches_indexed_for_all_versions_short_and_long_padding() {
    for version in [3, 2, 0] {
        for channels in [1, 2] {
            for (delay, padding) in [(576, 700), (0, 0), (0, 528), (0, 529), (4095, 4095)] {
                let mut data = info(version, 0, channels, 20, delay, padding);
                for n in 0..20 {
                    data.extend(frame(version, 0, channels, 190 + n, n % 2 == 0, 0));
                }
                let expected = indexed(&data, StreamOptions::default());
                for size in [1, 3, 31, 8192] {
                    let mut decoder = GaplessDecoder::new(Tiny {
                        data: &data,
                        size,
                        calls: 0,
                    });
                    assert_eq!(
                        collect(&mut decoder).unwrap(),
                        expected,
                        "{version} {channels} {delay} {padding} {size}"
                    );
                    assert_eq!(decoder.stats().decoded_frames, 21);
                    assert!(decoder.metadata().vbr().unwrap().is_some());
                }
            }
        }
    }
}

#[test]
fn untagged_unknown_tags_and_format_changes_have_the_same_coordinates() {
    let untagged = [
        frame(3, 0, 1, 206, false, 0).repeat(3),
        frame(2, 0, 2, 198, true, 0).repeat(3),
    ]
    .concat();
    let mut tag = info(3, 0, 1, 3, 576, 700);
    let h = FrameHeader::parse(&tag).unwrap();
    let name = 4 + h.side_info_bytes() + 12;
    tag[name..name + 4].copy_from_slice(b"????");
    let unknown = [tag, frame(3, 0, 1, 206, false, 0).repeat(3)].concat();
    for bytes in [&untagged, &unknown] {
        let expected = indexed(bytes, StreamOptions::default());
        let mut decoder = GaplessDecoder::new(Tiny {
            data: bytes,
            size: 1,
            calls: 0,
        });
        assert_eq!(collect(&mut decoder).unwrap(), expected);
    }
    let mixed = [
        info(3, 0, 1, 2, 576, 700),
        frame(3, 0, 1, 206, false, 0),
        frame(2, 0, 2, 206, false, 0),
    ]
    .concat();
    let mut decoder = GaplessDecoder::new(Cursor::new(&mixed));
    assert!(matches!(collect(&mut decoder), Err(Error::Unsupported(_))));
    assert!(decoder.next_frame().is_err());
    decoder.reset(Cursor::new(&untagged));
    assert_eq!(
        collect(&mut decoder).unwrap(),
        indexed(&untagged, StreamOptions::default())
    );
}

#[test]
fn empty_streams_impossible_trim_and_declared_frame_mismatches_are_explicit() {
    let empty = info(3, 0, 1, 0, 0, 0);
    let mut decoder = GaplessDecoder::new(Cursor::new(&empty));
    assert!(collect(&mut decoder).unwrap().is_empty());
    let no_bytes = Vec::new();
    decoder.reset(Cursor::new(&no_bytes));
    assert!(collect(&mut decoder).unwrap().is_empty());
    for (frames, delay, padding) in [(4, 576, 700), (2, 4095, 4095)] {
        let bytes = [
            info(3, 0, 1, frames, delay, padding),
            frame(3, 0, 1, 206, false, 0).repeat(2),
        ]
        .concat();
        let mut decoder = GaplessDecoder::new(Cursor::new(&bytes));
        assert!(matches!(collect(&mut decoder), Err(Error::Invalid(_))));
        assert!(decoder.next_frame().is_err());
    }
    struct Failing {
        source: Cursor<Vec<u8>>,
        fail_after: usize,
    }
    impl Read for Failing {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let position = self.source.position() as usize;
            if position >= self.fail_after {
                return Err(io::ErrorKind::ConnectionReset.into());
            }
            let size = output.len().min(self.fail_after - position);
            self.source.read(&mut output[..size])
        }
    }
    let audio = frame(3, 0, 1, 206, false, 0);
    let bytes = [info(3, 0, 1, 4, 576, 700), audio.repeat(4)].concat();
    let mut decoder = GaplessDecoder::new(Failing {
        source: Cursor::new(bytes.clone()),
        fail_after: audio.len() * 2,
    });
    assert!(matches!(collect(&mut decoder), Err(Error::Io(_))));
    assert!(decoder.next_frame().is_err());
    decoder.reset(Failing {
        source: Cursor::new(bytes.clone()),
        fail_after: usize::MAX,
    });
    assert_eq!(
        collect(&mut decoder).unwrap(),
        indexed(&bytes, StreamOptions::default())
    );
}

#[test]
fn raw_coordinates_metadata_and_recovery_survive_trimming() {
    let mut bad = support::protect(frame(3, 0, 2, 206, false, 0));
    bad[4] ^= 1; // Bad protection CRC in a complete physical frame.
    let bytes = [
        info(3, 0, 2, 4, 576, 700),
        frame(3, 0, 2, 206, false, 0),
        bad,
        frame(3, 0, 2, 198, true, 0).repeat(2),
    ]
    .concat();
    let options = StreamOptions {
        recovery: Recovery::Conceal,
        ..Default::default()
    };
    let mut decoder = GaplessDecoder::with_options(
        Tiny {
            data: &bytes,
            size: 1,
            calls: 0,
        },
        options,
    );
    let first = decoder.next_frame().unwrap().unwrap();
    assert_eq!(first.position, 0);
    assert_eq!(first.raw_position, 1152 + 576 + DECODER_DELAY);
    assert_eq!(first.byte_offset, 417);
    let mut decoder = GaplessDecoder::with_options(
        Tiny {
            data: &bytes,
            size: 1,
            calls: 0,
        },
        options,
    );
    assert_eq!(collect(&mut decoder).unwrap(), indexed(&bytes, options));
    assert!(decoder.stats().concealed_frames > 0);
    assert!(decoder.last_recovered_error().is_some());
}

#[test]
fn first_output_is_bounded_and_does_not_require_eof_or_seek() {
    struct Counting {
        source: Cursor<Vec<u8>>,
        read: Rc<Cell<usize>>,
    }
    impl Read for Counting {
        fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
            let size = data.len().min(7);
            let n = self.source.read(&mut data[..size])?;
            self.read.set(self.read.get() + n);
            Ok(n)
        }
    }
    let bytes = [
        info(3, 0, 2, 10000, 4095, 4095),
        frame(3, 0, 2, 206, false, 0).repeat(10000),
    ]
    .concat();
    let read = Rc::new(Cell::new(0));
    let mut decoder = GaplessDecoder::new(Counting {
        source: Cursor::new(bytes),
        read: read.clone(),
    });
    assert!(decoder.next_frame().unwrap().is_some());
    assert!(read.get() < 8192, "read entire source before first PCM");
    assert!((1..=1152).contains(&decoder.position()));
}
