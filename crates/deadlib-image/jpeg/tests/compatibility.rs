use deadlib_image::{ColorType, Image, Samples};
use deadlib_image_jpeg::{
    DecodeOptions, Error, Event, HuffmanClass, Limits, MarkerReader, QuantizationTable, Tables,
    Upsampling, decode, decode_coefficients, decode_coefficients_with_options, decode_hierarchical,
    decode_hierarchical_with_options, decode_lossless, decode_lossless_with_options,
    decode_with_options,
};

fn compatible() -> DecodeOptions {
    DecodeOptions {
        strict: false,
        ..DecodeOptions::default()
    }
}

fn segment(out: &mut Vec<u8>, marker: u8, data: &[u8]) {
    out.extend([0xff, marker]);
    out.extend(u16::try_from(data.len() + 2).unwrap().to_be_bytes());
    out.extend(data);
}

fn frame(width: u16, height: u16) -> Vec<u8> {
    let mut data = vec![8];
    data.extend(height.to_be_bytes());
    data.extend(width.to_be_bytes());
    data.extend([1, 1, 0x11, 0]);
    data
}

fn huffman(ac_symbol: u8) -> Vec<u8> {
    let mut data = Vec::new();
    for (selector, symbol) in [(0, 0), (0x10, ac_symbol)] {
        data.extend([selector, 1]);
        data.extend([0; 15]);
        data.push(symbol);
    }
    data
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Sequential,
    Progressive,
    Lossless,
}

impl Kind {
    fn dimensions(self) -> (u16, u16) {
        match self {
            Self::Lossless => (1, 2),
            _ => (16, 8),
        }
    }

    fn marker(self) -> u8 {
        match self {
            Self::Sequential => 0xc0,
            Self::Progressive => 0xc2,
            Self::Lossless => 0xc3,
        }
    }

    fn padded_byte(self) -> u8 {
        match self {
            Self::Sequential => 0x3f, // DC=0 and EOB=0, followed by six one bits.
            _ => 0x7f,                // One zero DC difference or EOB, then seven one bits.
        }
    }
}

const KINDS: [Kind; 3] = [Kind::Sequential, Kind::Progressive, Kind::Lossless];

// Independent wire bytes: each Huffman table has only code 0 -> symbol 0.
// These produce constant 128 without calling the native encoder.
fn jpeg(kind: Kind, entropy: &[u8], interval: u16) -> Vec<u8> {
    let (width, height) = kind.dimensions();
    let mut out = vec![0xff, 0xd8];
    let mut quant = vec![1; 65];
    quant[0] = 0;
    segment(&mut out, 0xdb, &quant);
    segment(&mut out, 0xc4, &huffman(0));
    segment(&mut out, kind.marker(), &frame(width, height));
    if interval != 0 {
        segment(&mut out, 0xdd, &interval.to_be_bytes());
    }
    let scans: &[[u8; 3]] = match kind {
        Kind::Sequential => &[[0, 63, 0]],
        Kind::Progressive => &[[0, 0, 0], [1, 63, 0]],
        Kind::Lossless => &[[1, 0, 0]],
    };
    for &[start, end, approximation] in scans {
        segment(&mut out, 0xda, &[1, 1, 0, start, end, approximation]);
        out.extend(entropy);
    }
    out.extend([0xff, 0xd9]);
    out
}

fn expected(kind: Kind) -> Image {
    let (width, height) = kind.dimensions();
    Image {
        width: u32::from(width),
        height: u32::from(height),
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8(vec![128; usize::from(width) * usize::from(height)]),
    }
}

fn model_pixels(kind: Kind, bytes: &[u8], options: DecodeOptions) -> Result<Image, Error> {
    match kind {
        Kind::Lossless => decode_lossless_with_options(bytes, options)?
            .into_planar()?
            .to_image(Upsampling::Linear),
        _ => decode_coefficients_with_options(bytes, options)?
            .reconstruct()?
            .to_image(Upsampling::Linear),
    }
}

fn assert_pixels(kind: Kind, bytes: &[u8], options: DecodeOptions) {
    let expected = expected(kind);
    for method in [Upsampling::Nearest, Upsampling::Linear] {
        assert_eq!(
            decode_with_options(bytes, options, method).unwrap(),
            expected,
            "{kind:?}: {method:?}"
        );
    }
    assert_eq!(model_pixels(kind, bytes, options).unwrap(), expected);
}

fn assert_strict_error(kind: Kind, bytes: &[u8]) {
    assert!(decode(bytes, Limits::default()).is_err(), "{kind:?}");
    assert!(model_pixels(kind, bytes, DecodeOptions::default()).is_err());
    match kind {
        Kind::Lossless => assert!(decode_lossless(bytes, Limits::default()).is_err()),
        _ => assert!(decode_coefficients(bytes, Limits::default()).is_err()),
    }
}

