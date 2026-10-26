//! Independent RFC 1951 wire construction for tests and structured fuzzing.
#![allow(dead_code)]

#[derive(Clone)]
pub struct Case {
    pub name: String,
    pub bytes: Vec<u8>,
    pub output: Option<Vec<u8>>,
    /// Published HDIST range permits definitions which default zlib rejects.
    pub extended_distance: bool,
}

#[derive(Default)]
pub struct Wire {
    bytes: Vec<u8>,
    partial: u8,
    bits: u8,
}
impl Wire {
    pub fn put(&mut self, value: u32, count: u8) {
        for bit in 0..count {
            self.partial |= (((value >> bit) & 1) as u8) << self.bits;
            self.bits += 1;
            if self.bits == 8 {
                self.bytes.push(self.partial);
                self.partial = 0;
                self.bits = 0;
            }
        }
    }
    pub fn align(&mut self, ones: bool) {
        if self.bits != 0 {
            self.put(if ones { 255 } else { 0 }, 8 - self.bits);
        }
    }
    pub fn stored(&mut self, input: &[u8], last: bool) {
        assert!(input.len() <= 65535);
        self.put(u32::from(last), 3);
        self.align(false);
        self.put(input.len() as u32, 16);
        self.put(input.len() as u32 ^ 65535, 16);
        self.bytes.extend_from_slice(input);
    }
    pub fn finish(mut self, raw: &[u8], ones: bool) -> Vec<u8> {
        self.align(ones);
        let mut out = vec![0x78, 0x01];
        out.extend_from_slice(&self.bytes);
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in raw {
            a = (a + u32::from(byte)) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }
}

pub struct Codes {
    lengths: Vec<u8>,
    values: Vec<u32>,
}
impl Codes {
    pub fn new(lengths: &[u8]) -> Self {
        // Canonical assignment by lexicographic (length, symbol) order, without
        // importing the native decoder or using its table construction.
        let mut sorted: Vec<_> = lengths
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, length)| length != 0)
            .collect();
        sorted.sort_unstable_by_key(|&(symbol, length)| (length, symbol));
        let mut values = vec![0; lengths.len()];
        let (mut code, mut previous) = (0u32, 0);
        for (symbol, length) in sorted {
            code <<= length - previous;
            assert!(code < 1 << length);
            values[symbol] = code;
            code += 1;
            previous = length;
        }
        Self {
            lengths: lengths.to_vec(),
            values,
        }
    }
    pub fn symbol(&self, wire: &mut Wire, symbol: usize) {
        let count = self.lengths[symbol];
        assert!(count != 0);
        wire.put(self.values[symbol].reverse_bits() >> (32 - count), count);
    }
}

const ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];
const CL: [u8; 19] = [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5];

pub fn header(wire: &mut Wire, literals: usize, distances: usize, cl: &[u8; 19], count: usize) {
    assert!((257..=288).contains(&literals) && (1..=32).contains(&distances));
    assert!((4..=19).contains(&count));
    wire.put(5, 3);
    wire.put((literals - 257) as u32, 5);
    wire.put((distances - 1) as u32, 5);
    wire.put((count - 4) as u32, 4);
    for &symbol in &ORDER[..count] {
        wire.put(cl[symbol] as u32, 3);
    }
}
pub fn lengths(wire: &mut Wire, literals: &[u8], distances: &[u8]) {
    header(wire, literals.len(), distances.len(), &CL, 19);
    let codes = Codes::new(&CL);
    for &length in literals.iter().chain(distances) {
        codes.symbol(wire, length as usize);
    }
}
fn literal_pair() -> Vec<u8> {
    let mut lengths = vec![0; 257];
    lengths[0] = 1;
    lengths[256] = 1;
    lengths
}
fn literal_match() -> Vec<u8> {
    let mut lengths = vec![0; 258];
    lengths[0] = 1;
    lengths[256] = 2;
    lengths[257] = 2;
    lengths
}
fn emit(wire: &mut Wire, codes: &Codes, symbols: &[usize]) {
    for &symbol in symbols {
        codes.symbol(wire, symbol);
    }
}
fn valid(name: String, wire: Wire, raw: Vec<u8>, extended_distance: bool) -> Case {
    Case {
        name,
        bytes: wire.finish(&raw, false),
        output: Some(raw),
        extended_distance,
    }
}
fn invalid(name: String, wire: Wire) -> Case {
    Case {
        name,
        bytes: wire.finish(&[], false),
        output: None,
        extended_distance: false,
    }
}

