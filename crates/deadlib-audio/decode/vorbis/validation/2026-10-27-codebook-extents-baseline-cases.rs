#[path = "support/book_extents.rs"]
mod extents;
mod support;
use deadlib_audio_decode_vorbis::{bitpack::BitReader, codebook::Codebook, *};
use extents::{Shape, Style};

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
