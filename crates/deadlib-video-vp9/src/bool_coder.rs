//! VP9's bool decoder (arithmetic decoder, spec §9.2 "Parsing process for Boolean decoder").
//!
//! Nearly all of the VP9 bitstream except the uncompressed frame header (the
//! compressed header and tile data) is entropy-coded by this bool coder. This
//! implementation follows the spec's arithmetic verbatim while keeping a 64-bit
//! shift register of prefetched bits and using single-step `leading_zeros()`
//! renormalization instead of bit-by-bit loops.
//!
//! Spec text referenced (§9.2.1 - §9.2.4):
//!
//! ```text
//! 9.2.1 Initialization process for Boolean decoder
//!   BoolValue = f(8)
//!   BoolRange = 255
//!   BoolMaxBits = 8 * sz - 8
//!   read a marker with read_bool(128), requiring it to be 0
//!
//! 9.2.2 Boolean decoding process (read_bool(p))
//!   split = 1 + (((BoolRange - 1) * p) >> 8)
//!   if BoolValue < split:
//!       BoolRange = split; bool = 0
//!   else:
//!       BoolRange -= split; BoolValue -= split; bool = 1
//!   while BoolRange < 128:
//!       newBit = (BoolMaxBits > 0) ? f(1) : 0   (if read, BoolMaxBits -= 1)
//!       BoolRange *= 2
//!       BoolValue = (BoolValue << 1) + newBit
//!
//! 9.2.3 Exit process for Boolean decoder (exit_bool())
//!   discard the remaining BoolMaxBits worth of padding (expected to be 0)
//!
//! 9.2.4 Parsing process for read_literal(n)
//!   x = 0
//!   for i in 0..n: x = 2 * x + read_bool(128)
//! ```

use core::fmt;

/// Errors that can occur when initializing the bool decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolCoderError {
    /// `init_bool(sz)` must not be called with sz < 1 (spec §9.2.1).
    EmptyBuffer,
    /// The marker bit read right after initialization must be 0 (spec §9.2.1).
    InvalidMarker,
}

impl fmt::Display for BoolCoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoolCoderError::EmptyBuffer => {
                f.write_str("bool decoder initialized with empty buffer")
            }
            BoolCoderError::InvalidMarker => f.write_str("bool decoder marker bit is non-zero"),
        }
    }
}

impl core::error::Error for BoolCoderError {}

/// VP9's arithmetic decoder (bool decoder).
///
/// `data` is expected to be the `sz`-byte slice from `init_bool( sz )` (the
/// compressed header or tile data itself), passed through as-is.
#[derive(Debug, Clone)]
pub struct BoolDecoder<'a> {
    data: &'a [u8],
    /// Byte offset of the next raw byte to prefetch from `data`.
    byte_pos: usize,
    /// Top 8 bits (`value >> 56`) hold spec `BoolValue`; bits `[55 .. 56 - count]` hold
    /// `count` prefetched raw bits (zero-padded past EOF). All bits below `56 - count` are 0.
    value: u64,
    /// Spec `BoolRange` (always in `128..=255` at entry/exit of `read_bool`).
    range: u32,
    /// Number of valid prefetched bits below the top 8 bits of `value`.
    /// Invariant: `count >= 8` at entry and exit of `read_bool` (max renorm shift is 7, so
    /// shifting `value <<= shift` always draws valid prefetched bits).
    count: i32,
    /// Total virtual zero-padding bits added past `data.len() * 8` during refills.
    over_bits_loaded: usize,
    /// Set by `exit_bool()` so `bit_position()` reports the end of the buffer.
    exited: bool,
}

impl<'a> BoolDecoder<'a> {
    /// Corresponds to `init_bool( sz )`. `data.len()` is the spec's `sz`.
    pub fn new(data: &'a [u8]) -> Result<Self, BoolCoderError> {
        if data.is_empty() {
            return Err(BoolCoderError::EmptyBuffer);
        }
        let mut decoder = Self {
            data,
            byte_pos: 1,
            value: (data[0] as u64) << 56,
            range: 255,
            count: 0,
            over_bits_loaded: 0,
            exited: false,
        };
        decoder.refill();
        // In a spec-conformant stream, the marker bit read right after initialization is always 0.
        if decoder.read_bool(128) {
            return Err(BoolCoderError::InvalidMarker);
        }
        Ok(decoder)
    }