pub fn count_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for nl in 257..=288 {
        for nd in 1..=32 {
            for nc in 4..=19 {
                let mut wire = Wire::default();
                header(&mut wire, nl, nd, &[0; 19], nc);
                cases.push(invalid(format!("counts/{nl}/{nd}/{nc}"), wire));
            }
        }
    }
    // Isolate the count limits with an otherwise valid tree, body and Adler.
    for nl in 257..=288 {
        for nd in 1..=32 {
            let mut lit = literal_pair();
            lit.resize(nl, 0);
            let mut wire = Wire::default();
            lengths(&mut wire, &lit, &vec![0; nd]);
            emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
            cases.push(Case {
                name: format!("counts-with-body/{nl}/{nd}"),
                bytes: wire.finish(&[0, 0], false),
                output: (nl <= 286).then(|| vec![0, 0]),
                extended_distance: nl <= 286 && nd > 30,
            });
        }
    }
    // HCLEN=5 is the first count that can represent a positive code length.
    for nc in 5..=19 {
        let mut cl = [0; 19];
        cl[0] = 1;
        cl[8] = 1;
        let mut lit = vec![8; 257];
        lit[255] = 0;
        let mut wire = Wire::default();
        header(&mut wire, 257, 1, &cl, nc);
        let codes = Codes::new(&cl);
        for &length in lit.iter().chain(&[0]) {
            codes.symbol(&mut wire, length as usize);
        }
        emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
        cases.push(valid(format!("cl-count/{nc}"), wire, vec![0, 0], false));
    }
    cases
}

fn histograms(
    depth: usize,
    slots: usize,
    leaves: usize,
    counts: &mut [usize; 8],
    out: &mut Vec<Vec<u8>>,
) {
    if slots + leaves > 19 {
        return;
    }
    if depth == 7 {
        counts[depth] = slots;
        out.push(
            (1..=7)
                .flat_map(|n| std::iter::repeat_n(n as u8, counts[n]))
                .collect(),
        );
        return;
    }
    for used in 0..=slots {
        counts[depth] = used;
        if used == slots {
            counts[depth + 1..].fill(0);
            out.push(
                (1..=7)
                    .flat_map(|n| std::iter::repeat_n(n as u8, counts[n]))
                    .collect(),
            );
        } else {
            histograms(depth + 1, (slots - used) * 2, leaves + used, counts, out);
        }
    }
}
pub fn code_length_cases() -> Vec<Case> {
    let mut shapes = Vec::new();
    histograms(1, 2, 0, &mut [0; 8], &mut shapes);
    let lit = literal_pair();
    let body = Codes::new(&lit);
    let mut cases = Vec::new();
    for (index, shape) in shapes.iter().enumerate() {
        let mut cl = [0u8; 19];
        // Exercise the longest two code lengths for the symbols actually used.
        cl[0] = shape[shape.len() - 2];
        cl[1] = shape[shape.len() - 1];
        for (slot, &length) in shape[..shape.len() - 2].iter().enumerate() {
            cl[slot + 2] = length;
        }
        let codes = Codes::new(&cl);
        let mut wire = Wire::default();
        header(&mut wire, 257, 1, &cl, 19);
        for &length in lit.iter().chain(&[0]) {
            codes.symbol(&mut wire, length as usize);
        }
        emit(&mut wire, &body, &[0, 0, 256]);
        cases.push(valid(
            format!("complete-cl/{index}"),
            wire,
            vec![0, 0],
            false,
        ));
    }
    cases
}

