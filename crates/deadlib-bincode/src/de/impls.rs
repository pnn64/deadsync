use super::{
    read::{BorrowReader, Reader},
    BorrowDecode, BorrowDecoder, Decode, Decoder,
};
use crate::{
    config::{Endianness, IntEncoding, InternalEndianConfig, InternalIntEncodingConfig},
    error::DecodeError,
    impl_borrow_decode,
};

#[inline]
fn decode_u8_array<T, D: Decoder, const N: usize>(
    decoder: &mut D,
) -> Option<Result<[T; N], DecodeError>> {
    if !crate::unty::type_equal::<T, u8>() {
        return None;
    }

    let mut bytes = [0u8; N];
    if let Err(error) = decoder.reader().read(&mut bytes) {
        return Some(Err(error));
    }

    // SAFETY: `type_equal` established that T and u8 are identical types.
    Some(Ok(unsafe {
        (&bytes as *const [u8; N]).cast::<[T; N]>().read()
    }))
}

fn decode_raw_array<T, D: Decoder, const N: usize>(
    decoder: &mut D,
) -> Option<Result<[T; N], DecodeError>> {
    if !crate::utils::can_memcpy::<T, D::C>() {
        return None;
    }

    let byte_len = core::mem::size_of::<[T; N]>();
    let source = decoder.reader().peek_read(byte_len)?;
    if source.len() < byte_len {
        return Some(Err(DecodeError::UnexpectedEnd {
            additional: byte_len - source.len(),
        }));
    }
    let mut values = core::mem::MaybeUninit::<[T; N]>::uninit();
    // SAFETY: `can_memcpy` restricts T to padding-free types where every bit
    // pattern is valid and memory matches the configured wire representation.
    // The source has exactly N complete values and `values` provides enough
    // correctly aligned storage.
    unsafe {
        core::ptr::copy_nonoverlapping(source.as_ptr(), values.as_mut_ptr().cast::<u8>(), byte_len);
        decoder.reader().consume(byte_len);
        Some(Ok(values.assume_init()))
    }
}

fn decode_bool_array<T, D: Decoder, const N: usize>(
    decoder: &mut D,
) -> Option<Result<[T; N], DecodeError>> {
    if !crate::unty::type_equal::<T, bool>() {
        return None;
    }

    let source = decoder.reader().peek_read(N)?;
    if source.len() < N {
        return None;
    }
    let source = &source[..N];
    if let Some(index) = source.iter().position(|byte| *byte > 1) {
        let value = source[index];
        decoder.reader().consume(index + 1);
        return Some(Err(DecodeError::InvalidBooleanValue(value)));
    }

    let mut values = [false; N];
    // SAFETY: type_equal established that T is bool. Every source byte was
    // checked to be 0 or 1, both valid bool representations, and values has
    // storage for exactly N bools.
    unsafe {
        core::ptr::copy_nonoverlapping(source.as_ptr(), values.as_mut_ptr().cast::<u8>(), N);
    }
    decoder.reader().consume(N);
    // SAFETY: type_equal established that T and bool are the same type.
    Some(Ok(unsafe {
        (&values as *const [bool; N]).cast::<[T; N]>().read()
    }))
}

