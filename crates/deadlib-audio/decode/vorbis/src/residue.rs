use crate::{
    Error, Residue, Residue2Layout, ResidueBehavior, bitpack::BitReader, codebook::Codebook,
};

#[cfg(test)]
#[path = "residue_reference.rs"]
mod reference_tests;

pub(crate) struct Output<'a> {
    pub behavior: ResidueBehavior,
    pub layout: Residue2Layout,
    pub spectrum: &'a mut [f32],
    pub stride: usize,
    pub bins: usize,
    pub bundle: &'a [usize],
    pub nonzero: &'a [bool],
    pub classes: &'a mut [u8],
}

/// EndOfPacket leaves completed residue work intact. Type zero commits only
/// complete partitions, matching libvorbis's decodevs_add behavior. The packet
/// decoder treats truncation as nominal and proceeds to coupling/floor synthesis.
pub(crate) fn decode(
    config: &Residue,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut Output<'_>,
) -> Result<(), Error> {
    let exhausted = bits.is_exhausted();
    let behavior = output.behavior;
    let result = decode_inner(config, books, bits, output);
    finish_reader(bits, behavior, exhausted, result)
}
fn finish_reader(
    bits: &mut BitReader<'_>,
    behavior: ResidueBehavior,
    exhausted: bool,
    result: Result<(), Error>,
) -> Result<(), Error> {
    if behavior == ResidueBehavior::Libvorbis
        && !exhausted
        && matches!(result, Err(Error::EndOfPacket))
    {
        bits.clear_exhausted();
    }
    result
}
fn decode_inner(
    config: &Residue,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut Output<'_>,
) -> Result<(), Error> {
    let channels = output.bundle.len();
    if channels == 0 || !output.bundle.iter().any(|&ch| output.nonzero[ch]) {
        return Ok(());
    }
    let interleaved = config.kind == 2;
    let actual = output.bins * if interleaved { channels } else { 1 };
    let begin = (config.begin as usize).min(actual);
    let end = (config.end as usize).min(actual);
    let partition_size = config.partition_size as usize;
    let partitions = end.saturating_sub(begin) / partition_size;
    if partitions == 0 {
        return Ok(());
    }
    let classbook = &books[config.classbook as usize];
    // A zero-used-entry scalar book fails without reading bits. Stop this
    // residue only, so later submaps can still consume their own data.
    if classbook.is_empty() {
        return Ok(());
    }
    let dimensions = classbook.dimensions();
    let vectors = if interleaved { 1 } else { channels };
    let classifications = config.books.len() as u32;
    // libvorbis runs only through the highest active cascade bit. In
    // particular an all-zero cascade consumes no phrasebook words, which
    // matters when a later submap reads from this same packet bit reader.
    let stages = config
        .books
        .iter()
        .filter_map(|books| books.iter().rposition(Option::is_some))
        .max()
        .map_or(0, |last| last + 1);
    for pass in 0..stages {
        let mut partition = 0;
        while partition < partitions {
            if pass == 0 {
                for ch in 0..vectors {
                    if !interleaved && !output.nonzero[output.bundle[ch]] {
                        continue;
                    }
                    let mut value = classbook.decode_scalar(bits)?;
                    for i in (0..dimensions).rev() {
                        if i < partitions - partition {
                            output.classes[ch * partitions + partition + i] =
                                (value % classifications) as u8;
                        }
                        value /= classifications;
                    }
                    if value != 0 {
                        // Historical encoders can declare a phrasebook larger
                        // than the classification cardinality. Like libvorbis,
                        // stop this residue at a surplus word, keeping completed
                        // additions and leaving following submap bits available.
                        return Ok(());
                    }
                }
            }
            for _ in 0..dimensions {
                if partition == partitions {
                    break;
                }
                for ch in 0..vectors {
                    if !interleaved && !output.nonzero[output.bundle[ch]] {
                        continue;
                    }
                    let class = output.classes[ch * partitions + partition] as usize;
                    let Some(book) = config.books[class][pass] else {
                        continue;
                    };
                    let book = &books[book as usize];
                    // All three reference vector-add routines are no-ops for
                    // empty books, including zero-dimensional empty VQ books.
                    if book.is_empty() {
                        continue;
                    }
                    let start = begin + partition * partition_size;
                    decode_partition(config.kind, bits, book, output, ch, start, partition_size)?;
                }
                partition += 1;
            }
        }
    }
    Ok(())
}
fn decode_partition(
    kind: u8,
    bits: &mut BitReader<'_>,
    book: &Codebook,
    output: &mut Output<'_>,
    ch: usize,
    start: usize,
    size: usize,
) -> Result<(), Error> {
    let exhausted = bits.is_exhausted();
    let behavior = output.behavior;
    let result = decode_partition_inner(kind, bits, book, output, ch, start, size);
    finish_reader(bits, behavior, exhausted, result)
}
fn decode_partition_inner(
    kind: u8,
    bits: &mut BitReader<'_>,
    book: &Codebook,
    output: &mut Output<'_>,
    ch: usize,
    start: usize,
    size: usize,
) -> Result<(), Error> {
    if book.is_empty() {
        return Ok(());
    }
    let dimensions = book.dimensions();
    if kind == 0 {
        let step = size / dimensions;
        // The reference gathers all codewords before adding this partition.
        // Preflight a reader clone so an incomplete partition leaves earlier
        // passes/partitions intact without allocating packet scratch.
        let mut probe = bits.clone();
        for _ in 0..step {
            if let Err(error) = book.decode_vector(&mut probe) {
                *bits = probe;
                return Err(error);
            }
        }
        let plane = output.bundle[ch] * output.stride;
        for i in 0..step {
            let vector = book.decode_vector(bits)?;
            for (j, &value) in vector.iter().enumerate() {
                output.spectrum[plane + start + i + j * step] += value;
            }
        }
    } else if kind == 2 {
        // Traverse the flattened channel interleave in stream order. A vector
        // can begin/end between channels, so keep the cursor across vectors;
        // divide only once per partition rather than once per coefficient.
        let channels = output.bundle.len();
        let (start, size) = if output.layout == Residue2Layout::Libvorbis {
            let end = (start + size) / channels * channels;
            let start = start / channels * channels;
            (start, end - start)
        } else {
            (start, size)
        };
        let mut channel = start % channels;
        let mut bin = start / channels;
        for offset in (0..size).step_by(dimensions) {
            let vector = book.decode_vector(bits)?;
            // A rounded reference range may end inside the last VQ vector.
            // Its codeword is consumed, but components past the range are not.
            for &value in vector.iter().take(size - offset) {
                output.spectrum[output.bundle[channel] * output.stride + bin] += value;
                channel += 1;
                if channel == channels {
                    channel = 0;
                    bin += 1;
                }
            }
        }
    } else {
        let plane = output.bundle[ch] * output.stride;
        for i in (0..size).step_by(dimensions) {
            let vector = book.decode_vector(bits)?;
            for (j, &value) in vector.iter().take(size - i).enumerate() {
                output.spectrum[plane + start + i + j] += value;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, bitpack::BitWriter};
    #[test]
    fn empty_vector_stage_preserves_earlier_additions_later_stages_and_reader() {
        use crate::codebook::empty_test_book;
        for dimensions in [0, 1, 3, u16::MAX] {
            let books = [vector_book(), empty_test_book(dimensions, 2)];
            let mut packet = BitWriter::new();
            packet.write(0, 32).unwrap();
            packet.write(0, 9).unwrap();
            packet.write(21, 5).unwrap();
            let bytes = packet.finish().unwrap();
            for kind in 0..=2 {
                let config = Residue {
                    kind,
                    begin: 1,
                    end: 41,
                    partition_size: 20,
                    classbook: 0,
                    books: vec![[Some(0), Some(1), Some(0), None, None, None, None, None]],
                };
                let mut actual = [0.125; 3 * 68];
                let mut expected = actual;
                for i in 0..40 {
                    expected[2 * 68 + 1 + i] += 2.0
                        * if kind == 0 {
                            if i % 20 < 10 { 1.0 } else { 2.0 }
                        } else {
                            (i % 2 + 1) as f32
                        };
                }
                let mut bits = BitReader::new(&bytes);
                decode(
                    &config,
                    &books,
                    &mut bits,
                    &mut Output {
                        behavior: ResidueBehavior::Vorbis,
                        layout: Residue2Layout::Interleaved,
                        spectrum: &mut actual,
                        stride: 68,
                        bins: 64,
                        bundle: &[2],
                        nonzero: &[false, false, true],
                        classes: &mut [255; 2],
                    },
                )
                .unwrap();
                assert_eq!(actual, expected, "kind {kind}, dimensions {dimensions}");
                assert_eq!(bits.position(), 41);
                assert!(!bits.is_exhausted());
                assert_eq!(bits.read(5), Ok(21));
            }
        }
    }
    fn vector_book() -> Codebook {
        vector_book_dimensions(2)
    }
    fn vector_book_dimensions(dimensions: usize) -> Codebook {
        let mut bits = BitWriter::new();
        for (value, width) in [
            (0x564342, 24),
            (dimensions as u32, 16),
            (1, 24),
            (0, 1),
            (0, 1),
            (0, 5),
            (2, 4),
            (0, 32),
            ((788 << 21) | 1, 32),
            (3, 4),
            (0, 1),
        ] {
            bits.write(value, width).unwrap();
        }
        for value in 1..=dimensions {
            bits.write(value as u32, 4).unwrap();
        }
        let bytes = bits.finish().unwrap();
        Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).unwrap()
    }
    #[test]
    fn surplus_classwords_keep_completed_residue_and_following_bits() {
        let mut packed = BitWriter::new();
        for (value, width) in [(0x564342, 24), (1, 16), (8, 24), (0, 1), (0, 1)] {
            packed.write(value, width).unwrap();
        }
        for _ in 0..8 {
            packed.write(2, 5).unwrap();
        }
        packed.write(0, 4).unwrap();
        let packed = packed.finish().unwrap();
        let books = [
            Codebook::read(&mut BitReader::new(&packed), &Limits::default()).unwrap(),
            vector_book(),
        ];
        let mut packet = BitWriter::new();
        packet.write(0, 3).unwrap(); // Valid classword, then ten VQ entries.
        packet.write(0, 10).unwrap();
        packet.write(7, 3).unwrap(); // Surplus classword, not an EOF.
        packet.write(21, 5).unwrap();
        let packet = packet.finish().unwrap();
        for kind in 0..=2 {
            let config = Residue {
                kind,
                begin: 1,
                end: 41,
                partition_size: 20,
                classbook: 0,
                books: vec![[Some(1), None, None, None, None, None, None, None]],
            };
            let mut actual = [0.125; 3 * 68];
            let mut expected = actual;
            for i in 0..20 {
                expected[2 * 68 + 1 + i] += if kind == 0 {
                    if i < 10 { 1.0 } else { 2.0 }
                } else {
                    (i % 2 + 1) as f32
                };
            }
            let mut bits = BitReader::new(&packet);
            assert_eq!(
                decode(
                    &config,
                    &books,
                    &mut bits,
                    &mut Output {
                        behavior: ResidueBehavior::Vorbis,
                        layout: Residue2Layout::Interleaved,
                        spectrum: &mut actual,
                        stride: 68,
                        bins: 64,
                        bundle: &[2],
                        nonzero: &[false, false, true],
                        classes: &mut [255; 2],
                    }
                ),
                Ok(())
            );
            assert_eq!(actual, expected, "residue={kind}");
            assert_eq!(bits.position(), 16);
            assert!(!bits.is_exhausted());
            assert_eq!(bits.read(5), Ok(21));
        }
    }
    #[test]
    fn stageless_residue_leaves_packet_and_output_untouched() {
        let books = [vector_book()];
        for kind in 0..=2 {
            let config = Residue {
                kind,
                begin: 0,
                end: 32,
                partition_size: 8,
                classbook: 0,
                books: vec![[None; 8]],
            };
            for packet in [&[][..], &[0xff][..]] {
                let mut bits = BitReader::new(packet);
                let mut spectrum = [0.125; 64];
                let mut classes = [255; 8];
                assert_eq!(
                    decode(
                        &config,
                        &books,
                        &mut bits,
                        &mut Output {
                            behavior: ResidueBehavior::Vorbis,
                            layout: Residue2Layout::Interleaved,
                            spectrum: &mut spectrum,
                            stride: 32,
                            bins: 32,
                            bundle: &[0, 1],
                            nonzero: &[true, true],
                            classes: &mut classes,
                        }
                    ),
                    Ok(())
                );
                assert_eq!(bits.position(), 0);
                assert!(!bits.is_exhausted());
                assert_eq!(spectrum, [0.125; 64]);
                assert_eq!(classes, [255; 8]);
            }
        }
    }
    #[test]
    fn residue_zero_truncation_keeps_only_completed_partitions() {
        let books = [vector_book()];
        let config = Residue {
            kind: 0,
            begin: 1,
            end: 41,
            partition_size: 20,
            classbook: 0,
            books: vec![[Some(0), None, None, None, None, None, None, None]],
        };
        for bytes in [&[][..], &[0][..], &[0, 0][..], &[0, 0, 0][..]] {
            let mut actual = [0.125; 3 * 68];
            let mut expected = actual;
            // One classword bit precedes two ten-codeword partitions.
            let completed = (bytes.len() * 8).saturating_sub(1) / 10;
            for partition in 0..completed.min(2) {
                for i in 0..20 {
                    expected[2 * 68 + 1 + partition * 20 + i] += if i < 10 { 1.0 } else { 2.0 };
                }
            }
            let mut bits = BitReader::new(bytes);
            let result = decode(
                &config,
                &books,
                &mut bits,
                &mut Output {
                    behavior: ResidueBehavior::Vorbis,
                    layout: Residue2Layout::Interleaved,
                    spectrum: &mut actual,
                    stride: 68,
                    bins: 64,
                    bundle: &[2],
                    nonzero: &[false, false, true],
                    classes: &mut [0; 2],
                },
            );
            assert_eq!(
                result,
                if completed >= 2 {
                    Ok(())
                } else {
                    Err(Error::EndOfPacket)
                }
            );
            assert_eq!(bits.position(), (bytes.len() * 8).min(21));
            assert_eq!(actual, expected, "bytes={}", bytes.len());
        }
    }
    #[test]
    fn residue_two_scatter_covers_pairs_generic_interleaves_and_truncation() {
        for layout in [Residue2Layout::Interleaved, Residue2Layout::Libvorbis] {
            for dimensions in [1, 2, 3, 4, 8] {
                let book = vector_book_dimensions(dimensions);
                for channels in [1usize, 2, 3, 4, 5, 7, 8, 17, 255] {
                    let bundle: Vec<_> = (1..=channels).rev().collect();
                    for start in [0, 1, channels - 1] {
                        for size in [1usize, 7, 20, 23] {
                            let (first, count) = if layout == Residue2Layout::Libvorbis {
                                let first = start / channels * channels;
                                (first, (start + size) / channels * channels - first)
                            } else {
                                (start, size)
                            };
                            let vectors = count.div_ceil(dimensions);
                            let bins = (start + vectors * dimensions).div_ceil(channels) + 1;
                            let stride = bins + 3;
                            for bytes in [&[][..], &[0][..], &[0, 0][..]] {
                                let mut actual = vec![0.125; (channels + 2) * stride];
                                let mut expected = actual.clone();
                                let decoded_vectors = vectors.min(bytes.len() * 8);
                                let decoded = (decoded_vectors * dimensions).min(count);
                                for i in 0..decoded {
                                    let flat = first + i;
                                    expected[bundle[flat % channels] * stride + flat / channels] +=
                                        (i % dimensions + 1) as f32;
                                }
                                let mut bits = BitReader::new(bytes);
                                let result = decode_partition(
                                    2,
                                    &mut bits,
                                    &book,
                                    &mut Output {
                                        behavior: ResidueBehavior::Vorbis,
                                        layout,
                                        spectrum: &mut actual,
                                        stride,
                                        bins,
                                        bundle: &bundle,
                                        nonzero: &[],
                                        classes: &mut [],
                                    },
                                    0,
                                    start,
                                    size,
                                );
                                assert_eq!(
                                    result,
                                    if decoded_vectors == vectors {
                                        Ok(())
                                    } else {
                                        Err(Error::EndOfPacket)
                                    }
                                );
                                assert_eq!(bits.position(), decoded_vectors);
                                assert_eq!(
                                    actual,
                                    expected,
                                    "channels={channels}, start={start}, bytes={}",
                                    bytes.len()
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn residue_interleaves_clips_and_propagates_truncation() {
        let books = [vector_book()];
        let bundle = [2, 0];
        let nonzero = [false, false, true];
        for kind in 0..=2 {
            let config = Residue {
                kind,
                begin: 0,
                end: 8,
                partition_size: 4,
                classbook: 0,
                books: vec![[Some(0), None, None, None, None, None, None, None]],
            };
            let mut spectrum = [0.0; 12];
            let mut classes = [0; 12];
            decode(
                &config,
                &books,
                &mut BitReader::new(&[0]),
                &mut Output {
                    behavior: ResidueBehavior::Vorbis,
                    layout: Residue2Layout::Interleaved,
                    spectrum: &mut spectrum,
                    stride: 4,
                    bins: 4,
                    bundle: &bundle,
                    nonzero: &nonzero,
                    classes: &mut classes,
                },
            )
            .unwrap();
            if kind == 0 {
                assert_eq!(&spectrum[8..], [1.0, 1.0, 2.0, 2.0]);
                assert_eq!(&spectrum[..4], [0.0; 4]);
            } else if kind == 1 {
                assert_eq!(&spectrum[8..], [1.0, 2.0, 1.0, 2.0]);
                assert_eq!(&spectrum[..4], [0.0; 4]);
            } else {
                assert_eq!(&spectrum[8..], [1.0; 4]);
                assert_eq!(&spectrum[..4], [2.0; 4]);
            }
            spectrum.fill(0.0);
            assert_eq!(
                decode(
                    &config,
                    &books,
                    &mut BitReader::new(&[]),
                    &mut Output {
                        behavior: ResidueBehavior::Vorbis,
                        layout: Residue2Layout::Interleaved,
                        spectrum: &mut spectrum,
                        stride: 4,
                        bins: 4,
                        bundle: &bundle,
                        nonzero: &nonzero,
                        classes: &mut classes
                    }
                ),
                Err(Error::EndOfPacket)
            );
            let mut bits = BitReader::new(&[]);
            decode(
                &config,
                &books,
                &mut bits,
                &mut Output {
                    behavior: ResidueBehavior::Vorbis,
                    layout: Residue2Layout::Interleaved,
                    spectrum: &mut spectrum,
                    stride: 4,
                    bins: 4,
                    bundle: &bundle,
                    nonzero: &[false; 3],
                    classes: &mut classes,
                },
            )
            .unwrap();
            assert!(!bits.is_exhausted());
            let empty = Residue {
                begin: 1000,
                end: 2000,
                ..config
            };
            decode(
                &empty,
                &books,
                &mut bits,
                &mut Output {
                    behavior: ResidueBehavior::Vorbis,
                    layout: Residue2Layout::Interleaved,
                    spectrum: &mut spectrum,
                    stride: 4,
                    bins: 4,
                    bundle: &bundle,
                    nonzero: &nonzero,
                    classes: &mut classes,
                },
            )
            .unwrap();
        }
    }
}
