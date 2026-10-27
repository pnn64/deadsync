//! Vorbis section 3: entry-order Huffman assignment and lookup types 0/1/2.
use crate::{
    Budget, Error, Limits, ParseOptions,
    bitpack::{BitReader, ilog},
    reserve,
};

const LEAF: u32 = 1 << 31;
const MISSING: u32 = u32::MAX;

/// Flat decode tree with an optional bounded prefix table in the same buffer.
/// Single-entry books consume one bit (both values accepted), as required by
/// the 2015 specification erratum.
pub struct Huffman {
    nodes: Vec<[u32; 2]>,
    singleton: u32,
    prefix_bits: u8,
}
impl Huffman {
    /// Zero lengths indicate unused entries. All other lengths are 1..=32.
    pub fn from_lengths(lengths: &[u8], max_nodes: usize) -> Result<Self, Error> {
        let mut budget = Budget {
            remaining: usize::MAX,
        };
        let mut tree = Self::build(lengths, max_nodes, &mut budget, false)?;
        tree.accelerate(lengths, &mut budget)?;
        Ok(tree)
    }
    fn build(
        lengths: &[u8],
        max_nodes: usize,
        budget: &mut Budget,
        allow_empty: bool,
    ) -> Result<Self, Error> {
        if lengths.len() >= LEAF as usize {
            return Err(Error::LimitExceeded("Huffman entries"));
        }
        let used = lengths.iter().filter(|&&n| n != 0).count();
        if used == 0 {
            if allow_empty {
                return Ok(Self {
                    nodes: Vec::new(),
                    singleton: MISSING,
                    prefix_bits: 0,
                });
            }
            return Err(Error::Invalid("empty Huffman tree"));
        }
        if used == 1 {
            let entry = lengths.iter().position(|&n| n != 0).unwrap();
            if lengths[entry] != 1 {
                return Err(Error::Invalid("single-entry codeword length must be one"));
            }
            return Ok(Self {
                nodes: Vec::new(),
                singleton: entry as u32,
                prefix_bits: 0,
            });
        }
        let mut nodes = Vec::new();
        add_node(&mut nodes, max_nodes, budget)?;
        // Free prefixes, represented by their MSB-aligned numeric left edge.
        // For each entry choose the lowest usable prefix, then split left;
        // this follows entry order, not canonical length-sorted assignment.
        let mut free = [None; 33];
        free[0] = Some(0u32);
        for (entry, &length) in lengths.iter().enumerate() {
            if length == 0 {
                continue;
            }
            if length > 32 {
                return Err(Error::Invalid("Huffman codeword length"));
            }
            let length = length as usize;
            let (depth, code) = free[..=length]
                .iter()
                .enumerate()
                .filter_map(|(depth, &code)| code.map(|code| (depth, code)))
                .min_by_key(|&(_, code)| code)
                .ok_or(Error::Invalid("overspecified Huffman tree"))?;
            free[depth] = None;
            for (d, slot) in free.iter_mut().enumerate().take(length + 1).skip(depth + 1) {
                if slot.replace(code | (1u32 << (32 - d))).is_some() {
                    return Err(Error::Invalid("Huffman prefix collision"));
                }
            }
            let mut node = 0usize;
            for d in 0..length {
                let branch = ((code >> (31 - d)) & 1) as usize;
                if d + 1 == length {
                    if nodes[node][branch] != MISSING {
                        return Err(Error::Invalid("Huffman leaf collision"));
                    }
                    nodes[node][branch] = LEAF | entry as u32;
                } else {
                    if nodes[node][branch] == MISSING {
                        let next = add_node(&mut nodes, max_nodes, budget)?;
                        nodes[node][branch] = next;
                    }
                    if nodes[node][branch] & LEAF != 0 {
                        return Err(Error::Invalid("Huffman prefix collision"));
                    }
                    node = nodes[node][branch] as usize;
                }
            }
        }
        if free.iter().any(Option::is_some) {
            return Err(Error::Invalid("underspecified Huffman tree"));
        }
        Ok(Self {
            nodes,
            singleton: MISSING,
            prefix_bits: 0,
        })
    }
    fn accelerate(&mut self, lengths: &[u8], budget: &mut Budget) -> Result<(), Error> {
        // Four low tag bits hold a short leaf's consumed length; zero tags a
        // continuation node. Large public from_lengths inputs keep the tree
        // path rather than imposing an additional format/entry limit.
        if self.singleton != MISSING
            || self.prefix_bits != 0
            || lengths.len() > 1 << 28
            || self.nodes.len() > 1 << 28
        {
            return Ok(());
        }
        let maximum = lengths.iter().copied().max().unwrap_or(0);
        if maximum <= 2 {
            return Ok(());
        }
        let used = lengths.iter().filter(|&&length| length != 0).count();
        let mut width = maximum.min(8).min(ilog(used as u32) as u8);
        while width != 0 && (4usize << width) > budget.remaining {
            width -= 1;
        }
        if width < 2 {
            return Ok(());
        }
        let pairs = 1usize << (width - 1);
        let start = self.nodes.len();
        if self.nodes.try_reserve_exact(pairs).is_err() {
            return Ok(());
        }
        budget.charge(pairs * 8)?;
        self.nodes.resize(start + pairs, [MISSING; 2]);
        fill_prefix(&mut self.nodes, 0, 0, 0, width, start);
        self.prefix_bits = width;
        Ok(())
    }
    #[inline(always)]
    pub fn decode(&self, bits: &mut BitReader<'_>) -> Result<u32, Error> {
        if self.singleton != MISSING {
            bits.read(1)?;
            return Ok(self.singleton);
        }
        if self.nodes.is_empty() {
            return Err(Error::EndOfPacket);
        }
        let mut node = 0usize;
        let width = self.prefix_bits as u32;
        if width != 0
            && let Some(word) = bits.peek(width)
        {
            let start = self.nodes.len() - (1usize << (width - 1));
            let entry = self.nodes[start + word as usize / 2][word as usize & 1];
            if entry != MISSING {
                let consumed = entry & 15;
                if consumed != 0 {
                    bits.consume(consumed);
                    return Ok(entry >> 4);
                }
                bits.consume(width);
                node = (entry >> 4) as usize;
            }
        }
        self.decode_from(bits, node)
    }
    #[inline(never)]
    fn decode_from(&self, bits: &mut BitReader<'_>, mut node: usize) -> Result<u32, Error> {
        loop {
            let next = self.nodes[node][bits.read(1)? as usize];
            if next == MISSING {
                return Err(Error::Invalid("unused Huffman prefix"));
            }
            if next & LEAF != 0 {
                return Ok(next & !LEAF);
            }
            node = next as usize;
        }
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
            - if self.prefix_bits == 0 {
                0
            } else {
                1usize << (self.prefix_bits - 1)
            }
    }
    /// Empty books are available only through the explicit parsing policy.
    /// Their scalar reads fail without consuming or exhausting the bit reader.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.singleton == MISSING
    }
    fn storage_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<[u32; 2]>()
    }
}
// Traverse only the first eight levels. Short leaves fill all suffix aliases;
// each table slot is written once, without walking every prefix from the root.
fn fill_prefix(
    nodes: &mut [[u32; 2]],
    node: usize,
    depth: u8,
    prefix: usize,
    width: u8,
    start: usize,
) {
    for branch in 0..2 {
        let next = nodes[node][branch];
        if next == MISSING {
            continue;
        }
        let word = prefix | branch << depth;
        let bits = depth + 1;
        if next & LEAF != 0 {
            let entry = ((next & !LEAF) << 4) | bits as u32;
            for word in (word..1usize << width).step_by(1usize << bits) {
                nodes[start + word / 2][word & 1] = entry;
            }
        } else if bits == width {
            nodes[start + word / 2][word & 1] = next << 4;
        } else {
            fill_prefix(nodes, next as usize, bits, word, width, start);
        }
    }
}
fn add_node(nodes: &mut Vec<[u32; 2]>, limit: usize, budget: &mut Budget) -> Result<u32, Error> {
    if nodes.len() >= limit || nodes.len() >= LEAF as usize {
        return Err(Error::LimitExceeded("Huffman nodes"));
    }
    budget.charge(8)?;
    reserve(nodes, 1)?;
    let index = nodes.len() as u32;
    nodes.push([MISSING; 2]);
    Ok(index)
}