fn assert_rejected(kind: Kind, bytes: &[u8]) {
    assert!(
        decode_with_options(bytes, compatible(), Upsampling::Linear).is_err(),
        "{kind:?}: {bytes:02x?}"
    );
    assert!(model_pixels(kind, bytes, compatible()).is_err(), "{kind:?}");
}

fn first_scan(bytes: &[u8]) -> usize {
    MarkerReader::new(bytes)
        .unwrap()
        .find_map(|event| match event.unwrap() {
            Event::Scan { offset, .. } => Some(offset),
            _ => None,
        })
        .unwrap()
}

fn insert(bytes: &[u8], offset: usize, extra: &[u8]) -> Vec<u8> {
    let mut out = bytes[..offset].to_vec();
    out.extend(extra);
    out.extend(&bytes[offset..]);
    out
}

#[test]
fn empty_table_segments_are_no_ops_before_and_after_scans() {
    let empty = [0xff, 0xc4, 0, 2, 0xff, 0xdb, 0, 2];
    for kind in KINDS {
        let byte = kind.padded_byte();
        let original = jpeg(kind, &[byte, 0xff, 0xd0, byte], 1);
        for offset in [2, first_scan(&original), original.len() - 2] {
            let bytes = insert(&original, offset, &empty);
            assert_pixels(kind, &bytes, DecodeOptions::default());
            assert_pixels(kind, &bytes, compatible());
        }
    }
    // A late table segment sends sequential raster decoding through its
    // coefficient fallback; it must preserve the tables active at the scan.
    let original = jpeg(Kind::Sequential, &[0x3f, 0xff, 0xd0, 0x3f], 1);
    let with_empty = insert(&original, original.len() - 2, &empty);
    let a = decode_coefficients(&original, Limits::default()).unwrap();
    let b = decode_coefficients(&with_empty, Limits::default()).unwrap();
    assert_eq!(a.components[0].blocks, b.components[0].blocks);
    assert_eq!(a.components[0].quantization, b.components[0].quantization);
}

#[test]
fn empty_segments_preserve_tables_but_empty_definitions_replace_them() {
    let mut empty_stream = vec![0xff, 0xd8];
    segment(&mut empty_stream, 0xc4, &[]);
    segment(&mut empty_stream, 0xdb, &[]);
    empty_stream.extend([0xff, 0xd9]);
    let mut tables = Tables::new();
    tables.load(&empty_stream, Limits::default()).unwrap();
    assert!(tables.is_empty());

    let quant = QuantizationTable {
        values: [7; 64],
        wide: false,
    };
    let mut counts = [0; 16];
    counts[0] = 1;
    for id in 0..4 {
        tables.set_quantization(id, quant).unwrap();
        for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
            tables.set_huffman(class, id, counts, &[0]).unwrap();
        }
    }
    tables.load(&empty_stream, Limits::default()).unwrap();
    for id in 0..4 {
        assert_eq!(tables.quantization(id), Some(&quant));
        for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
            let table = tables.huffman(class, id).unwrap();
            assert_eq!(table.counts, &counts);
            assert_eq!(table.symbols, &[0]);
        }
    }

    let mut redefine = vec![0xff, 0xd8];
    segment(&mut redefine, 0xc4, &[0; 17]); // DC destination 0, sixteen zero counts.
    redefine.extend([0xff, 0xd9]);
    tables.load(&redefine, Limits::default()).unwrap();
    let table = tables.huffman(HuffmanClass::Dc, 0).unwrap();
    assert_eq!(table.counts, &[0; 16]);
    assert!(table.symbols.is_empty());
    assert_eq!(tables.huffman(HuffmanClass::Ac, 0).unwrap().symbols, &[0]);

    let valid = jpeg(Kind::Sequential, &[0x3f, 0xff, 0xd0, 0x3f], 1);
    let redefined = insert(&valid, first_scan(&valid), &redefine[2..redefine.len() - 2]);
    assert_rejected(Kind::Sequential, &redefined);
    for (marker, body) in [(0xc4, vec![0]), (0xdb, vec![0]), (0xdb, vec![0; 65])] {
        let mut bad_table = Vec::new();
        segment(&mut bad_table, marker, &body);
        let bytes = insert(&valid, first_scan(&valid), &bad_table);
        assert_rejected(Kind::Sequential, &bytes);
    }

    // An empty segment must not install a missing destination.
    for marker in [0xc4, 0xdb] {
        let (offset, size) = MarkerReader::new(&valid)
            .unwrap()
            .find_map(|event| match event.unwrap() {
                Event::Segment {
                    marker: found,
                    data,
                    offset,
                } if found == marker => Some((offset, data.len() + 4)),
                _ => None,
            })
            .unwrap();
        let mut missing = valid[..offset].to_vec();
        segment(&mut missing, marker, &[]);
        missing.extend(&valid[offset + size..]);
        assert_rejected(Kind::Sequential, &missing);
    }
}

