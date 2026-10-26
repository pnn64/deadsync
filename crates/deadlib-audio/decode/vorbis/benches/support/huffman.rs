use deadlib_audio_decode_vorbis::{bitpack::BitReader, codebook::Huffman};
use std::{hint::black_box, time::Instant};

fn take<'a>(input: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (bytes, rest) = input.split_at(n);
    *input = rest;
    bytes
}
fn u32_in(input: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(input, 4).try_into().unwrap())
}
struct Book<'a> {
    tree: Huffman,
    wire: &'a [u8],
    expected: Vec<u32>,
}
pub fn measure(data: &[u8], iterations: usize) {
    let mut input = data;
    assert_eq!(take(&mut input, 8), b"DLVHUF1\0");
    let count = u32_in(&mut input);
    assert!((1..=256).contains(&count));
    let mut books = Vec::new();
    let mut symbols = 0;
    let mut expected_sum = 0u64;
    for _ in 0..count {
        let entries = u32_in(&mut input) as usize;
        let lengths = take(&mut input, entries);
        let bit_count = u64::from_le_bytes(take(&mut input, 8).try_into().unwrap());
        let bytes = u32_in(&mut input) as usize;
        let wire = take(&mut input, bytes);
        let tree = Huffman::from_lengths(lengths, 2 << 20).unwrap();
        let expected: Vec<_> = lengths
            .iter()
            .enumerate()
            .filter(|&(_, &length)| length != 0)
            .map(|(entry, _)| entry as u32)
            .collect();
        let mut bits = BitReader::new(wire);
        for &entry in &expected {
            assert_eq!(tree.decode(&mut bits).unwrap(), entry);
            expected_sum += entry as u64;
        }
        assert_eq!(bits.position() as u64, bit_count);
        symbols += expected.len();
        books.push(Book {
            tree,
            wire,
            expected,
        });
    }
    assert!(input.is_empty());
    let begin = Instant::now();
    let mut sum = 0u64;
    for _ in 0..iterations {
        for book in &books {
            let mut bits = BitReader::new(black_box(book.wire));
            for _ in 0..book.expected.len() {
                sum = sum.wrapping_add(book.tree.decode(&mut bits).unwrap() as u64);
            }
        }
        black_box(sum);
    }
    let ns = begin.elapsed().as_nanos();
    assert_eq!(sum, expected_sum.wrapping_mul(iterations as u64));
    println!(
        "native\thuffman\t{iterations}\t{ns}\t{}\t0\t0",
        symbols * iterations
    );
}