pub struct Codebook {
    dimensions: u16,
    lengths: Vec<u8>,
    huffman: Huffman,
    lookup_type: u8,
    vectors: Vec<f32>,
}
impl Codebook {
    pub fn storage_bytes(&self) -> usize {
        self.lengths.capacity() + self.vectors.capacity() * 4 + self.huffman.storage_bytes()
    }
    pub fn dimensions(&self) -> usize {
        self.dimensions as usize
    }
    pub fn entries(&self) -> usize {
        self.lengths.len()
    }
    pub fn lengths(&self) -> &[u8] {
        &self.lengths
    }
    pub fn lookup_type(&self) -> u8 {
        self.lookup_type
    }
    pub fn huffman(&self) -> &Huffman {
        &self.huffman
    }
    pub fn is_empty(&self) -> bool {
        self.huffman.is_empty()
    }
    pub fn read(bits: &mut BitReader<'_>, limits: &Limits) -> Result<Self, Error> {
        Self::read_with_options(
            bits,
            ParseOptions {
                limits: *limits,
                ..ParseOptions::default()
            },
        )
    }
    pub fn read_with_options(
        bits: &mut BitReader<'_>,
        options: ParseOptions,
    ) -> Result<Self, Error> {
        let limits = &options.limits;
        let mut budget = Budget::new(limits);
        let mut book =
            Self::read_with_budget(bits, limits, &mut budget, options.allow_empty_codebooks)?;
        book.accelerate(&mut budget)?;
        Ok(book)
    }
    pub(crate) fn accelerate(&mut self, budget: &mut Budget) -> Result<(), Error> {
        self.huffman.accelerate(&self.lengths, budget)
    }
    pub(crate) fn read_with_budget(
        bits: &mut BitReader<'_>,
        limits: &Limits,
        budget: &mut Budget,
        allow_empty: bool,
    ) -> Result<Self, Error> {
        if bits.read(24)? != 0x564342 {
            return Err(Error::Invalid("codebook sync"));
        }
        let dimensions = bits.read(16)? as u16;
        let entries = bits.read(24)? as usize;
        if ilog(dimensions as u32) + ilog(entries as u32) > limits.max_codebook_extent_bits as u32 {
            return Err(Error::LimitExceeded("codebook extent"));
        }
        if entries == 0 && !allow_empty {
            return Err(Error::Invalid("zero codebook entries"));
        }
        if entries > limits.max_codebook_entries {
            return Err(Error::LimitExceeded("codebook entries"));
        }
        budget.charge(entries)?;
        let mut lengths = Vec::new();
        reserve(&mut lengths, entries)?;
        lengths.resize(entries, 0);
        if bits.flag()? {
            let mut length = bits.read(5)? + 1;
            let mut position = 0;
            while position < entries {
                if length > 32 {
                    return Err(Error::Invalid("ordered codeword length exceeds 32"));
                }
                let count = bits.read(ilog((entries - position) as u32))? as usize;
                if count > entries - position {
                    return Err(Error::Invalid("ordered codeword run"));
                }
                lengths[position..position + count].fill(length as u8);
                position += count;
                length += 1;
            }
        } else {
            let sparse = bits.flag()?;
            for length in &mut lengths {
                if !sparse || bits.flag()? {
                    *length = (bits.read(5)? + 1) as u8;
                }
            }
        }
        let huffman = Huffman::build(&lengths, limits.max_huffman_nodes, budget, allow_empty)?;
        let lookup_type = bits.read(4)? as u8;
        let mut vectors = Vec::new();
        if lookup_type != 0 {
            if lookup_type > 2 {
                return Err(Error::Invalid("codebook lookup type"));
            }
            if dimensions == 0 && lookup_type == 1 && !huffman.is_empty() {
                return Err(Error::Invalid("zero-dimensional lookup-one codebook"));
            }
            let minimum = float32_unpack(bits.read(32)?);
            let delta = float32_unpack(bits.read(32)?);
            let value_bits = bits.read(4)? + 1;
            let sequence = bits.flag()?;
            let value_count = entries
                .checked_mul(dimensions as usize)
                .ok_or(Error::LimitExceeded("codebook values"))?;
            if value_count > limits.max_codebook_values {
                return Err(Error::LimitExceeded("codebook values"));
            }
            let lookup_count = if lookup_type == 1 {
                lookup1_values(entries as u32, dimensions as u32) as usize
            } else {
                value_count
            };
            // Check packet availability before allocating large lookup tables.
            if lookup_count > bits.remaining() / value_bits as usize {
                return Err(Error::EndOfPacket);
            }
            if huffman.is_empty() {
                // Packed lookup values are still present, but no vector can
                // be returned. The reference skips decoded-vector creation.
                for _ in 0..lookup_count {
                    bits.read(value_bits)?;
                }
            } else {
                budget.charge(
                    lookup_count
                        .checked_mul(2)
                        .ok_or(Error::LimitExceeded("setup memory"))?,
                )?;
                let mut multiplicands = Vec::new();
                reserve(&mut multiplicands, lookup_count)?;
                for _ in 0..lookup_count {
                    multiplicands.push(bits.read(value_bits)? as u16);
                }
                budget.charge(
                    value_count
                        .checked_mul(4)
                        .ok_or(Error::LimitExceeded("setup memory"))?,
                )?;
                reserve(&mut vectors, value_count)?;
                for entry in 0..entries {
                    let mut last = 0.0;
                    let mut quotient = entry;
                    for dimension in 0..dimensions as usize {
                        let offset = if lookup_type == 1 {
                            let offset = quotient % lookup_count;
                            quotient /= lookup_count;
                            offset
                        } else {
                            entry * dimensions as usize + dimension
                        };
                        let value = multiplicands[offset] as f64 * delta + minimum + last;
                        vectors.push(value as f32);
                        if sequence {
                            last = (value as f32) as f64;
                        }
                    }
                }
            }
        }
        Ok(Self {
            dimensions,
            lengths,
            huffman,
            lookup_type,
            vectors,
        })
    }
    #[inline]
    pub fn decode_scalar(&self, bits: &mut BitReader<'_>) -> Result<u32, Error> {
        self.huffman.decode(bits)
    }
    pub fn vector(&self, entry: u32) -> Result<&[f32], Error> {
        if self.lookup_type == 0 {
            return Err(Error::Invalid("VQ decode requires a lookup"));
        }
        if entry as usize >= self.entries() || self.lengths[entry as usize] == 0 {
            return Err(Error::Invalid("unused codebook entry"));
        }
        let start = entry as usize * self.dimensions();
        Ok(&self.vectors[start..start + self.dimensions()])
    }
    #[inline(always)]
    pub fn decode_vector(&self, bits: &mut BitReader<'_>) -> Result<&[f32], Error> {
        if self.lookup_type == 0 {
            return Err(Error::Invalid("VQ decode requires a lookup"));
        }
        self.vector(self.decode_scalar(bits)?)
    }
}