#[test]
fn complete_intervals_and_scans_can_discard_huffman_padding() {
    assert!(DecodeOptions::default().strict);
    for kind in KINDS {
        let byte = kind.padded_byte();
        let original = jpeg(kind, &[byte, 0xff, 0xd0, byte], 1);
        assert_pixels(kind, &original, DecodeOptions::default());
        // Exercise restart alignment and final scan alignment separately,
        // including surplus bytes beyond the fast reader's refill window.
        for (first, last) in [
            (vec![0], vec![byte]),
            (vec![byte], vec![0]),
            (vec![byte, 0x13, 0x42], vec![byte]),
            (vec![byte], vec![byte, 0x55, 0xff, 0]),
            (vec![0; 19], vec![0, 0xff, 0, 0x42]),
        ] {
            let mut entropy = first;
            entropy.extend([0xff, 0xd0]);
            entropy.extend(last);
            let bytes = jpeg(kind, &entropy, 1);
            assert_strict_error(kind, &bytes);
            assert_pixels(kind, &bytes, compatible());
        }
    }
}

fn hierarchy(kind: Kind, bytes: &[u8]) -> Vec<u8> {
    let (width, height) = kind.dimensions();
    let mut header = Vec::new();
    segment(&mut header, 0xde, &frame(width, height));
    insert(bytes, 2, &header)
}

#[test]
fn hierarchical_frames_receive_the_same_huffman_policy() {
    for kind in KINDS {
        let byte = kind.padded_byte();
        let valid = hierarchy(kind, &jpeg(kind, &[byte, 0xff, 0xd0, byte], 1));
        let changed = hierarchy(kind, &jpeg(kind, &[0, 0x55, 0xff, 0, 0xff, 0xd0, 0], 1));
        let expected = decode_hierarchical(&valid, Limits::default())
            .unwrap()
            .reconstruct()
            .unwrap()
            .to_image(Upsampling::Linear)
            .unwrap();
        assert_eq!(expected, self::expected(kind));
        assert!(decode_hierarchical(&changed, Limits::default()).is_err());
        assert_eq!(
            decode_hierarchical_with_options(&changed, compatible())
                .unwrap()
                .reconstruct()
                .unwrap()
                .to_image(Upsampling::Linear)
                .unwrap(),
            expected
        );
        assert_eq!(
            decode_with_options(&changed, compatible(), Upsampling::Linear).unwrap(),
            expected
        );
        assert!(
            decode_hierarchical_with_options(&changed[..changed.len() - 2], compatible()).is_err()
        );
    }
}

#[test]
fn recovery_never_skips_wrong_restarts_or_supplies_missing_samples() {
    for kind in KINDS {
        let byte = kind.padded_byte();
        for entropy in [
            vec![0xff, 0xd0, byte],       // Early restart; no first MCU's entropy.
            vec![byte, 0xff, 0xd0],       // Missing second MCU; catches virtual-bit reads.
            vec![byte, 0xff, 0xd1, byte], // Wrong first restart.
            vec![byte, 0xff, 0xd1, 0xff, 0xd0, byte], // Cannot search past wrong RST.
            vec![byte, 0xff, 0xd0, byte, 0xff, 0xd1], // Extra final restart.
            vec![byte, byte],             // Required restart absent.
            vec![0xff, 0, 0xff, 0xd0, byte], // Unassigned Huffman prefix.
            vec![byte, 0xff, 0xdd, 0, 4, 0, 1], // Other marker before required restart.
        ] {
            assert_rejected(kind, &jpeg(kind, &entropy, 1));
        }
        let recovered = jpeg(kind, &[0, 0xff, 0xd0, 0], 1);
        assert_pixels(kind, &recovered, compatible());
        // EOI itself may be absent in compatible mode; partial markers and
        // every earlier truncation still lack a complete image.
        for end in 0..recovered.len() {
            if end != recovered.len() - 2 {
                assert_rejected(kind, &recovered[..end]);
            }
        }
    }
}

