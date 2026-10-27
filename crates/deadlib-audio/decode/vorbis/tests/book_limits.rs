#[path = "support/book_extents.rs"]
mod extents;
mod support;
#[path = "support/degenerate_books.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_vorbis::{bitpack::BitReader, codebook::Codebook, *};
use extents::{Shape, Style};

#[test]
fn extent_limit_precedes_lengths_entry_limits_and_allocation() {
    let shape = Shape {
        dimensions: u16::MAX,
        entries: 256,
        style: Style::First,
        lookup: 0,
    };
    let (packed, _) = shape.packed();
    for length in [8, packed.len()] {
        let mut bits = BitReader::new(&packed[..length]);
        let limits = Limits {
            max_codebook_entries: 0,
            max_setup_memory: 0,
            ..Limits::default()
        };
        assert!(matches!(
            Codebook::read(&mut bits, &limits),
            Err(Error::LimitExceeded("codebook extent"))
        ));
        assert_eq!(bits.position(), 64);
        assert!(!bits.is_exhausted());
    }
    for style in [Style::First, Style::Last, Style::Empty, Style::Complete] {
        let shape = Shape { style, ..shape };
        let (packed, length) = shape.packed();
        let mut bits = BitReader::new(&packed);
        let book = Codebook::read_with_options(
            &mut bits,
            ParseOptions {
                limits: Limits {
                    max_codebook_extent_bits: 40,
                    ..Limits::default()
                },
                allow_empty_codebooks: true,
            },
        )
        .unwrap();
        assert_eq!(book.entries(), 256);
        assert_eq!(bits.position(), length);
        assert_eq!(bits.read(32), Ok(0x5a83c729));
        let mut bits = BitReader::new(&packed);
        assert!(matches!(
            Codebook::read_with_options(
                &mut bits,
                ParseOptions {
                    limits: Limits {
                        max_codebook_extent_bits: 40,
                        max_setup_memory: 0,
                        ..Limits::default()
                    },
                    allow_empty_codebooks: true,
                }
            ),
            Err(Error::LimitExceeded("setup memory"))
        ));
    }
}

