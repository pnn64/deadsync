//! Internal byte hashing for string and constant indices.

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    const A: u64 = 0x9e3779b185ebca87;
    const B: u64 = 0xc2b2ae3d27d4eb4f;
    const INITIAL_LANES: [u64; 4] = [A, B, !A, !B];
    const fn mix_lanes([a, b, c, d]: [u64; 4]) -> u64 {
        a.wrapping_add(b.rotate_left(13)) ^ c.rotate_left(31).wrapping_add(d.rotate_left(47))
    }
    fn word(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes[..8].try_into().unwrap())
    }
    fn mix_word(hash: u64, word: u64) -> u64 {
        (hash ^ word).rotate_left(27).wrapping_mul(B)
    }
    fn tail(bytes: &[u8]) -> u64 {
        let n = bytes.len();
        if n >= 4 {
            u64::from(u32::from_le_bytes(bytes[..4].try_into().unwrap()))
                | (u64::from(u32::from_le_bytes(bytes[n - 4..].try_into().unwrap())) << 32)
        } else if n != 0 {
            u64::from(bytes[0]) | (u64::from(bytes[n / 2]) << 8) | (u64::from(bytes[n - 1]) << 16)
        } else {
            0
        }
    }
    let mut h = (bytes.len() as u64).wrapping_mul(A) ^ B;
    if bytes.len() <= 16 {
        h ^= if bytes.len() >= 8 {
            word(bytes).wrapping_mul(A)
                ^ word(&bytes[bytes.len() - 8..])
                    .rotate_left(27)
                    .wrapping_mul(B)
        } else {
            tail(bytes).wrapping_mul(B)
        };
    } else if bytes.len() < 32 {
        // No full block precedes these two or three trailing words.
        h ^= const { mix_lanes(INITIAL_LANES) };
        h = mix_word(mix_word(h, word(bytes)), word(&bytes[8..]));
        let rest = if bytes.len() >= 24 {
            h = mix_word(h, word(&bytes[16..]));
            &bytes[24..]
        } else {
            &bytes[16..]
        };
        h ^= tail(rest).wrapping_mul(A);
    } else {
        // Four independent lanes consume every byte; equality still checks
        // the complete slice. Loads copy bytes safely, including unaligned input.
        let mut lanes = INITIAL_LANES;
        let (blocks, rest) = bytes.as_chunks::<32>();
        for block in blocks {
            for (lane, bytes) in lanes.iter_mut().zip(block.as_chunks::<8>().0) {
                *lane = (*lane ^ u64::from_le_bytes(*bytes))
                    .rotate_left(29)
                    .wrapping_mul(A);
            }
        }
        h ^= mix_lanes(lanes);
        let (words, rest) = rest.as_chunks::<8>();
        for bytes in words {
            h = mix_word(h, u64::from_le_bytes(*bytes));
        }
        h ^= tail(rest).wrapping_mul(A);
    }
    h ^= h >> 29;
    h = h.wrapping_mul(0x165667b19e3779f9);
    h ^ (h >> 31)
}