#[test]
fn progressive_eob_runs_still_end_at_scan_and_restart_boundaries() {
    for restart in [false, true] {
        let mut bytes = jpeg(Kind::Progressive, &[0x3f], 0);
        let ac_scan = MarkerReader::new(&bytes)
            .unwrap()
            .filter_map(|event| match event.unwrap() {
                Event::Scan { offset, .. } => Some(offset),
                _ => None,
            })
            .nth(1)
            .unwrap();
        bytes.truncate(ac_scan);
        segment(&mut bytes, 0xc4, &huffman(0x20));
        if restart {
            segment(&mut bytes, 0xdd, &[0, 1]);
        }
        segment(&mut bytes, 0xda, &[1, 1, 0, 1, 63, 0]);
        bytes.push(0x1f); // EOB run 4, but only two blocks (or one before RST).
        if restart {
            bytes.extend([0xff, 0xd0]);
        }
        bytes.extend([0xff, 0xd9]);
        let expected = Error::Invalid(if restart {
            "EOB run crosses restart"
        } else {
            "EOB run exceeds scan"
        });
        assert_eq!(
            decode_coefficients_with_options(&bytes, compatible()).unwrap_err(),
            expected
        );
        assert_rejected(Kind::Progressive, &bytes);
    }
}

#[test]
fn complete_huffman_images_can_omit_eoi_but_partial_scans_cannot() {
    for kind in KINDS {
        let byte = kind.padded_byte();
        let original = jpeg(kind, &[byte, 0xff, 0xd0, byte], 1);
        let without_eoi = &original[..original.len() - 2];
        assert_strict_error(kind, without_eoi);
        assert_pixels(kind, without_eoi, compatible());
        assert_rejected(kind, &original[..original.len() - 3]);
        assert_rejected(kind, &original[..original.len() - 1]);

        let mut noise = vec![0x12, 0x34, 0xff, 0, 0x56];
        noise.extend([0xff, 0xc4, 0, 2, 0xff, 0xdb, 0, 2]);
        let noisy = insert(&original, first_scan(&original), &noise);
        assert_strict_error(kind, &noisy);
        assert_pixels(kind, &noisy, compatible());
        assert_pixels(kind, &noisy[..noisy.len() - 2], compatible());
    }
    let progressive = jpeg(Kind::Progressive, &[0x7f, 0xff, 0xd0, 0x7f], 1);
    let last_scan = MarkerReader::new(&progressive)
        .unwrap()
        .filter_map(|event| match event.unwrap() {
            Event::Scan { offset, .. } => Some(offset),
            _ => None,
        })
        .nth(1)
        .unwrap();
    assert_rejected(Kind::Progressive, &progressive[..last_scan]);

    // All bands introduced at Al=1 are valid when EOI explicitly finishes the
    // image. Without it, an interrupted refinement sequence cannot be ruled out.
    let mut partial_precision = progressive.clone();
    for event in MarkerReader::new(&progressive).unwrap() {
        if let Event::Scan { header, offset, .. } = event.unwrap() {
            partial_precision[offset + 4 + header.len() - 1] = 1;
        }
    }
    assert_pixels(
        Kind::Progressive,
        &partial_precision,
        DecodeOptions::default(),
    );
    assert_pixels(Kind::Progressive, &partial_precision, compatible());
    assert_rejected(
        Kind::Progressive,
        &partial_precision[..partial_precision.len() - 2],
    );

    // A declared but incomplete segment is not a missing image terminator.
    let mut partial = jpeg(Kind::Sequential, &[0x3f, 0xff, 0xd0, 0x3f], 1);
    partial.truncate(partial.len() - 2);
    partial.extend([0xff, 0xfe, 0, 8, b'a']);
    assert_rejected(Kind::Sequential, &partial);
}

#[test]
fn compatibility_preserves_pixel_memory_metadata_and_scan_limits() {
    for kind in KINDS {
        let bytes = jpeg(kind, &[0, 0xff, 0xd0, 0], 1);
        let mut comment = Vec::new();
        segment(&mut comment, 0xfe, b"metadata");
        let bytes = insert(&bytes, 2, &comment);
        assert_pixels(kind, &bytes, compatible());
        for limits in [
            Limits {
                max_pixels: 1,
                ..Limits::default()
            },
            Limits {
                max_memory_bytes: 1,
                ..Limits::default()
            },
            Limits {
                max_metadata_bytes: 1,
                ..Limits::default()
            },
            Limits {
                max_scans: 0,
                ..Limits::default()
            },
        ] {
            let options = DecodeOptions {
                limits,
                strict: false,
            };
            for input in [&bytes[..], &bytes[..bytes.len() - 2]] {
                assert!(matches!(
                    decode_with_options(input, options, Upsampling::Linear),
                    Err(Error::LimitExceeded(_))
                ));
                assert!(matches!(
                    model_pixels(kind, input, options),
                    Err(Error::LimitExceeded(_))
                ));
            }
        }
    }
}
