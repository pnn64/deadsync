use crate::config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig};

pub trait Sealed {}

impl<T> Sealed for &mut T where T: Sealed {}

/// Locate the first byte that cannot represent a wire-format Boolean.
#[inline]
pub(crate) fn invalid_bool(bytes: &[u8]) -> Option<usize> {
    let valid = |block: &[u8]| {
        let bits = block.chunks_exact(16).fold(0, |bits, word| {
            bits | u128::from_ne_bytes(word.try_into().expect("sixteen-byte chunk"))
        });
        bits & u128::from_ne_bytes([0xfe; 16]) == 0
    };
    let mut offset = 0;
    for block in bytes.chunks_exact(256) {
        // Validate sixteen packed words before branching; the ordered scan below
        // still locates the first invalid byte inside a failing block.
        if !valid(block) {
            break;
        }
        offset += 256;
    }
    for block in bytes[offset..].chunks_exact(64) {
        if !valid(block) {
            break;
        }
        offset += 64;
    }
    let mut words = bytes[offset..].chunks_exact(8);
    for (index, word) in words.by_ref().enumerate() {
        // Only bit zero may be set in each byte. Little endian makes the
        // least significant invalid bit identify the earliest source byte.
        let invalid =
            u64::from_le_bytes(word.try_into().expect("eight-byte chunk")) & 0xfefe_fefe_fefe_fefe;
        if invalid != 0 {
            return Some(offset + index * 8 + invalid.trailing_zeros() as usize / 8);
        }
    }
    let tail = words.remainder();
    tail.iter()
        .position(|&byte| byte > 1)
        .map(|index| bytes.len() - tail.len() + index)
}

pub fn can_memcpy<T, C>() -> bool
where
    C: InternalEndianConfig + InternalIntEncodingConfig,
{
    // u8 and i8 are encoded as their single in-memory byte, independently of
    // integer encoding or byte order, and every bit pattern is valid. Keep
    // this direct gate separate so its hot-path code generation stays minimal.
    if crate::unty::type_equal::<T, u8>() || crate::unty::type_equal::<T, i8>() {
        return true;
    }

    // These alignment-one arrays also have wire representations identical to
    // memory. Size gates keep unrelated primitive paths free of more checks.
    if core::mem::align_of::<T>() == 1 {
        match core::mem::size_of::<T>() {
            16 if crate::unty::type_equal::<T, [u8; 16]>() => return true,
            32 if crate::unty::type_equal::<T, [u8; 32]>() => return true,
            64 if crate::unty::type_equal::<T, [u8; 64]>() => return true,
            _ => {}
        }
    }

    let native_endian = match C::ENDIAN {
        Endianness::Little => cfg!(target_endian = "little"),
        Endianness::Big => cfg!(target_endian = "big"),
    };
    if !native_endian {
        return false;
    }

    crate::unty::type_equal::<T, f32>()
        || crate::unty::type_equal::<T, f64>()
        || (C::INT_ENCODING == IntEncoding::Fixed
            && (crate::unty::type_equal::<T, u16>()
                || crate::unty::type_equal::<T, u32>()
                || crate::unty::type_equal::<T, u64>()
                || crate::unty::type_equal::<T, u128>()
                || crate::unty::type_equal::<T, i16>()
                || crate::unty::type_equal::<T, i32>()
                || crate::unty::type_equal::<T, i64>()
                || crate::unty::type_equal::<T, i128>()))
}
