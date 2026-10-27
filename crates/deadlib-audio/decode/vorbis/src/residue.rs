use crate::{Error, Residue, bitpack::BitReader, codebook::Codebook};

pub(crate) struct Output<'a> {
    pub spectrum: &'a mut [f32],
    pub stride: usize,
    pub bins: usize,
    pub bundle: &'a [usize],
    pub nonzero: &'a [bool],
    pub classes: &'a mut [u8],
}

/// EndOfPacket leaves already decoded vectors intact. The packet decoder
/// treats this as nominal truncation and proceeds to coupling/floor synthesis.
pub(crate) fn decode(
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
    let partitions = (end - begin) / partition_size;
    if partitions == 0 {
        return Ok(());
    }
    let classbook = &books[config.classbook as usize];
    let dimensions = classbook.dimensions();
    let vectors = if interleaved { 1 } else { channels };
    let classifications = config.books.len() as u32;
    for pass in 0..8 {
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
                        return Err(Error::Invalid("residue classword exceeds classifications"));
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
    let dimensions = book.dimensions();
    if kind == 0 {
        let step = size / dimensions;
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
        let mut channel = start % channels;
        let mut bin = start / channels;
        for _ in (0..size).step_by(dimensions) {
            let vector = book.decode_vector(bits)?;
            for &value in vector {
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
            for (j, &value) in vector.iter().enumerate() {
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
    fn vector_book() -> Codebook {
        let mut bits = BitWriter::new();
        for (value, width) in [
            (0x564342, 24),
            (2, 16),
            (1, 24),
            (0, 1),
            (0, 1),
            (0, 5),
            (2, 4),
            (0, 32),
            ((788 << 21) | 1, 32),
            (1, 4),
            (0, 1),
            (1, 2),
            (2, 2),
        ] {
            bits.write(value, width).unwrap();
        }
        let bytes = bits.finish().unwrap();
        Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).unwrap()
    }
    #[test]
    fn residue_two_cursor_crosses_vector_channel_and_partition_edges_before_truncation() {
        let book = vector_book();
        for channels in [1usize, 2, 3, 4, 5, 7, 8, 17, 255] {
            let bundle: Vec<_> = (1..=channels).rev().collect();
            for start in [0, 1, channels - 1] {
                let size = 20;
                let bins = (start + size).div_ceil(channels) + 1;
                let stride = bins + 3;
                for bytes in [&[][..], &[0][..], &[0, 0][..]] {
                    let mut actual = vec![0.125; (channels + 2) * stride];
                    let mut expected = actual.clone();
                    let decoded = size.min(bytes.len() * 8 * 2);
                    for i in 0..decoded {
                        let flat = start + i;
                        expected[bundle[flat % channels] * stride + flat / channels] +=
                            if i % 2 == 0 { 1.0 } else { 2.0 };
                    }
                    let mut bits = BitReader::new(bytes);
                    let result = decode_partition(
                        2,
                        &mut bits,
                        &book,
                        &mut Output {
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
                        if decoded == size {
                            Ok(())
                        } else {
                            Err(Error::EndOfPacket)
                        }
                    );
                    assert_eq!(bits.position(), decoded / 2);
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
