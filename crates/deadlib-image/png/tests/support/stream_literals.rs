//! Independent fixed/dynamic literal runs followed by history references.
use super::huffman::{Case, Codes, Wire, lengths};

fn distance(value: usize) -> (usize, u8, u32) {
    if value <= 4 {
        return (value - 1, 0, 0);
    }
    let mut base = 5;
    for bits in 1..=13u8 {
        for group in 0..2 {
            if value < base + (1usize << bits) {
                return (
                    4 + 2 * usize::from(bits - 1) + group,
                    bits,
                    (value - base) as u32,
                );
            }
            base += 1usize << bits;
        }
    }
    panic!("distance outside DEFLATE range");
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for prefix in [
        0, 1, 2, 7, 8, 15, 16, 31, 32, 257, 258, 259, 32767, 32768, 32769, 65535, 65536, 65537,
    ] {
        for back in [0, 1, 2, 3, 257, 32767, 32768] {
            if back > prefix {
                continue;
            }
            for dynamic in [false, true] {
                let mut wire = Wire::default();
                let (literal_lengths, distance_lengths) = if dynamic {
                    let mut lit = vec![8; 286];
                    lit[226..].fill(9);
                    let mut dist = vec![5; 30];
                    dist[..2].fill(4);
                    lengths(&mut wire, &lit, &dist);
                    (lit, dist)
                } else {
                    wire.put(3, 3);
                    let mut lit = vec![8; 288];
                    lit[144..256].fill(9);
                    lit[256..280].fill(7);
                    (lit, vec![5; 32])
                };
                let lit = Codes::new(&literal_lengths);
                let dist = Codes::new(&distance_lengths);
                let mut raw: Vec<_> = (0..prefix)
                    .map(|n| ((n * 73) ^ (n >> 3) ^ (n >> 9)) as u8)
                    .collect();
                for &byte in &raw {
                    lit.symbol(&mut wire, usize::from(byte));
                }
                if back != 0 {
                    let (symbol, extra, value) = distance(back);
                    for _ in 0..2 {
                        lit.symbol(&mut wire, 285); // length 258, no extra bits
                        dist.symbol(&mut wire, symbol);
                        wire.put(value, extra);
                        for _ in 0..258 {
                            raw.push(raw[raw.len() - back]);
                        }
                    }
                }
                lit.symbol(&mut wire, 256);
                cases.push(Case {
                    name: format!("literal-run-{prefix}-{back}-{dynamic}"),
                    bytes: wire.finish(&raw, prefix & 1 != 0),
                    output: Some(raw),
                    extended_distance: false,
                });
            }
        }
    }
    assert_eq!(cases.len(), 172);
    cases
}
pub fn seeds() -> Vec<Case> {
    cases()
        .into_iter()
        .filter(|c| c.bytes.len() <= 65536)
        .collect()
}