pub fn tree_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for depth in 1..=7 {
        for (label, cl) in [
            ("empty", [0; 19]),
            ("single", {
                let mut a = [0; 19];
                a[0] = depth;
                a
            }),
            ("oversubscribed", {
                let mut a = [0; 19];
                for v in a.iter_mut().take((1usize << depth).min(19)) {
                    *v = depth;
                }
                if (1usize << depth) < 19 {
                    a[1usize << depth] = depth;
                }
                a
            }),
        ] {
            if label == "oversubscribed" && (1usize << depth) >= 19 {
                continue;
            }
            let mut wire = Wire::default();
            header(&mut wire, 257, 1, &cl, 19);
            cases.push(invalid(format!("bad-cl/{label}/{depth}"), wire));
        }
    }
    for depth in 1..=15 {
        // Two equal-length symbols are complete only at depth one.
        for is_distance in [false, true] {
            let mut lit = literal_pair();
            let mut dist = vec![0; 2];
            if is_distance {
                dist.fill(depth);
            } else {
                lit[0] = depth;
                lit[256] = depth;
            }
            let mut wire = Wire::default();
            lengths(&mut wire, &lit, &dist);
            if depth == 1 {
                let codes = Codes::new(&lit);
                emit(&mut wire, &codes, &[0, 0, 256]);
                cases.push(valid(
                    format!("two-leaf/{is_distance}/{depth}"),
                    wire,
                    vec![0, 0],
                    false,
                ));
            } else {
                cases.push(invalid(format!("incomplete/{is_distance}/{depth}"), wire));
            }
        }
        let mut clit = vec![0; 257];
        clit[256] = depth;
        let mut wire = Wire::default();
        lengths(&mut wire, &clit, &[0]);
        if depth == 1 {
            Codes::new(&clit).symbol(&mut wire, 256);
            cases.push(valid("sole-eob".into(), wire, vec![], false));
        } else {
            cases.push(invalid(format!("sole-eob-width/{depth}"), wire));
        }
        let mut wire = Wire::default();
        lengths(&mut wire, &literal_pair(), &[depth]);
        if depth == 1 {
            emit(&mut wire, &Codes::new(&literal_pair()), &[0, 0, 256]);
            cases.push(valid("sole-distance".into(), wire, vec![0, 0], false));
        } else {
            cases.push(invalid(format!("sole-distance-width/{depth}"), wire));
        }
    }
    for count in [0, 1, 3, 4] {
        let mut lit = vec![0; 257];
        for v in lit.iter_mut().take(count) {
            *v = 1;
        }
        let mut wire = Wire::default();
        lengths(&mut wire, &lit, &[0]);
        cases.push(invalid(format!("missing-eob/{count}"), wire));
    }
    for count in [3, 4] {
        let mut lit = vec![0; 257];
        lit[256] = 1;
        lit[..count - 1].fill(1);
        let mut wire = Wire::default();
        lengths(&mut wire, &lit, &[0]);
        cases.push(invalid(format!("oversubscribed-literals/{count}"), wire));
        let mut wire = Wire::default();
        lengths(&mut wire, &literal_pair(), &vec![1; count]);
        cases.push(invalid(format!("oversubscribed-distances/{count}"), wire));
    }
    // A valid single-symbol code leaves the opposite one-bit value unused.
    for distance in [false, true] {
        let mut wire = Wire::default();
        if distance {
            let lit = literal_match();
            lengths(&mut wire, &lit, &[1]);
            emit(&mut wire, &Codes::new(&lit), &[0, 0, 257]);
            wire.put(1, 1);
        } else {
            let mut lit = vec![0; 257];
            lit[256] = 1;
            lengths(&mut wire, &lit, &[0]);
            wire.put(1, 1);
        }
        cases.push(invalid(format!("unused-code/{distance}"), wire));
    }
    let mut wire = Wire::default();
    let lit = literal_match();
    lengths(&mut wire, &lit, &[0]);
    emit(&mut wire, &Codes::new(&lit), &[0, 0, 257]);
    wire.put(0, 1);
    cases.push(invalid("match-with-empty-distance-tree".into(), wire));
    cases
}