/// Vorbis's custom float representation (not IEEE binary32).
pub fn float32_unpack(value: u32) -> f64 {
    let mantissa = (value & 0x1fffff) as f64;
    let signed = if value & 0x80000000 != 0 {
        -mantissa
    } else {
        mantissa
    };
    signed * 2.0f64.powi(((value >> 21) & 0x3ff) as i32 - 788)
}
/// Exact integer root, avoiding floating-point rounding around perfect powers.
pub fn lookup1_values(entries: u32, dimensions: u32) -> u32 {
    if entries == 0 || dimensions == 0 {
        return 0;
    }
    if dimensions == 1 {
        return entries;
    }
    // entries <= 2^24-1 in a packed book; this also handles all public u32 input.
    let mut low = 1;
    let mut high = entries;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if power_at_most(mid, dimensions, entries) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}
pub(crate) fn power_at_most(base: u32, exponent: u32, limit: u32) -> bool {
    if exponent == 0 || base == 1 {
        return limit >= 1;
    }
    let mut product = 1u64;
    for _ in 0..exponent {
        product *= base as u64;
        if product > limit as u64 {
            return false;
        }
    }
    true
}

#[cfg(test)]
pub(crate) fn empty_test_book(dimensions: u16, lookup: u8) -> Codebook {
    use crate::bitpack::BitWriter;
    let mut bits = BitWriter::new();
    for (value, width) in [
        (0x564342, 24),
        (dimensions as u32, 16),
        (0, 24),
        (0, 1),
        (1, 1),
        (lookup as u32, 4),
    ] {
        bits.write(value, width).unwrap();
    }
    if lookup != 0 {
        for (value, width) in [(0, 32), (0, 32), (0, 4), (0, 1)] {
            bits.write(value, width).unwrap();
        }
    }
    Codebook::read_with_options(
        &mut BitReader::new(&bits.finish().unwrap()),
        ParseOptions {
            allow_empty_codebooks: true,
            ..ParseOptions::default()
        },
    )
    .unwrap()
}