fn decode_varint_array<T, D: Decoder, const N: usize>(
    decoder: &mut D,
) -> Option<Result<[T; N], DecodeError>> {
    if D::C::INT_ENCODING != IntEncoding::Variable {
        return None;
    }

    macro_rules! decode_as {
        ($ty:ty, $decode:path) => {
            if crate::unty::type_equal::<T, $ty>() {
                let mut values = core::mem::MaybeUninit::<[T; N]>::uninit();
                let output = values.as_mut_ptr().cast::<$ty>();
                for index in 0..N {
                    let value = match $decode(decoder.reader(), D::C::ENDIAN) {
                        Ok(value) => value,
                        Err(error) => return Some(Err(error)),
                    };
                    // SAFETY: `type_equal` established that T is the concrete
                    // integer type, and index is within the N-element array.
                    unsafe { output.add(index).write(value) };
                }
                // SAFETY: every one of the N array elements was initialized.
                return Some(Ok(unsafe { values.assume_init() }));
            }
        };
    }

    match core::mem::size_of::<T>() {
        2 => {
            decode_as!(u16, crate::varint::varint_decode_u16);
            decode_as!(i16, crate::varint::varint_decode_i16);
        }
        4 => {
            decode_as!(u32, crate::varint::varint_decode_u32);
            decode_as!(i32, crate::varint::varint_decode_i32);
            decode_as!(usize, crate::varint::varint_decode_usize);
            decode_as!(isize, crate::varint::varint_decode_isize);
        }
        8 => {
            decode_as!(u64, crate::varint::varint_decode_u64);
            decode_as!(i64, crate::varint::varint_decode_i64);
            decode_as!(usize, crate::varint::varint_decode_usize);
            decode_as!(isize, crate::varint::varint_decode_isize);
        }
        16 => {
            decode_as!(u128, crate::varint::varint_decode_u128);
            decode_as!(i128, crate::varint::varint_decode_i128);
        }
        _ => {}
    }
    None
}

impl<Context> Decode<Context> for bool {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match u8::decode(decoder)? {
            0 => Ok(false),
            1 => Ok(true),
            x => Err(DecodeError::InvalidBooleanValue(x)),
        }
    }
}
impl_borrow_decode!(bool);

impl<Context> Decode<Context> for u8 {
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(1)?;
        if let Some(buf) = decoder.reader().peek_read(1) {
            let byte = buf[0];
            decoder.reader().consume(1);
            Ok(byte)
        } else {
            let mut bytes = [0u8; 1];
            decoder.reader().read(&mut bytes)?;
            Ok(bytes[0])
        }
    }
}
impl_borrow_decode!(u8);

impl<Context> Decode<Context> for u16 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(2)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_u16(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 2];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(u16);

impl<Context> Decode<Context> for u32 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(4)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_u32(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 4];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(u32);

impl<Context> Decode<Context> for u64 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(8)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_u64(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 8];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(u64);

impl<Context> Decode<Context> for u128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(16)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_u128(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 16];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(u128);

impl<Context> Decode<Context> for usize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(8)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_usize(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 8];
                decoder.reader().read(&mut bytes)?;

                let value = match D::C::ENDIAN {
                    Endianness::Little => u64::from_le_bytes(bytes),
                    Endianness::Big => u64::from_be_bytes(bytes),
                };

                value
                    .try_into()
                    .map_err(|_| DecodeError::OutsideUsizeRange(value))
            }
        }
    }
}
impl_borrow_decode!(usize);

impl<Context> Decode<Context> for i8 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(1)?;
        let mut bytes = [0u8; 1];
        decoder.reader().read(&mut bytes)?;
        Ok(bytes[0] as Self)
    }
}
impl_borrow_decode!(i8);

impl<Context> Decode<Context> for i16 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(2)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_i16(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 2];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(i16);

impl<Context> Decode<Context> for i32 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(4)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_i32(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 4];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(i32);

impl<Context> Decode<Context> for i64 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(8)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_i64(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 8];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(i64);

impl<Context> Decode<Context> for i128 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(16)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_i128(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 16];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => Self::from_le_bytes(bytes),
                    Endianness::Big => Self::from_be_bytes(bytes),
                })
            }
        }
    }
}
impl_borrow_decode!(i128);

impl<Context> Decode<Context> for isize {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(8)?;
        match D::C::INT_ENCODING {
            IntEncoding::Variable => {
                crate::varint::varint_decode_isize(decoder.reader(), D::C::ENDIAN)
            }
            IntEncoding::Fixed => {
                let mut bytes = [0u8; 8];
                decoder.reader().read(&mut bytes)?;
                Ok(match D::C::ENDIAN {
                    Endianness::Little => i64::from_le_bytes(bytes),
                    Endianness::Big => i64::from_be_bytes(bytes),
                } as Self)
            }
        }
    }
}
impl_borrow_decode!(isize);

