//! Independent complete-tree shapes, cyclic symbol assignments and RFC wires.
#![allow(dead_code)]
use super::{chunks, huffman as wire};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Family {
    Literals,
    Lengths,
    Distances,
    CodeLengths,
}
impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Self::Literals => "literals",
            Self::Lengths => "lengths",
            Self::Distances => "distances",
            Self::CodeLengths => "code-lengths",
        }
    }
    pub fn alphabet(self) -> usize {
        match self {
            Self::Literals => 257,
            Self::Lengths => 286,
            Self::Distances => 30,
            Self::CodeLengths => 19,
        }
    }
    pub fn maximum(self) -> u8 {
        if self == Self::CodeLengths { 7 } else { 15 }
    }
    pub fn count(self) -> usize {
        match self {
            Self::Literals => 4626,
            Self::Lengths => 5148,
            Self::Distances => 780,
            Self::CodeLengths => 190,
        }
    }
}
pub const FAMILIES: [Family; 4] = [
    Family::Literals,
    Family::Lengths,
    Family::Distances,
    Family::CodeLengths,
];
pub const COUNT: usize = 10744;

pub struct Case {
    pub name: String,
    pub bytes: Vec<u8>,
    pub raw: Vec<u8>,
    pub assignment: Vec<u8>,
}

fn shape(leaves: usize, maximum: u8, growth_depth: Option<u8>) -> Vec<u8> {
    // Begin with a complete comb. Splitting one leaf preserves Kraft's sum.
    let mut slots: Vec<_> = (1..maximum)
        .chain([maximum, maximum])
        .map(|depth| (depth, growth_depth.is_none_or(|n| n == depth)))
        .collect();
    assert!(slots.len() <= leaves);
    while slots.len() < leaves {
        let index = slots
            .iter()
            .enumerate()
            .filter(|(_, (_, grow))| *grow)
            .min_by_key(|(_, (depth, _))| *depth)
            .unwrap()
            .0;
        let (depth, grow) = slots.swap_remove(index);
        assert!(depth < maximum);
        slots.extend([(depth + 1, grow), (depth + 1, grow)]);
    }
    let values: Vec<_> = slots.into_iter().map(|(depth, _)| depth).collect();
    assert_eq!(values.iter().copied().max(), Some(maximum));
    assert_eq!(
        values
            .iter()
            .map(|&depth| 1u32 << (maximum - depth))
            .sum::<u32>(),
        1u32 << maximum
    );
    values
}
fn shapes(family: Family) -> Vec<Vec<u8>> {
    let first = if family == Family::Distances || family == Family::CodeLengths {
        5
    } else {
        9
    };
    let mut result: Vec<_> = (first..=family.maximum())
        .map(|depth| shape(family.alphabet(), depth, None))
        .collect();
    // Restrict expansion to one shallow subtree, retaining the comb's other
    // depths. The two choices jointly retain every width 1..=maximum.
    for growth in [1, 2] {
        result.push(shape(family.alphabet(), family.maximum(), Some(growth)));
    }
    result
}

fn literal_data(wire: &mut wire::Wire, codes: &wire::Codes, rotation: usize) -> Vec<u8> {
    let mut raw = vec![0];
    codes.symbol(wire, 0); // PNG filter, independent of the following byte order.
    for n in 0..256 {
        let byte = ((n * 197 + rotation * 29) & 255) as u8;
        codes.symbol(wire, byte as usize);
        raw.push(byte);
    }
    raw
}

fn literals(lengths: &[u8], rotation: usize, matches: bool) -> (wire::Wire, Vec<u8>) {
    let mut bits = wire::Wire::default();
    wire::lengths(&mut bits, lengths, if matches { &[1] } else { &[0] });
    let codes = wire::Codes::new(lengths);
    let mut raw = literal_data(&mut bits, &codes, rotation);
    if matches {
        for n in 0..29 {
            let symbol = 257 + (n * 11 + rotation) % 29;
            codes.symbol(&mut bits, symbol);
            let length = match symbol {
                257..=264 => symbol - 254,
                265..=284 => {
                    let width = (symbol - 265) / 4 + 1;
                    let base = (1usize << (width + 2)) + 3 + (symbol - 265) % 4 * (1 << width);
                    // RFC table range for 284 ends at 257; use 285 for 258.
                    let high = if symbol == 284 {
                        30
                    } else {
                        (1usize << width) - 1
                    };
                    let extra = if rotation & 1 == 0 { 0 } else { high };
                    bits.put(extra as u32, width as u8);
                    base + extra
                }
                285 => 258,
                _ => unreachable!(),
            };
            bits.put(0, 1); // Sole distance code: distance one.
            raw.resize(raw.len() + length, *raw.last().unwrap());
        }
    }
    codes.symbol(&mut bits, 256);
    (bits, raw)
}