fn rle(wire: &mut Wire, codes: &Codes, symbol: usize, count: usize) {
    codes.symbol(wire, symbol);
    let (base, bits) = match symbol {
        16 => (3, 2),
        17 => (3, 3),
        18 => (11, 7),
        _ => unreachable!(),
    };
    assert!((base..base + (1 << bits)).contains(&count));
    wire.put((count - base) as u32, bits);
}
pub fn repeat_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let codes = Codes::new(&CL);
    let lit = literal_pair();
    for symbol in [16, 17, 18] {
        let range = match symbol {
            16 => 3..=6,
            17 => 3..=10,
            _ => 11..=138,
        };
        for count in range {
            let mut wire = Wire::default();
            header(&mut wire, 257, 1, &CL, 19);
            codes.symbol(&mut wire, 1);
            codes.symbol(&mut wire, 0);
            rle(&mut wire, &codes, symbol, count);
            for &length in lit[2 + count..].iter().chain(&[0]) {
                codes.symbol(&mut wire, length as usize);
            }
            emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
            cases.push(valid(
                format!("repeat/{symbol}/{count}"),
                wire,
                vec![0, 0],
                false,
            ));
        }
    }
    // Repeats cross from the literal table into the distance table as one list.
    for count in 3..=6 {
        let (lit, dist) = if count == 3 {
            let mut a = vec![0; 257];
            a[255] = 1;
            a[256] = 1;
            (a, vec![1; 2])
        } else {
            let mut a = vec![0; 257];
            for n in [0, 254, 255, 256] {
                a[n] = 2;
            }
            (a, vec![2; 4])
        };
        let all: Vec<_> = lit.iter().chain(&dist).copied().collect();
        let at = all.len() - count;
        let mut wire = Wire::default();
        header(&mut wire, lit.len(), dist.len(), &CL, 19);
        for &length in &all[..at] {
            codes.symbol(&mut wire, length as usize);
        }
        rle(&mut wire, &codes, 16, count);
        Codes::new(&lit).symbol(&mut wire, 256);
        cases.push(valid(
            format!("cross-repeat16/{count}"),
            wire,
            vec![],
            false,
        ));
    }
    for symbol in [17, 18] {
        for count in if symbol == 17 { 3..=10 } else { 11..=30 } {
            let mut lit = literal_pair();
            lit.resize(286, 0);
            let total = 287;
            let at = total - count;
            let mut wire = Wire::default();
            header(&mut wire, 286, 1, &CL, 19);
            for &length in &lit[..at] {
                codes.symbol(&mut wire, length as usize);
            }
            rle(&mut wire, &codes, symbol, count);
            emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
            cases.push(valid(
                format!("cross-repeat/{symbol}/{count}"),
                wire,
                vec![0, 0],
                false,
            ));
        }
    }
    let mut wire = Wire::default();
    header(&mut wire, 257, 1, &CL, 19);
    rle(&mut wire, &codes, 16, 3);
    cases.push(invalid("repeat16-without-previous".into(), wire));
    for symbol in [16, 17, 18] {
        let count = if symbol == 18 { 11 } else { 3 };
        for over in 1..count {
            let mut wire = Wire::default();
            header(&mut wire, 257, 1, &CL, 19);
            for _ in 0..258 - count + over {
                codes.symbol(&mut wire, 0);
            }
            rle(&mut wire, &codes, symbol, count);
            cases.push(invalid(format!("repeat-overflow/{symbol}/{over}"), wire));
        }
    }
    cases
}

