//! Internal byte hashing for string and constant indices.

pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    const A: u64 = 0x9e3779b185ebca87;
    const B: u64 = 0xc2b2ae3d27d4eb4f;
    fn word(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes[..8].try_into().unwrap())
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
    } else {
        // Four independent lanes consume every byte; equality still checks
        // the complete slice. Loads copy bytes safely, including unaligned input.
        let mut lanes = [A, B, !A, !B];
        let (blocks, rest) = bytes.as_chunks::<32>();
        for block in blocks {
            for (lane, bytes) in lanes.iter_mut().zip(block.as_chunks::<8>().0) {
                *lane = (*lane ^ u64::from_le_bytes(*bytes))
                    .rotate_left(29)
                    .wrapping_mul(A);
            }
        }
        h ^= lanes[0].wrapping_add(lanes[1].rotate_left(13))
            ^ lanes[2]
                .rotate_left(31)
                .wrapping_add(lanes[3].rotate_left(47));
        let (words, rest) = rest.as_chunks::<8>();
        for bytes in words {
            h = (h ^ u64::from_le_bytes(*bytes))
                .rotate_left(27)
                .wrapping_mul(B);
        }
        h ^= tail(rest).wrapping_mul(A);
    }
    h ^= h >> 29;
    h = h.wrapping_mul(0x165667b19e3779f9);
    h ^ (h >> 31)
}
