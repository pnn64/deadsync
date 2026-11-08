//! Independently assembled Huffman/stored transitions and short final codes.
use super::huffman;

pub struct Case {
    pub name: String,
    pub bytes: Vec<u8>,
    pub raw: Vec<u8>,
}

pub fn suffix(length: usize, salt: usize) -> Vec<u8> {
    (0..length)
        .map(|n| ((n * 137 + salt * 73) ^ (n >> 2)) as u8)
        .collect()
}

pub fn fixtures() -> Vec<Case> {
    let mut result = Vec::new();
    let mut fixed = [8; 288];
    fixed[144..256].fill(9);
    fixed[256..280].fill(7);
    let fixed = huffman::Codes::new(&fixed);
    for before in 0..17 {
        for stored in 0..18 {
            for ones in [false, true] {
                let mut wire = huffman::Wire::default();
                let mut raw = vec![164; before];
                wire.put(2, 3);
                for &byte in &raw {
                    fixed.symbol(&mut wire, byte as usize);
                }
                fixed.symbol(&mut wire, 256);
                let tail = suffix(stored, before);
                wire.stored(&tail, false);
                raw.extend(tail);
                wire.put(3, 3);
                for byte in [0, 255, 144, 143, 3, 127, 248, 1] {
                    fixed.symbol(&mut wire, byte as usize);
                    raw.push(byte);
                }
                fixed.symbol(&mut wire, 256);
                result.push(Case {
                    name: format!("mixed-before-{before}-stored-{stored}-pad-{ones}"),
                    bytes: wire.finish(&raw, ones),
                    raw,
                });
            }
        }
    }
    for length in 0..64 {
        for ones in [false, true] {
            let mut lengths = vec![0; 257];
            lengths[65] = 1;
            lengths[256] = 1;
            let codes = huffman::Codes::new(&lengths);
            let mut wire = huffman::Wire::default();
            huffman::lengths(&mut wire, &lengths, &[0]);
            for _ in 0..length {
                codes.symbol(&mut wire, 65);
            }
            codes.symbol(&mut wire, 256);
            let raw = vec![65; length];
            result.push(Case {
                name: format!("one-bit-length-{length}-pad-{ones}"),
                bytes: wire.finish(&raw, ones),
                raw,
            });
        }
    }
    assert_eq!(result.len(), 740);
    result
}
