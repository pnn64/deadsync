//! Independent complete canonical trees for two-literal values and bit widths.
use super::huffman::{Case, Codes, Wire, lengths};

fn fixture(name: String, lengths_data: &[u8], raw: &[u8], ones: bool) -> Case {
    let mut wire = Wire::default();
    lengths(&mut wire, lengths_data, &[0]);
    let codes = Codes::new(lengths_data);
    for &byte in raw {
        codes.symbol(&mut wire, usize::from(byte));
    }
    codes.symbol(&mut wire, 256);
    Case {
        name,
        bytes: wire.finish(raw, ones),
        output: Some(raw.to_vec()),
        extended_distance: false,
    }
}
pub fn byte_pair(first: u8, second: u8) -> Case {
    let mut lens = vec![0; 257];
    lens[usize::from(first)] = 1;
    if first == second {
        lens[256] = 1;
    } else {
        lens[usize::from(second)] = 2;
        lens[256] = 2;
    }
    fixture(
        format!("primary-pair-{first:02x}-{second:02x}"),
        &lens,
        &[first, second],
        first & 1 != 0,
    )
}
pub fn widths() -> Vec<Case> {
    let mut cases = Vec::new();
    for width in 1..=4u8 {
        let leaves = 1usize << width;
        let mut even = vec![0; 257];
        even[..leaves - 1].fill(width);
        even[256] = width;
        for ones in [false, true] {
            cases.push(fixture(
                format!("primary-width-{}-padding-{ones}", width * 2),
                &even,
                &[0, (leaves - 2) as u8],
                ones,
            ));
        }
        let mut odd = vec![0; 257];
        odd[..leaves - 1].fill(width);
        odd[255] = width + 1;
        odd[256] = width + 1;
        for raw in [[0, 255], [255, 0], [0, 0], [255, 255]] {
            for ones in [false, true] {
                cases.push(fixture(
                    format!(
                        "primary-width-{}-{}-{}-padding-{ones}",
                        width * 2 + 1,
                        raw[0],
                        raw[1]
                    ),
                    &odd,
                    &raw,
                    ones,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 40);
    cases
}
pub fn seeds() -> Vec<Case> {
    let mut cases = widths();
    cases.extend(wide_edges());
    for first in 0..=255u8 {
        cases.push(byte_pair(first, first));
        cases.push(byte_pair(first, 255 - first));
    }
    cases
}

pub fn wide_edges() -> Vec<Case> {
    let mut cases = Vec::new();
    for width in [5, 6] {
        let leaves = 1usize << width;
        let mut even = vec![0; 257];
        even[..leaves - 1].fill(width);
        even[256] = width;
        for ones in [false, true] {
            cases.push(fixture(
                format!("wide-even-{width}-{ones}"),
                &even,
                &[0, (leaves - 2) as u8],
                ones,
            ));
        }
        let mut odd = vec![0; 257];
        odd[..leaves - 1].fill(width);
        odd[255] = width + 1;
        odd[256] = width + 1;
        for raw in [[0, 255], [255, 0], [0, 0], [255, 255]] {
            for ones in [false, true] {
                cases.push(fixture(
                    format!("wide-odd-{width}-{}-{}-{ones}", raw[0], raw[1]),
                    &odd,
                    &raw,
                    ones,
                ));
            }
        }
    }
    for width in 7..=15u8 {
        let mut lens = vec![0; 257];
        for (n, value) in lens.iter_mut().take(usize::from(width - 1)).enumerate() {
            *value = (n + 1) as u8;
        }
        lens[255] = width;
        lens[256] = width;
        for raw in [[0, 255], [255, 0], [0, 0], [255, 255]] {
            for ones in [false, true] {
                cases.push(fixture(
                    format!("wide-chain-{width}-{}-{}-{ones}", raw[0], raw[1]),
                    &lens,
                    &raw,
                    ones,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 92);
    cases
}
