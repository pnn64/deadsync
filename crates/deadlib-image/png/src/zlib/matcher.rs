use crate::{Error, reserve};

pub(super) const WINDOW: usize = 32768;
pub(super) const SEARCH: usize = 16;
const HASHES: usize = 1 << 16;

/// Spend deeper searches only after the current block shows useful repetition.
#[inline]
pub(super) fn searches(bits: usize, consumed: usize) -> usize {
    if consumed >= 256 && bits <= consumed * 4 {
        SEARCH
    } else {
        1
    }
}

/// Flat four-byte hash heads and a 32 KiB ring of backward links. Low 32-bit
/// positions wrap independently of stream length; candidates are bounded by
/// distance and their bytes are always verified before emitting a match.
/// Fixed table sizes let the hash and ring masks bound every index.
pub(super) struct Matcher {
    heads: Box<[u32; HASHES]>,
    previous: Box<[u16; WINDOW]>,
}
impl Matcher {
    pub(super) fn new() -> Result<Self, Error> {
        Ok(Self {
            heads: table(u32::MAX)?,
            previous: table(0)?,
        })
    }
    pub(super) fn memory(&self) -> usize {
        HASHES * 4 + WINDOW * 2
    }
    #[inline]
    pub(super) fn insert(&mut self, bytes: &[u8], position: usize) -> Option<usize> {
        self.insert_word(u32::from_le_bytes(bytes[..4].try_into().unwrap()), position)
    }
    /// Make `position` the head of the chain for `word`, its four bytes.
    #[inline(always)]
    fn insert_word(&mut self, word: u32, position: usize) -> Option<usize> {
        let hash = hash(word);
        let head = self.heads[hash];
        self.heads[hash] = position as u32;
        let distance = (position as u32).wrapping_sub(head) as usize;
        let candidate = if head != u32::MAX && (1..=WINDOW).contains(&distance) {
            position.checked_sub(distance)
        } else {
            None
        };
        self.previous[position & (WINDOW - 1)] = if candidate.is_some() {
            distance as u16
        } else {
            0
        };
        candidate
    }
    /// Hash a match's interior: every fourth position, then each of the last
    /// eight, which the next search may reach. `data[0]` is position `base`;
    /// only positions with four bytes of lookahead in `data` are hashed.
    #[inline(always)]
    pub(super) fn insert_match(&mut self, data: &[u8], base: usize, start: usize, length: usize) {
        let end = start + length;
        debug_assert!(length >= 3 && end <= data.len());
        let stop = end.min(data.len().saturating_sub(3));
        let tail = end.saturating_sub(8).max(start + 1);
        // Each position in the stride starts one four-byte chunk of this slice,
        // so the words need no individual bounds checks.
        let strided = tail.min(stop);
        let from = (start + 1).min(strided);
        let words = data[from..strided + 3].as_chunks::<4>().0;
        let mut k = 0;
        if let Some(first) = words.first() {
            // While later words repeat the first, each chain head is the
            // previous insert, so only links are stored and the head is set
            // once. A link to a position that reads as an empty head is zero.
            self.insert_word(u32::from_le_bytes(*first), base + from);
            let mut last = base + from;
            k = 1;
            while let Some(word) = words.get(k)
                && word == first
            {
                let position = last + 4;
                self.previous[position & (WINDOW - 1)] =
                    if last as u32 != u32::MAX { 4 } else { 0 };
                last = position;
                k += 1;
            }
            self.heads[hash(u32::from_le_bytes(*first))] = last as u32;
        }
        for (k, word) in words.iter().enumerate().skip(k) {
            self.insert_word(u32::from_le_bytes(*word), base + from + 4 * k);
        }
        for next in tail..stop {
            self.insert(&data[next..], base + next);
        }
    }
    #[inline]
    pub(super) fn next(&self, candidate: usize, position: usize) -> Option<usize> {
        let distance = self.previous[candidate & (WINDOW - 1)] as usize;
        if distance == 0 {
            return None;
        }
        candidate
            .checked_sub(distance)
            .filter(|&next| position - next <= WINDOW)
    }
}

fn hash(word: u32) -> usize {
    (word.wrapping_mul(0x1e35_a7bd) >> 16) as usize
}

fn table<T: Copy, const N: usize>(value: T) -> Result<Box<[T; N]>, Error> {
    let mut table = Vec::new();
    reserve(&mut table, N)?;
    table.resize(N, value);
    let Ok(table) = table.into_boxed_slice().try_into() else {
        unreachable!("the table has exactly N entries")
    };
    Ok(table)
}

/// Equal-prefix length in portable eight-byte groups. Little-endian words make
/// trailing-zero counts identify the first differing byte on every host.
#[inline(always)]
pub(super) fn prefix(a: &[u8], b: &[u8]) -> usize {
    debug_assert_eq!(a.len(), b.len());
    let mut equal = 0;
    for (a, b) in a.as_chunks::<8>().0.iter().zip(b.as_chunks::<8>().0) {
        let different = u64::from_le_bytes(*a) ^ u64::from_le_bytes(*b);
        if different != 0 {
            return equal + different.trailing_zeros() as usize / 8;
        }
        equal += 8;
    }
    for (&a, &b) in a[equal..].iter().zip(&b[equal..]) {
        if a != b {
            break;
        }
        equal += 1;
    }
    equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_prefix_mismatch_and_tail() {
        for size in 0..=258 {
            let a: Vec<_> = (0..size).map(|n| (n * 71) as u8).collect();
            assert_eq!(prefix(&a, &a), size);
            for mismatch in 0..size {
                let mut b = a.clone();
                b[mismatch] ^= 0x81;
                assert_eq!(prefix(&a, &b), mismatch);
            }
        }
    }

    #[test]
    fn bounded_history_chains_and_wrapping_head_positions() {
        let mut bases = vec![0];
        if let Some(wrapped) = (u32::MAX as usize).checked_add(1) {
            bases.extend([wrapped - 65, wrapped]);
        }
        for base in bases {
            let mut matcher = Matcher::new().unwrap();
            let bytes = [1, 2, 3, 4];
            assert_eq!(matcher.insert(&bytes, base), None);
            assert_eq!(matcher.insert(&bytes, base + 32), Some(base));
            assert_eq!(matcher.insert(&bytes, base + WINDOW), Some(base + 32));
            assert_eq!(matcher.next(base + 32, base + WINDOW), Some(base));
            assert_eq!(matcher.next(base, base + WINDOW), None);
            assert_eq!(matcher.insert(&bytes, base + WINDOW * 2 + 1), None);
            assert_eq!(matcher.memory(), 65536 * 4 + WINDOW * 2);
        }
    }
}
