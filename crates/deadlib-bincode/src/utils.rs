use crate::config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig};

pub trait Sealed {}

impl<T> Sealed for &mut T where T: Sealed {}

pub fn can_memcpy<T, C>() -> bool
where
    C: InternalEndianConfig + InternalIntEncodingConfig,
{
    // i8 is always encoded as its single in-memory byte, independently of
    // integer encoding or byte order, and every bit pattern is valid. Keep
    // this direct gate separate so its hot-path code generation stays minimal.
    if crate::unty::type_equal::<T, i8>() {
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
