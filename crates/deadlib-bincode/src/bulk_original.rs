//! Frozen production reference from 63cdbfd0a.
use crate::config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig};
use crate::de::{read::Reader, Decoder};
use crate::error::DecodeError;

pub(super) fn decode_endian_vec_into<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
    values: &mut Vec<T>,
    decode_floats: bool,
) -> Option<Result<(), DecodeError>> {
    let fixed_integers = D::C::INT_ENCODING == IntEncoding::Fixed;

    macro_rules! decode_as {
        ($ty:ty, $size:literal) => {
            if crate::unty::type_equal::<T, $ty>() {
                let byte_len = match len.checked_mul($size) {
                    Some(byte_len) => byte_len,
                    None => return Some(Err(DecodeError::LimitExceeded)),
                };
                let source = decoder.reader().peek_read(byte_len)?;
                if source.len() < byte_len {
                    return Some(Err(DecodeError::UnexpectedEnd {
                        additional: byte_len - source.len(),
                    }));
                }

                values.reserve(len);
                for bytes in source[..byte_len].chunks_exact($size) {
                    let bytes: [u8; $size] = bytes.try_into().unwrap();
                    let value = match D::C::ENDIAN {
                        Endianness::Little => <$ty>::from_le_bytes(bytes),
                        Endianness::Big => <$ty>::from_be_bytes(bytes),
                    };
                    // SAFETY: `type_equal` established that T is the concrete
                    // numeric type, and reserve provided room for every value.
                    unsafe {
                        values
                            .as_mut_ptr()
                            .add(values.len())
                            .cast::<$ty>()
                            .write(value);
                        values.set_len(values.len() + 1);
                    }
                }
                decoder.reader().consume(byte_len);
                return Some(Ok(()));
            }
        };
    }

    if fixed_integers {
        match core::mem::size_of::<T>() {
            2 => {
                decode_as!(u16, 2);
                decode_as!(i16, 2);
            }
            4 => {
                decode_as!(u32, 4);
                decode_as!(i32, 4);
            }
            8 => {
                decode_as!(u64, 8);
                decode_as!(i64, 8);
            }
            16 => {
                decode_as!(u128, 16);
                decode_as!(i128, 16);
            }
            _ => {}
        }
    }
    if decode_floats {
        match core::mem::size_of::<T>() {
            4 => decode_as!(f32, 4),
            8 => decode_as!(f64, 8),
            _ => {}
        }
    }
    None
}
