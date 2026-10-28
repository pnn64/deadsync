use deadlib_audio_decode_wav::{Container, Encoding, Muxer, RawChunk, Wave, WriteOptions};
use std::io::Cursor;

fn opaque() -> Vec<u8> {
    let mut f = Vec::new();
    for v in [0x1234u16, 2] {
        f.extend(v.to_le_bytes());
    }
    for v in [48000u32, 12345] {
        f.extend(v.to_le_bytes());
    }
    for v in [1u16, 0, 3] {
        f.extend(v.to_le_bytes());
    }
    f.extend([1, 2, 3]);
    f
}

#[test]
fn opaque_codec_muxing_preserves_format_unknown_chunks_and_padding() {
    for container in [Container::Riff, Container::Rf64, Container::Bw64] {
        let f = opaque();
        let options = WriteOptions {
            container,
            ..WriteOptions::default()
        };
        let mut m = Muxer::new(
            Cursor::new(Vec::new()),
            &f,
            options,
            &[RawChunk {
                id: *b"abcd",
                data: b"odd",
            }],
        )
        .unwrap();
        m.write_data(&[9, 8, 7], 4321).unwrap();
        m.write_data(&[6, 5], 12).unwrap();
        assert_eq!(m.bytes_written(), 5);
        assert_eq!(m.frames_written(), 4333);
        let bytes = m.finish().unwrap().into_inner();
        let w = Wave::parse(&bytes).unwrap();
        assert_eq!(w.format.encoding, Encoding::Other(0x1234));
        assert_eq!(w.format.raw, f);
        assert_eq!(w.fact, Some(4333));
        assert_eq!(w.data().unwrap(), [9, 8, 7, 6, 5]);
        assert_eq!(
            w.chunks()
                .unwrap()
                .find_map(|c| {
                    let c = c.unwrap();
                    (c.id == *b"abcd").then_some(c.data)
                })
                .unwrap(),
            b"odd"
        );
    }
}

#[test]
fn decoded_count_alone_can_promote_rf64_and_overflow_is_rejected() {
    let f = opaque();
    let mut m = Muxer::new(Cursor::new(Vec::new()), &f, WriteOptions::default(), &[]).unwrap();
    m.write_data(&[42], u64::from(u32::MAX) + 123).unwrap();
    let bytes = m.finish().unwrap().into_inner();
    let w = Wave::parse(&bytes).unwrap();
    assert_eq!(w.container, Container::Rf64);
    assert_eq!(w.fact, Some(u64::from(u32::MAX) + 123));
    assert_eq!(w.ds64.unwrap().data_size, 1);
    let mut m = Muxer::new(
        Cursor::new(Vec::new()),
        &f,
        WriteOptions {
            container: Container::Rf64,
            ..WriteOptions::default()
        },
        &[],
    )
    .unwrap();
    m.write_data(&[], u64::MAX - 1).unwrap();
    assert!(m.write_data(&[], 2).is_err());
    assert_eq!(m.frames_written(), u64::MAX - 1);
    let bytes = m.finish().unwrap().into_inner();
    let w = Wave::parse(&bytes).unwrap();
    assert_eq!(w.fact, Some(u64::MAX - 1));
    let mut m = Muxer::new(
        Cursor::new(Vec::new()),
        &f,
        WriteOptions {
            auto_rf64: false,
            ..WriteOptions::default()
        },
        &[],
    )
    .unwrap();
    assert!(m.write_data(&[1], u64::from(u32::MAX)).is_err());
    assert_eq!(m.bytes_written(), 0);
}

#[test]
fn native_muxer_checks_producer_frame_counts_before_writing() {
    let f = [1, 0, 1, 0, 0x40, 0x1f, 0, 0, 0x80, 0x3e, 0, 0, 2, 0, 16, 0];
    let mut m = Muxer::new(Cursor::new(Vec::new()), &f, WriteOptions::default(), &[]).unwrap();
    assert!(m.write_data(&[0, 0], 0).is_err());
    assert!(m.write_data(&[0], 1).is_err());
    m.write_data(&[123, 0], 1).unwrap();
    let bytes = m.finish().unwrap().into_inner();
    let w = Wave::parse(&bytes).unwrap();
    assert_eq!(w.data().unwrap(), [123, 0]);
    assert_eq!(w.fact, None);
}