pub fn reserved_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for nd in 1..=32 {
        let lit = literal_pair();
        let mut shapes = vec![vec![0; nd], {
            let mut d = vec![0; nd];
            d[0] = 1;
            d
        }];
        if nd == 31 {
            let mut d = vec![5; 31];
            d[0] = 4;
            shapes.push(d);
        }
        if nd == 32 {
            shapes.push(vec![5; 32]);
        }
        for (index, dist) in shapes.into_iter().enumerate() {
            let mut wire = Wire::default();
            lengths(&mut wire, &lit, &dist);
            emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
            cases.push(valid(
                format!("distance-definitions/{nd}/{index}"),
                wire,
                vec![0, 0],
                nd > 30,
            ));
        }
    }
    let mut fixed_lit = vec![8; 288];
    fixed_lit[144..256].fill(9);
    fixed_lit[256..280].fill(7);
    for symbol in [286, 287] {
        let mut wire = Wire::default();
        wire.put(3, 3);
        Codes::new(&fixed_lit).symbol(&mut wire, symbol);
        cases.push(invalid(format!("reserved-fixed-literal/{symbol}"), wire));
    }
    for symbol in [30, 31] {
        let mut wire = Wire::default();
        wire.put(3, 3);
        let codes = Codes::new(&fixed_lit);
        emit(&mut wire, &codes, &[0, 0, 257]);
        Codes::new(&[5; 32]).symbol(&mut wire, symbol);
        cases.push(invalid(format!("reserved-fixed-distance/{symbol}"), wire));
        let mut dist = vec![0; symbol + 1];
        dist[symbol] = 1;
        let mut wire = Wire::default();
        let lit = literal_pair();
        lengths(&mut wire, &lit, &dist);
        emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
        cases.push(valid(
            format!("unused-reserved-distance/{symbol}"),
            wire,
            vec![0, 0],
            true,
        ));
        let mut wire = Wire::default();
        let lit = literal_match();
        lengths(&mut wire, &lit, &dist);
        emit(&mut wire, &Codes::new(&lit), &[0, 0, 257]);
        Codes::new(&dist).symbol(&mut wire, symbol);
        cases.push(invalid(
            format!("consumed-reserved-distance/{symbol}"),
            wire,
        ));
    }
    cases
}

pub fn all_cases() -> Vec<Case> {
    let mut cases = count_cases();
    cases.extend(code_length_cases());
    cases.extend(tree_cases());
    cases.extend(repeat_cases());
    cases.extend(reserved_cases());
    cases.extend(range_cases());
    cases
}