impl<Context> Decode<Context> for f32 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(4)?;
        let mut bytes = [0u8; 4];
        decoder.reader().read(&mut bytes)?;
        Ok(match D::C::ENDIAN {
            Endianness::Little => Self::from_le_bytes(bytes),
            Endianness::Big => Self::from_be_bytes(bytes),
        })
    }
}
impl_borrow_decode!(f32);

impl<Context> Decode<Context> for f64 {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(8)?;
        let mut bytes = [0u8; 8];
        decoder.reader().read(&mut bytes)?;
        Ok(match D::C::ENDIAN {
            Endianness::Little => Self::from_le_bytes(bytes),
            Endianness::Big => Self::from_be_bytes(bytes),
        })
    }
}
impl_borrow_decode!(f64);

impl<'a, 'de: 'a, Context> BorrowDecode<'de, Context> for &'a [u8] {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let len = super::decode_slice_len(decoder)?;
        decoder.claim_bytes_read(len)?;
        decoder.borrow_reader().take_bytes(len)
    }
}

impl<'a, 'de: 'a, Context> BorrowDecode<'de, Context> for &'a str {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let slice = <&[u8]>::borrow_decode(decoder)?;
        core::str::from_utf8(slice).map_err(|inner| DecodeError::Utf8 { inner })
    }
}

impl<Context, T, const N: usize> Decode<Context> for [T; N]
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(core::mem::size_of::<[T; N]>())?;

        if let Some(result) = decode_u8_array(decoder) {
            result
        } else if let Some(result) = decode_raw_array(decoder) {
            result
        } else if let Some(result) = decode_bool_array(decoder) {
            result
        } else if let Some(result) = decode_varint_array(decoder) {
            result
        } else {
            let result = super::impl_core::collect_into_array(&mut (0..N).map(|_| {
                // See the documentation on `unclaim_bytes_read` as to why we're doing this here
                decoder.unclaim_bytes_read(core::mem::size_of::<T>());
                T::decode(decoder)
            }));

            // result is only None if N does not match the values of `(0..N)`, which it always should
            // So this unwrap should never occur
            result.unwrap()
        }
    }
}

impl<'de, T, const N: usize, Context> BorrowDecode<'de, Context> for [T; N]
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        decoder.claim_bytes_read(core::mem::size_of::<[T; N]>())?;

        if let Some(result) = decode_u8_array(decoder) {
            result
        } else if let Some(result) = decode_raw_array(decoder) {
            result
        } else if let Some(result) = decode_bool_array(decoder) {
            result
        } else if let Some(result) = decode_varint_array(decoder) {
            result
        } else {
            let result = super::impl_core::collect_into_array(&mut (0..N).map(|_| {
                // See the documentation on `unclaim_bytes_read` as to why we're doing this here
                decoder.unclaim_bytes_read(core::mem::size_of::<T>());
                T::borrow_decode(decoder)
            }));

            // result is only None if N does not match the values of `(0..N)`, which it always should
            // So this unwrap should never occur
            result.unwrap()
        }
    }
}

impl<Context> Decode<Context> for () {
    fn decode<D: Decoder<Context = Context>>(_: &mut D) -> Result<Self, DecodeError> {
        Ok(())
    }
}
impl_borrow_decode!(());

impl<Context, T> Decode<Context> for Option<T>
where
    T: Decode<Context>,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match super::decode_option_variant(decoder, core::any::type_name::<Self>())? {
            Some(_) => {
                let val = T::decode(decoder)?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }
}

impl<'de, T, Context> BorrowDecode<'de, Context> for Option<T>
where
    T: BorrowDecode<'de, Context>,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        match super::decode_option_variant(decoder, core::any::type_name::<Self>())? {
            Some(_) => {
                let val = T::borrow_decode(decoder)?;
                Ok(Some(val))
            }
            None => Ok(None),
        }
    }
}
