#[path = "support/book_extents.rs"]
#[allow(dead_code)]
mod extents;
#[path = "support/raw_cases.rs"]
mod raw_cases;
mod support;
#[path = "support/degenerate_books.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_vorbis::*;
use extents::{Shape, Style};
use std::io::Cursor;

fn shapes() -> [Shape; 4] {
    [
        Shape {
            dimensions: 64,
            entries: 64,
            style: Style::First,
            lookup: 0,
        },
        Shape {
            dimensions: u16::MAX,
            entries: 128,
            style: Style::Last,
            lookup: 0,
        },
        Shape {
            dimensions: 8,
            entries: (1 << 19) + 1,
            style: Style::First,
            lookup: 0,
        },
        Shape {
            dimensions: u16::MAX,
            entries: 256,
            style: Style::First,
            lookup: 0,
        },
    ]
}
fn headers(shape: Shape) -> Vec<Vec<u8>> {
    let (packed, count) = shape.packed();
    vectors::Config {
        empty: false,
        ..vectors::Config::default()
    }
    .headers_with_unused(Some((&packed, count)))
}
fn packets() -> Vec<Vec<u8>> {
    let c = vectors::Config {
        empty: false,
        ..vectors::Config::default()
    };
    vec![
        c.audio(false, 0, 1),
        c.audio(true, 3, 2),
        c.audio(true, 0, 5),
        c.audio(false, 0, 3),
        c.audio(true, 0, 4),
    ]
}
fn decoder(headers: &[Vec<u8>], limits: Limits) -> PacketDecoder {
    let id = Identification::parse(&headers[0]).unwrap();
    PacketDecoder::with_limits(Setup::parse(&headers[2], &id, &limits).unwrap(), limits).unwrap()
}
#[test]
fn extent_limit_survives_ogg_reset_and_indexed_setup_reparse() {
    use deadlib_audio_decode_ogg::PacketWriter;
    let mut seeks = 0;
    for (i, shape) in shapes().into_iter().enumerate() {
        let headers = headers(shape);
        let packets = packets();
        let bits = match i {
            0 => 14,
            3 => 25,
            _ => 24,
        };
        let limits = Limits {
            max_codebook_extent_bits: bits,
            ..Limits::default()
        };
        let low = Limits {
            max_codebook_extent_bits: bits - 1,
            ..limits
        };
        let id = Identification::parse(&headers[0]).unwrap();
        assert!(matches!(
            Setup::parse(&headers[2], &id, &low),
            Err(Error::LimitExceeded("codebook extent"))
        ));
        let mut raw = decoder(&headers, limits);
        let mut expected = vec![Vec::new(); 2];
        let mut writer = PacketWriter::new(Vec::new(), 83);
        for h in &headers {
            writer.write_packet(h, 0, false).unwrap();
        }
        writer.flush_page().unwrap();
        let mut position = 0;
        for (i, (p, frames)) in packets.iter().zip([0, 80, 128, 80, 80]).enumerate() {
            let block = raw.decode(p).unwrap().unwrap();
            assert_eq!(block.start_frame, position);
            assert_eq!(block.frames, frames as usize);
            for (out, plane) in expected.iter_mut().zip(block.planes()) {
                out.extend(plane.iter().map(|p| p.to_bits()));
            }
            position += frames;
            writer
                .write_packet(p, position, i + 1 == packets.len())
                .unwrap();
            writer.flush_page().unwrap();
        }
        let bytes = writer.into_inner();
        assert_eq!(position, 368);
        if i == 2 {
            assert!(headers[2].len() > 65025);
        }
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_BOOK_LIMIT_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("extent-stream-{i}.ogg")),
                &bytes,
            )
            .unwrap();
        }
        let mut rejected = OggReader::with_options(
            Cursor::new(&bytes),
            OggOptions {
                limits: low,
                ..OggOptions::default()
            },
        );
        loop {
            match rejected.read_next(|_| {}) {
                Ok(true) => {}
                Err(Error::LimitExceeded("codebook extent")) => break,
                _ => panic!("expected extent rejection"),
            }
        }
        let options = OggOptions {
            limits,
            ..OggOptions::default()
        };
        let collect = |out: &mut Vec<Vec<u32>>, event: OggEvent<'_>| {
            if let OggEvent::Pcm(block) = event {
                assert_eq!(block.start_frame as usize, out[0].len());
                assert_eq!(block.start_granule, block.start_frame);
                for (out, plane) in out.iter_mut().zip(block.pcm.planes()) {
                    out.extend(plane.iter().map(|p| p.to_bits()));
                }
            }
        };
        let mut pull = OggReader::with_options(Cursor::new(&bytes), options);
        let mut out = vec![Vec::new(); 2];
        while pull.read_next(|e| collect(&mut out, e)).unwrap() {}
        assert_eq!(out, expected);
        let mut stream = OggStreamDecoder::new(options);
        for size in [1, 17, bytes.len()] {
            stream.reset();
            let mut out = vec![Vec::new(); 2];
            for chunk in bytes.chunks(size) {
                let mut input = chunk;
                while !input.is_empty() {
                    let used = stream.push(input, |e| collect(&mut out, e)).unwrap();
                    assert!(used > 0);
                    input = &input[used..];
                }
            }
            stream.finish().unwrap();
            assert_eq!(out, expected);
        }
        assert!(matches!(
            VorbisIndex::read(
                &mut Cursor::new(&bytes),
                SeekOptions {
                    limits: low,
                    ..SeekOptions::default()
                }
            ),
            Err(Error::LimitExceeded("codebook extent"))
        ));
        for packet_preroll in [false, true] {
            let index = VorbisIndex::read(
                &mut Cursor::new(&bytes),
                SeekOptions {
                    limits,
                    packet_preroll,
                    ..SeekOptions::default()
                },
            )
            .unwrap();
            let mut reader = OggSeekReader::with_index(Cursor::new(&bytes), index).unwrap();
            let id = reader.selected().info.id;
            for target in [0, 1, 79, 80, 81, 207, 208, 287, 288, 367, 368, 0, 81] {
                reader.seek(id, target).unwrap();
                assert_eq!(reader.position(), target);
                let mut out = vec![Vec::new(); 2];
                while reader
                    .read_next(|e| {
                        if let OggEvent::Pcm(block) = e {
                            assert_eq!(block.start_frame, target + out[0].len() as u64);
                            for (out, plane) in out.iter_mut().zip(block.pcm.planes()) {
                                out.extend(plane.iter().map(|p| p.to_bits()));
                            }
                        }
                    })
                    .unwrap()
                {}
                for (got, expected) in out.iter().zip(&expected) {
                    assert_eq!(got, &expected[target as usize..]);
                }
                seeks += 1;
            }
        }
    }
    assert_eq!(seeks, 104);
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn unused_extent_boundary_books_preserve_reference_pcm() {
    for (i, shape) in shapes().into_iter().take(3).enumerate() {
        let headers = headers(shape);
        let cases = [raw_cases::Case {
            name: format!("{shape:?}"),
            packets: packets(),
        }];
        let reference = raw_cases::reference(&format!("extent-stream-{i}"), &headers, &cases);
        raw_cases::report(
            &format!("extent-stream-{i}"),
            raw_cases::compare(&headers, &cases, &reference),
        );
    }
}