#[cfg(test)]
mod prefix_tests {
    use super::*;
    #[test]
    fn cardinality_bound_matches_checked_integer_powers_including_zero_entries() {
        for base in [0u32, 1, 2, 3, 64, u32::MAX] {
            for exponent in [0, 1, 2, 3, 16, 65_535] {
                for limit in [0, 1, 2, 64, u32::MAX] {
                    assert_eq!(
                        power_at_most(base, exponent, limit),
                        base.checked_pow(exponent).is_some_and(|n| n <= limit),
                        "{base}^{exponent} <= {limit}"
                    );
                }
            }
        }
    }
    #[test]
    fn lookup_matches_tree_at_every_prefix_alignment_and_packet_tail() {
        let mut deep: Vec<_> = (1..=32).collect();
        deep.push(32);
        for lengths in [vec![2, 4, 4, 4, 4, 2, 3, 3], vec![8; 256], deep] {
            let fast = Huffman::from_lengths(&lengths, 4096).unwrap();
            let slow = Huffman::build(
                &lengths,
                4096,
                &mut Budget {
                    remaining: usize::MAX,
                },
                false,
            )
            .unwrap();
            assert!(fast.prefix_bits != 0);
            assert_eq!(fast.node_count(), slow.node_count());
            for first in 0..=255u8 {
                for tail in [&[][..], &[0][..], &[255][..], &[255, 255, 255][..]] {
                    let mut input = vec![first];
                    input.extend_from_slice(tail);
                    for offset in 0..8 {
                        let mut a = BitReader::new(&input);
                        let mut b = a.clone();
                        a.read(offset).unwrap();
                        b.read(offset).unwrap();
                        for _ in 0..16 {
                            let actual = fast.decode(&mut a);
                            let expected = slow.decode(&mut b);
                            assert_eq!(actual, expected);
                            assert_eq!(
                                (a.position(), a.remaining(), a.is_exhausted()),
                                (b.position(), b.remaining(), b.is_exhausted())
                            );
                            if actual.is_err() {
                                assert_eq!(a.read(0), b.read(0));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn prefix_memory_is_optional_and_logical_node_limits_are_unchanged() {
        let lengths = [3; 8];
        let mut budget = Budget { remaining: 7 * 8 };
        let mut tree = Huffman::build(&lengths, 7, &mut budget, false).unwrap();
        assert_eq!(budget.remaining, 0);
        tree.accelerate(&lengths, &mut budget).unwrap();
        assert_eq!(tree.prefix_bits, 0);
        assert_eq!(tree.node_count(), 7);
        let fast = Huffman::from_lengths(&lengths, 7).unwrap();
        assert_eq!(fast.node_count(), 7);
        assert!(fast.prefix_bits > 0);
        assert!(matches!(
            Huffman::from_lengths(&lengths, 6),
            Err(Error::LimitExceeded("Huffman nodes"))
        ));
        for (tree, input) in [(&tree, [0xff]), (&fast, [0xff])] {
            assert_eq!(tree.decode(&mut BitReader::new(&input)), Ok(7));
        }
    }
}
