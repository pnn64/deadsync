//! Vorbis section 3: entry-order Huffman assignment and lookup types 0/1/2.
use crate::{
    Budget, Error, Limits,
    bitpack::{BitReader, ilog},
    reserve,
};

const LEAF: u32 = 1 << 31;
const MISSING: u32 = u32::MAX;

/// Flat decode tree. Single-entry books consume one bit (both values accepted),
/// as required by the 2015 specification erratum.
pub struct Huffman {
    nodes: Vec<[u32; 2]>,
    singleton: Option<u32>,
}
impl Huffman {
    /// Zero lengths indicate unused entries. All other lengths are 1..=32.
    pub fn from_lengths(lengths: &[u8], max_nodes: usize) -> Result<Self, Error> {
        Self::build(
            lengths,
            max_nodes,
            &mut Budget {
                remaining: usize::MAX,
            },
        )
    }
    fn build(lengths: &[u8], max_nodes: usize, budget: &mut Budget) -> Result<Self, Error> {
        if lengths.len() >= LEAF as usize {
            return Err(Error::LimitExceeded("Huffman entries"));
        }
        let used = lengths.iter().filter(|&&n| n != 0).count();
        if used == 0 {
            return Err(Error::Invalid("empty Huffman tree"));
        }
        if used == 1 {
            let entry = lengths.iter().position(|&n| n != 0).unwrap();
            if lengths[entry] != 1 {
                return Err(Error::Invalid("single-entry codeword length must be one"));
            }
            return Ok(Self {
                nodes: Vec::new(),
                singleton: Some(entry as u32),
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
            singleton: None,
        })
    }
    #[inline]
    pub fn decode(&self, bits: &mut BitReader<'_>) -> Result<u32, Error> {
        if let Some(entry) = self.singleton {
            bits.read(1)?;
            return Ok(entry);
        }
        let mut node = 0usize;
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
    }
    fn storage_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<[u32; 2]>()
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
    pub fn read(bits: &mut BitReader<'_>, limits: &Limits) -> Result<Self, Error> {
        Self::read_with_budget(bits, limits, &mut Budget::new(limits))
    }
    pub(crate) fn read_with_budget(
        bits: &mut BitReader<'_>,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Result<Self, Error> {
        if bits.read(24)? != 0x564342 {
            return Err(Error::Invalid("codebook sync"));
        }
        let dimensions = bits.read(16)? as u16;
        let entries = bits.read(24)? as usize;
        if dimensions == 0 || entries == 0 {
            return Err(Error::Invalid("zero codebook dimensions/entries"));
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
        let huffman = Huffman::build(&lengths, limits.max_huffman_nodes, budget)?;
        let lookup_type = bits.read(4)? as u8;
        let mut vectors = Vec::new();
        if lookup_type != 0 {
            if lookup_type > 2 {
                return Err(Error::Invalid("codebook lookup type"));
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
                        last = value;
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
    if base == 1 {
        return true;
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
