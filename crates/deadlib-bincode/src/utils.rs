use crate::config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig};

pub trait Sealed {}

impl<T> Sealed for &mut T where T: Sealed {}

pub(crate) fn can_memcpy<T, C>() -> bool
where
    C: InternalEndianConfig + InternalIntEncodingConfig,
{
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