    /// Refills `self.value` with up to 48 new prefetched bits from `self.data` (or virtual
    /// zero-padding past EOF). Called when `self.count < 8`, restoring `self.count >= 48`.
    #[inline(never)]
    fn refill(&mut self) {
        // Common case: at least 8 bytes remain in `data`, so read 8 bytes big-endian and keep
        // the top 6 bytes (48 bits). Bits `63..56 - count` are occupied; shifting the 48-bit
        // value left by `8 - count` places it directly in bits `55 - count .. 8 - count`.
        if let Some(&bytes) = self
            .data
            .get(self.byte_pos..)
            .and_then(|rest| rest.first_chunk::<8>())
        {
            let word = u64::from_be_bytes(bytes) >> 16;
            self.value |= word << (8 - self.count);
            self.byte_pos += 6;
            self.count += 48;
            return;
        }
        while self.count <= 48 && self.byte_pos < self.data.len() {
            self.value |= (self.data[self.byte_pos] as u64) << (48 - self.count);
            self.byte_pos += 1;
            self.count += 8;
        }
        if self.count < 8 {
            // Past EOF: spec supplies infinite 0 bits; vacated low bits of `value` are already 0.
            self.over_bits_loaded += 48;
            self.count += 48;
        }
    }

    /// `read_bool( p )` (spec §9.2.2). Decodes 1 bool based on probability p (0..=255, denominator 256).
    #[inline]
    pub fn read_bool(&mut self, p: u8) -> bool {
        let split = 1 + (((self.range - 1) * p as u32) >> 8);
        self.read_with_split(split)
    }

    #[inline(always)]
    fn read_with_split(&mut self, split: u32) -> bool {
        let big_split = (split as u64) << 56;
        let bit = self.value >= big_split;
        let range = if bit { self.range - split } else { split };
        let value = if bit {
            self.value.wrapping_sub(big_split)
        } else {
            self.value
        };
        // `range` is in `1..=255`, so `(range as u8).leading_zeros()` is in `0..=7`
        // and equals the exact number of `while range < 128` doublings required by spec §9.2.2.
        let shift = (range as u8).leading_zeros();
        self.range = range << shift;
        self.value = value << shift;
        self.count -= shift as i32;
        if self.count < 8 {
            self.refill();
        }
        bit
    }

    /// `read_literal( n )` (spec §9.2.4). Reads n bits with probability 1/2 (p=128), high bit first.
    #[inline]
    pub fn read_literal(&mut self, n: u32) -> u32 {
        let mut x = 0u32;
        for _ in 0..n {
            x = (x << 1) | self.flag() as u32;
        }
        x
    }

    /// `read_literal( 1 ) == 1`, as a single-bit flag (spec §9.2.4). A convenience wrapper for
    /// the very common single-bit `L(1)` reads scattered through the compressed header.
    #[inline]
    pub fn flag(&mut self) -> bool {
        self.read_with_split((self.range + 1) >> 1)
    }

    /// Decoding process for tree-coded syntax elements (spec §9.3.3, "Tree decoding process").
    ///
    /// `tree` is the spec's tree array itself (leaves are non-positive values
    /// `-value`, internal nodes are non-negative values pointing to the next
    /// index). `prob_of(node)` is a closure, built by the caller, that returns
    /// the probability obtained from the probability selection process (spec
    /// §9.3.2) given a node number (`n >> 1`).
    ///
    /// ```text
    /// do {
    ///     n = T[ n + read_bool( P( n >> 1 ) ) ]
    /// } while ( n > 0 )
    /// return -n
    /// ```
    #[inline]
    pub fn read_tree<F>(&mut self, tree: &[i32], mut prob_of: F) -> i32
    where
        F: FnMut(usize) -> u8,
    {
        let mut n: i32 = 0;
        loop {
            let node = (n as usize) >> 1;
            let bit = self.read_bool(prob_of(node)) as i32;
            n = tree[(n + bit) as usize];
            if n <= 0 {
                break;
            }
        }
        -n
    }

    /// Total raw bits consumed so far (including the initial 8 bits of `data[0]` and any
    /// zero-padding bits pulled past EOF).
    #[inline]
    fn consumed_bits(&self) -> usize {
        self.byte_pos * 8 + self.over_bits_loaded - self.count as usize
    }

    /// Number of raw bits this decoder has requested past the end of its buffer. Conformant
    /// decodes end with only a small tail of these; a large value means the arithmetic decoder
    /// desynced and ran off the end -- i.e. the tile/header was corrupt.
    #[inline]
    pub fn over_read_bits(&self) -> usize {
        self.consumed_bits().saturating_sub(self.data.len() * 8)
    }

    /// Absolute raw-bit position (pinned at `data.len()*8` once the end is reached; further
    /// requests bump [`Self::over_read_bits`] instead). Lets a tile decode measure how much of
    /// its buffer it consumed, to reject a desynced (corrupt) tile that finishes far short.
    pub fn bit_position(&self) -> usize {
        if self.exited {
            self.data.len() * 8
        } else {
            self.consumed_bits().min(self.data.len() * 8)
        }
    }

    /// `exit_bool( )` (spec §9.2.3). Discards the remaining padding bits and finishes.
    ///
    /// The spec requires the padding to be 0, but this implementation does not
    /// validate the value and only marks the decoder as exited, so it does not
    /// panic on non-conformant streams.
    pub fn exit_bool(&mut self) {
        self.exited = true;
    }
}