fn distances(lengths: &[u8], rotation: usize) -> (wire::Wire, Vec<u8>) {
    let mut state = 0x71e2_9b35u32;
    let mut raw: Vec<_> = (0..32768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    raw[0] = 0;
    let mut bits = wire::Wire::default();
    bits.stored(&raw, false);
    let mut lit = vec![0; 258];
    lit[256] = 1;
    lit[257] = 1;
    wire::lengths(&mut bits, &lit, lengths);
    let lc = wire::Codes::new(&lit);
    let dc = wire::Codes::new(lengths);
    for n in 0..30 {
        let symbol = (n * 7 + rotation) % 30;
        lc.symbol(&mut bits, 257); // Length three.
        dc.symbol(&mut bits, symbol);
        let distance = if symbol < 4 {
            symbol + 1
        } else {
            let width = symbol / 2 - 1;
            let base = (1usize << (width + 1)) + 1 + symbol % 2 * (1 << width);
            let extra = if rotation & 1 == 0 {
                0
            } else {
                (1usize << width) - 1
            };
            bits.put(extra as u32, width as u8);
            base + extra
        };
        assert!((1..=32768).contains(&distance));
        for _ in 0..3 {
            raw.push(raw[raw.len() - distance]);
        }
    }
    lc.symbol(&mut bits, 256);
    (bits, raw)
}

fn code_lengths(lengths: &[u8]) -> (wire::Wire, Vec<u8>) {
    // Complete LL comb with four width-15 leaves; its missing width 14 appears
    // in the complete distance tree. The header uses every CL symbol 0..=18.
    let mut lit = vec![0; 257];
    lit[..4].fill(15);
    for (slot, width) in lit[20..32].iter_mut().zip(2..=13) {
        *slot = width;
    }
    lit[256] = 1;
    let dist: Vec<u8> = (1..=13).chain([14, 14]).collect();
    let mut bits = wire::Wire::default();
    wire::header(
        &mut bits,
        lit.len(),
        dist.len(),
        lengths.try_into().unwrap(),
        19,
    );
    let codes = wire::Codes::new(lengths);
    codes.symbol(&mut bits, 15);
    codes.symbol(&mut bits, 16);
    bits.put(0, 2); // 4 x 15.
    codes.symbol(&mut bits, 0);
    for count in [5, 10] {
        codes.symbol(&mut bits, 17);
        bits.put(count - 3, 3);
    }
    for width in 2..=13 {
        codes.symbol(&mut bits, width);
    }
    for count in [138, 86] {
        codes.symbol(&mut bits, 18);
        bits.put(count - 11, 7);
    }
    codes.symbol(&mut bits, 1);
    for &width in &dist {
        codes.symbol(&mut bits, width as usize);
    }
    let codes = wire::Codes::new(&lit);
    let raw: Vec<u8> = [0].into_iter().chain(0..=3).chain(20..=31).collect();
    for &byte in &raw {
        codes.symbol(&mut bits, byte as usize);
    }
    codes.symbol(&mut bits, 256);
    (bits, raw)
}

pub fn cases(family: Family) -> Vec<Case> {
    let mut result = Vec::new();
    for (index, shape) in shapes(family).iter().enumerate() {
        for rotation in 0..family.alphabet() {
            let assignment: Vec<_> = (0..shape.len())
                .map(|n| shape[(n + rotation) % shape.len()])
                .collect();
            for ones in [false, true] {
                let (bits, raw) = match family {
                    Family::Literals => literals(&assignment, rotation, false),
                    Family::Lengths => literals(&assignment, rotation, true),
                    Family::Distances => distances(&assignment, rotation),
                    Family::CodeLengths => code_lengths(&assignment),
                };
                let bytes = bits.finish(&raw, ones);
                assert!(bytes.len() <= 65536 && raw.len() <= 65536 && raw[0] == 0);
                result.push(Case {
                    name: format!(
                        "huffman-perm-{}-shape-{index}-rotation-{rotation}-pad-{ones}",
                        family.name()
                    ),
                    bytes,
                    raw,
                    assignment: assignment.clone(),
                });
            }
        }
    }
    assert_eq!(result.len(), family.count());
    result
}
pub fn all_cases() -> Vec<Case> {
    let result: Vec<_> = FAMILIES.into_iter().flat_map(cases).collect();
    assert_eq!(result.len(), COUNT);
    result
}
pub fn png(case: &Case) -> Vec<u8> {
    let mut header = (case.raw.len() as u32 - 1).to_be_bytes().to_vec();
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunks::file(&[
        chunks::chunk(*b"IHDR", &header),
        chunks::chunk(*b"IDAT", &case.bytes),
        chunks::chunk(*b"IEND", &[]),
    ])
}