/// A deterministic smoke corpus spanning every family without thousands of
/// near-identical invalid count fields or complete-tree histograms.
pub fn seed_indices(cases: &[Case]) -> Vec<usize> {
    cases
        .iter()
        .enumerate()
        .filter(|(i, case)| {
            if case.name.starts_with("counts/") {
                i % 256 == 0
            } else if case.name.starts_with("counts-with-body/") {
                i % 32 == 0 || case.extended_distance
            } else if case.name.starts_with("complete-cl/") {
                i % 32 == 0
            } else if case.name.starts_with("balanced-literals/") {
                i % 16 == 0
            } else {
                true
            }
        })
        .map(|(i, _)| i)
        .collect()
}

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn fixed_codes() -> Codes {
    let mut lit = vec![8; 288];
    lit[144..256].fill(9);
    lit[256..280].fill(7);
    Codes::new(&lit)
}
fn matched(
    wire: &mut Wire,
    lit: &Codes,
    dist: &Codes,
    raw: &mut Vec<u8>,
    length: usize,
    distance: usize,
) {
    let l = LENGTH_BASE
        .iter()
        .rposition(|&base| base <= length)
        .unwrap();
    let d = DIST_BASE
        .iter()
        .rposition(|&base| base <= distance)
        .unwrap();
    lit.symbol(wire, 257 + l);
    wire.put((length - LENGTH_BASE[l]) as u32, LENGTH_EXTRA[l]);
    dist.symbol(wire, d);
    wire.put((distance - DIST_BASE[d]) as u32, DIST_EXTRA[d]);
    for _ in 0..length {
        raw.push(raw[raw.len() - distance]);
    }
}
pub fn range_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for depth in 1..=15 {
        let mut lit = vec![0; 257];
        for n in 1..depth {
            lit[n - 1] = n as u8;
        }
        lit[depth - 1] = depth as u8;
        lit[256] = depth as u8;
        let mut wire = Wire::default();
        lengths(&mut wire, &lit, &[0]);
        let codes = Codes::new(&lit);
        for n in 0..depth {
            codes.symbol(&mut wire, n);
        }
        codes.symbol(&mut wire, 256);
        cases.push(valid(
            format!("literal-depth/{depth}"),
            wire,
            (0..depth).map(|n| n as u8).collect(),
            false,
        ));

        let mut raw: Vec<u8> = (0..32768).map(|n| ((n * 71) ^ (n / 13)) as u8).collect();
        let mut wire = Wire::default();
        wire.stored(&raw, false);
        let mut lit = vec![0; 258];
        lit[256] = 1;
        lit[257] = 1;
        let mut dist = vec![0; 30];
        for n in 1..depth {
            dist[n - 1] = n as u8;
        }
        dist[28] = depth as u8;
        dist[29] = depth as u8;
        lengths(&mut wire, &lit, &dist);
        let (lc, dc) = (Codes::new(&lit), Codes::new(&dist));
        for (code, &length) in dist.iter().enumerate() {
            if length == 0 {
                continue;
            }
            for extra in [0, (1usize << DIST_EXTRA[code]) - 1] {
                matched(&mut wire, &lc, &dc, &mut raw, 3, DIST_BASE[code] + extra);
            }
        }
        lc.symbol(&mut wire, 256);
        cases.push(valid(format!("distance-depth/{depth}"), wire, raw, false));
    }
    for leaves in 2usize..=286 {
        let depth = leaves.ilog2() as u8;
        let short = (1usize << (depth + 1)) - leaves;
        let mut lit = vec![0; 286];
        let mut symbols: Vec<_> = (0..286).filter(|&n| n != 256).take(leaves - 1).collect();
        symbols.push(256);
        for (n, &symbol) in symbols.iter().enumerate() {
            lit[symbol] = if n < short { depth } else { depth + 1 };
        }
        let mut wire = Wire::default();
        lengths(&mut wire, &lit, &[0]);
        emit(&mut wire, &Codes::new(&lit), &[0, 0, 256]);
        cases.push(valid(
            format!("balanced-literals/{leaves}"),
            wire,
            vec![0, 0],
            false,
        ));
    }
    for dynamic in [false, true] {
        for offset in 0..8 {
            for ones in [false, true] {
                let mut raw: Vec<u8> = (0..32768).map(|n| ((n * 71) ^ (n / 13)) as u8).collect();
                let mut wire = Wire::default();
                wire.stored(&raw, false);
                wire.put(2, 3);
                let fixed = fixed_codes();
                fixed.symbol(&mut wire, 0);
                raw.push(0);
                for _ in 0..offset {
                    fixed.symbol(&mut wire, 200);
                }
                raw.extend(std::iter::repeat_n(200, offset));
                fixed.symbol(&mut wire, 256);
                let (lit, dist) = if dynamic {
                    let mut l = vec![0; 286];
                    l[256] = 4;
                    l[257] = 4;
                    l[258..].fill(5);
                    let mut d = vec![5; 30];
                    d[..2].fill(4);
                    lengths(&mut wire, &l, &d);
                    (Codes::new(&l), Codes::new(&d))
                } else {
                    wire.put(3, 3);
                    (fixed_codes(), Codes::new(&[5; 32]))
                };
                for distance in 1..=32768 {
                    matched(&mut wire, &lit, &dist, &mut raw, 3, distance);
                }
                for length in 3..=258 {
                    for distance in [1, 2, 3, 4, 7, 257, 32767, 32768] {
                        matched(&mut wire, &lit, &dist, &mut raw, length, distance);
                    }
                }
                lit.symbol(&mut wire, 256);
                cases.push(Case {
                    name: format!("all-extra-values/{dynamic}/{offset}/{ones}"),
                    bytes: wire.finish(&raw, ones),
                    output: Some(raw),
                    extended_distance: false,
                });
            }
        }
    }
    cases
}