fn limits() -> Limits {
    Limits {
        max_codebook_entries: 1 << 24,
        ..Limits::default()
    }
}
fn request(shapes: &[Shape]) -> Vec<u8> {
    let mut wire = (shapes.len() as u64).to_le_bytes().to_vec();
    for shape in shapes {
        let (packed, _) = shape.packed();
        wire.extend((packed.len() as u64).to_le_bytes());
        wire.extend(packed);
    }
    wire
}
fn compare(shapes: &[Shape], mut response: &[u8]) {
    assert_eq!(
        u64::from_le_bytes(support::take(&mut response, 8).try_into().unwrap()) as usize,
        shapes.len()
    );
    for shape in shapes {
        let accepted = support::take(&mut response, 1)[0] != 0;
        let consumed =
            u64::from_le_bytes(support::take(&mut response, 8).try_into().unwrap()) as usize;
        let (packed, length) = shape.packed();
        let mut bits = BitReader::new(&packed);
        let actual = Codebook::read_with_options(
            &mut bits,
            ParseOptions {
                limits: limits(),
                allow_empty_codebooks: true,
            },
        );
        assert_eq!(actual.is_ok(), accepted, "{shape:?}, C consumed {consumed}");
        assert_eq!(bits.position(), consumed, "{shape:?}");
        assert!(!bits.is_exhausted());
        if accepted {
            let actual = actual.unwrap();
            assert_eq!(actual.dimensions(), shape.dimensions as usize);
            assert_eq!(actual.entries(), shape.entries as usize);
            assert_eq!(bits.position(), length);
            assert_eq!(bits.read(32), Ok(0x5a83c729));
            assert_eq!(actual.is_empty(), shape.style == Style::Empty);
        } else {
            assert_eq!(consumed, 64);
        }
    }
    assert!(response.is_empty());
}
#[test]
fn codebook_extent_decisions_and_positions_match_retained_reference() {
    let mut golden = include_bytes!("fixtures/codebook-extents-v1.libvorbis-headers").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVBEXT01");
    compare(&extents::matrix(), golden);
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn codebook_extent_decisions_and_positions_match_libvorbis() {
    let shapes = extents::matrix();
    for (i, batch) in shapes.chunks(16).enumerate() {
        let wire = request(batch);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_BOOK_LIMIT_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("book-extents-{i}.bin")),
                &wire,
            )
            .unwrap();
        }
        compare(batch, &support::file_oracle("bookheaders", &wire));
    }
    eprintln!("codebook extent matrix: {} declarations", shapes.len());
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn whole_setup_extent_acceptance_matches_libvorbis_initialization() {
    use std::{fs, process::Command};
    let shapes = extents::matrix();
    let golden = include_bytes!("fixtures/codebook-extents-v1.libvorbis-headers");
    assert_eq!(
        u64::from_le_bytes(golden[8..16].try_into().unwrap()) as usize,
        shapes.len()
    );
    let executable =
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set DEADLIB_LIBVORBIS_ORACLE");
    let temp = std::env::temp_dir().join(format!(
        "deadlib-vorbis-extent-init-{}.bin",
        std::process::id()
    ));
    let mut report = String::from("case\tdimensions\tentries\tstyle\tlookup\taccepted\n");
    for (i, shape) in shapes.into_iter().enumerate() {
        let accepted = golden[16 + i * 9] != 0;
        let (packed, count) = shape.packed();
        let headers = vectors::Config {
            empty: false,
            ..vectors::Config::default()
        }
        .headers_with_unused(Some((&packed, count)));
        let id = Identification::parse(&headers[0]).unwrap();
        let native = Setup::parse_with_options(
            &headers[2],
            &id,
            ParseOptions {
                limits: limits(),
                allow_empty_codebooks: true,
            },
        );
        assert_eq!(native.is_ok(), accepted, "setup {shape:?}");
        if let Ok(setup) = native {
            PacketDecoder::with_limits(setup, limits()).unwrap();
        }
        let mut input = Vec::new();
        for header in &headers {
            input.extend((header.len() as u64).to_le_bytes());
            input.extend(header);
        }
        fs::write(&temp, &input).unwrap();
        let output = Command::new(&executable)
            .arg("pcm")
            .arg(&temp)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "C init {shape:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if accepted {
            assert_eq!(output.stdout, 2u32.to_le_bytes());
        } else {
            assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "PCM header");
        }
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_BOOK_LIMIT_INPUTS") {
            fs::create_dir_all(&root).unwrap();
            fs::write(
                std::path::Path::new(&root).join(format!("extent-init-{i}.bin")),
                input,
            )
            .unwrap();
        }
        use std::fmt::Write;
        writeln!(
            report,
            "{i}\t{}\t{}\t{:?}\t{}\t{accepted}",
            shape.dimensions, shape.entries, shape.style, shape.lookup
        )
        .unwrap();
    }
    fs::remove_file(temp).unwrap();
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_BOOK_LIMIT_INPUTS") {
        fs::write(std::path::Path::new(&root).join("extent-init.tsv"), report).unwrap();
    }
    eprintln!("whole setup extent matrix: 475 matching parse/initialization decisions");
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_codebook_extent_golden() {
    let shapes = extents::matrix();
    let mut golden = b"DVBEXT01".to_vec();
    golden.extend((shapes.len() as u64).to_le_bytes());
    for batch in shapes.chunks(16) {
        let mut response = support::file_oracle("bookheaders", &request(batch));
        assert_eq!(
            u64::from_le_bytes(response[..8].try_into().unwrap()) as usize,
            batch.len()
        );
        golden.extend(response.drain(8..));
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codebook-extents-v1.libvorbis-headers"),
        golden,
    )
    .unwrap();
}
