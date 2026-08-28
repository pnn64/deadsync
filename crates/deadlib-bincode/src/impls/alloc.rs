use crate::{
    config::{IntEncoding, InternalEndianConfig, InternalIntEncodingConfig},
    de::{read::Reader, BorrowDecode, BorrowDecoder, Decode, Decoder},
    enc::{self, write::SizeWriter, Encode, Encoder},
    error::{DecodeError, EncodeError},
    impl_borrow_decode, Config,
};
use std::{boxed::Box, string::String, vec::Vec};

/// Return the number of bytes required to encode a value.
pub fn encoded_size<E: Encode, C: Config>(val: E, config: C) -> Result<usize, EncodeError> {
    let mut encoder = enc::EncoderImpl::<_, C>::new(SizeWriter::default(), config);
    val.encode(&mut encoder)?;
    Ok(encoder.into_writer().bytes_written)
}

/// Encode a value into a caller-provided byte slice.
///
/// The returned `usize` is the number of destination bytes written. If the
/// destination is too short, the already-written prefix remains in `dst`.
pub fn encode_into_slice<E: Encode, C: Config>(
    val: E,
    dst: &mut [u8],
    config: C,
) -> Result<usize, EncodeError> {
    let writer = enc::write::SliceWriter::new(dst);
    let mut encoder = enc::EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)?;
    Ok(encoder.into_writer().bytes_written())
}

struct VecWriter<'a> {
    inner: &'a mut Vec<u8>,
}

impl enc::write::Writer for VecWriter<'_> {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.inner.extend_from_slice(bytes);
        Ok(())
    }
}

/// Encode a value into a newly allocated byte vector.
pub fn encode_to_vec<E: Encode, C: Config>(val: E, config: C) -> Result<Vec<u8>, EncodeError> {
    let size = encoded_size(&val, config)?;
    let mut bytes = Vec::with_capacity(size);
    encode_into_vec(val, &mut bytes, config)?;
    Ok(bytes)
}

/// Encode a value into a reusable byte vector.
///
/// The vector is cleared before encoding, but retains its allocation. After one
/// sufficiently large call, repeated calls can therefore encode without any
/// allocator traffic. If encoding fails, the vector contains the bytes written
/// before the error.
pub fn encode_into_vec<E: Encode, C: Config>(
    val: E,
    bytes: &mut Vec<u8>,
    config: C,
) -> Result<(), EncodeError> {
    bytes.clear();
    let writer = VecWriter { inner: bytes };
    let mut encoder = enc::EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)
}

fn decode_raw_vec<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
) -> Option<Result<Vec<T>, DecodeError>> {
    if !crate::utils::can_memcpy::<T, D::C>() {
        return None;
    }

    let byte_len = match len.checked_mul(std::mem::size_of::<T>()) {
        Some(len) => len,
        None => return Some(Err(DecodeError::LimitExceeded)),
    };
    let source = decoder.reader().peek_read(byte_len)?;
    if source.len() < byte_len {
        return Some(Err(DecodeError::UnexpectedEnd {
            additional: byte_len - source.len(),
        }));
    }
    let source = &source[..byte_len];
    let mut values = Vec::<T>::with_capacity(len);
    // SAFETY: can_memcpy restricts T to numeric primitives where every bit
    // pattern is valid. The source contains exactly len complete values in
    // native byte order, and with_capacity allocated space for all of them.
    unsafe {
        std::ptr::copy_nonoverlapping(source.as_ptr(), values.as_mut_ptr().cast(), byte_len);
        values.set_len(len);
    }
    decoder.reader().consume(byte_len);
    Some(Ok(values))
}

#[inline]
fn decode_u8_vec<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
) -> Option<Result<Vec<T>, DecodeError>> {
    if !crate::unty::type_equal::<T, u8>() {
        return None;
    }

    let mut bytes = vec![0u8; len];
    if let Err(error) = decoder.reader().read(&mut bytes) {
        return Some(Err(error));
    }

    // SAFETY: `type_equal` established that T and u8 are identical types.
    Some(Ok(unsafe { std::mem::transmute::<Vec<u8>, Vec<T>>(bytes) }))
}

fn copy_into_vec<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
    values: &mut Vec<T>,
) -> Option<Result<(), DecodeError>> {
    let byte_len = match len.checked_mul(std::mem::size_of::<T>()) {
        Some(len) => len,
        None => return Some(Err(DecodeError::LimitExceeded)),
    };
    let source = decoder.reader().peek_read(byte_len)?;
    if source.len() < byte_len {
        return Some(Err(DecodeError::UnexpectedEnd {
            additional: byte_len - source.len(),
        }));
    }

    values.reserve(len);
    // SAFETY: callers restrict T to u8 or to numeric primitives for which
    // every bit pattern is valid. `reserve` provides space for `len` values
    // and source contains the corresponding number of initialized bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(source.as_ptr(), values.as_mut_ptr().cast(), byte_len);
        values.set_len(len);
    }
    decoder.reader().consume(byte_len);
    Some(Ok(()))
}

fn decode_bool_vec_into<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
    values: &mut Vec<T>,
) -> Option<Result<(), DecodeError>> {
    if !crate::unty::type_equal::<T, bool>() {
        return None;
    }

    let source = decoder.reader().peek_read(len)?;
    if source.len() < len {
        return None;
    }
    let source = &source[..len];
    let invalid = source
        .iter()
        .position(|byte| *byte > 1)
        .map(|index| (index, source[index]));
    let valid_len = invalid.map_or(len, |(index, _)| index);

    values.reserve(valid_len);
    // SAFETY: type_equal established that T is bool. Every source byte before
    // valid_len was checked to be 0 or 1, both valid bool representations, and
    // reserve provided space for exactly that many initialized values.
    unsafe {
        std::ptr::copy_nonoverlapping(source.as_ptr(), values.as_mut_ptr().cast::<u8>(), valid_len);
        values.set_len(valid_len);
    }

    if let Some((index, value)) = invalid {
        decoder.reader().consume(index + 1);
        Some(Err(DecodeError::InvalidBooleanValue(value)))
    } else {
        decoder.reader().consume(len);
        Some(Ok(()))
    }
}

fn decode_varint_vec_into<T, D: Decoder>(
    decoder: &mut D,
    len: usize,
    values: &mut Vec<T>,
) -> Option<Result<(), DecodeError>> {
    if D::C::INT_ENCODING != IntEncoding::Variable {
        return None;
    }

    macro_rules! decode_as {
        ($ty:ty, $decode:path) => {
            if crate::unty::type_equal::<T, $ty>() {
                values.reserve(len);
                for _ in 0..len {
                    let value = match $decode(decoder.reader(), D::C::ENDIAN) {
                        Ok(value) => value,
                        Err(error) => return Some(Err(error)),
                    };
                    // SAFETY: `type_equal` established that T and the concrete
                    // integer have identical size, alignment, and validity.
                    unsafe {
                        values
                            .as_mut_ptr()
                            .add(values.len())
                            .cast::<$ty>()
                            .write(value);
                        values.set_len(values.len() + 1);
                    }
                }
                return Some(Ok(()));
            }
        };
    }

    decode_as!(u16, crate::varint::varint_decode_u16);
    decode_as!(u32, crate::varint::varint_decode_u32);
    decode_as!(u64, crate::varint::varint_decode_u64);
    decode_as!(u128, crate::varint::varint_decode_u128);
    decode_as!(usize, crate::varint::varint_decode_usize);
    decode_as!(i16, crate::varint::varint_decode_i16);
    decode_as!(i32, crate::varint::varint_decode_i32);
    decode_as!(i64, crate::varint::varint_decode_i64);
    decode_as!(i128, crate::varint::varint_decode_i128);
    decode_as!(isize, crate::varint::varint_decode_isize);
    None
}

fn decode_string_vec_into<Context, D: Decoder<Context = Context>>(
    decoder: &mut D,
    values: &mut Vec<String>,
) -> Result<(), DecodeError> {
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            values.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<String>(len) {
        values.clear();
        return Err(error);
    }
    values.reserve(len.saturating_sub(values.len()));

    for index in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<String>());
        if index < values.len() {
            if let Err(error) = decode_string_into(decoder, &mut values[index]) {
                values.truncate(index);
                return Err(error);
            }
        } else {
            let mut value = String::new();
            decode_string_into(decoder, &mut value)?;
            values.push(value);
        }
    }
    values.truncate(len);
    Ok(())
}

fn decode_nested_vec_into<Context, T, D>(
    decoder: &mut D,
    values: &mut Vec<Vec<T>>,
) -> Result<(), DecodeError>
where
    T: Decode<Context>,
    D: Decoder<Context = Context>,
{
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            values.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<Vec<T>>(len) {
        values.clear();
        return Err(error);
    }
    values.reserve(len.saturating_sub(values.len()));

    for index in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<Vec<T>>());
        if index < values.len() {
            if let Err(error) = decode_vec_into(decoder, &mut values[index]) {
                values.truncate(index);
                return Err(error);
            }
        } else {
            let mut value = Vec::new();
            decode_vec_into(decoder, &mut value)?;
            values.push(value);
        }
    }
    values.truncate(len);
    Ok(())
}

fn decode_reused_vec_elements<T, Context, D>(
    decoder: &mut D,
    values: &mut Vec<T>,
) -> Option<Result<(), DecodeError>>
where
    T: Decode<Context>,
    D: Decoder<Context = Context>,
{
    // String and Vec have the same three-word representation on supported
    // targets. Most other T values can avoid every TypeId comparison.
    if std::mem::size_of::<T>() != std::mem::size_of::<String>() {
        return None;
    }

    if crate::unty::type_equal::<T, String>() {
        // SAFETY: `type_equal` established that T is String, so the vector
        // layouts and element validity requirements are identical.
        let values = unsafe { &mut *(values as *mut Vec<T>).cast::<Vec<String>>() };
        return Some(decode_string_vec_into(decoder, values));
    }

    macro_rules! decode_nested_as {
        ($ty:ty) => {
            if crate::unty::type_equal::<T, Vec<$ty>>() {
                // SAFETY: `type_equal` established that T is Vec<$ty>, so the
                // outer vector layouts and element validity are identical.
                let values = unsafe { &mut *(values as *mut Vec<T>).cast::<Vec<Vec<$ty>>>() };
                return Some(decode_nested_vec_into(decoder, values));
            }
        };
    }

    decode_nested_as!(u8);
    decode_nested_as!(u16);
    decode_nested_as!(u32);
    decode_nested_as!(u64);
    decode_nested_as!(u128);
    decode_nested_as!(usize);
    decode_nested_as!(i8);
    decode_nested_as!(i16);
    decode_nested_as!(i32);
    decode_nested_as!(i64);
    decode_nested_as!(i128);
    decode_nested_as!(isize);
    decode_nested_as!(f32);
    decode_nested_as!(f64);
    decode_nested_as!(bool);
    decode_nested_as!(String);
    None
}

pub(crate) fn decode_vec_into<T: Decode<Context>, Context, D: Decoder<Context = Context>>(
    decoder: &mut D,
    values: &mut Vec<T>,
) -> Result<(), DecodeError> {
    if !values.is_empty() {
        if let Some(result) = decode_reused_vec_elements(decoder, values) {
            return result;
        }
    }

    values.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<T>(len)?;

    if crate::unty::type_equal::<T, u8>() {
        if let Some(result) = copy_into_vec(decoder, len, values) {
            return result;
        }

        values.reserve(len);
        // SAFETY: `type_equal` established that T is u8, so zero is a valid T
        // and the initialized allocation can be exposed to `Reader::read`.
        unsafe {
            std::ptr::write_bytes(values.as_mut_ptr(), 0, len);
            values.set_len(len);
            let bytes = std::slice::from_raw_parts_mut(values.as_mut_ptr().cast::<u8>(), len);
            decoder.reader().read(bytes)?;
        }
        return Ok(());
    }

    if crate::utils::can_memcpy::<T, D::C>() {
        if let Some(result) = copy_into_vec(decoder, len, values) {
            return result;
        }
    }
    if let Some(result) = decode_bool_vec_into(decoder, len, values) {
        return result;
    }
    if let Some(result) = decode_varint_vec_into(decoder, len, values) {
        return result;
    }

    values.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        values.push(T::decode(decoder)?);
    }
    Ok(())
}

pub(crate) fn borrow_decode_vec_into<
    'de,
    T: BorrowDecode<'de, Context>,
    Context,
    D: BorrowDecoder<'de, Context = Context>,
>(
    decoder: &mut D,
    values: &mut Vec<T>,
) -> Result<(), DecodeError> {
    values.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<T>(len)?;

    if crate::unty::type_equal::<T, u8>() {
        if let Some(result) = copy_into_vec(decoder, len, values) {
            return result;
        }

        values.reserve(len);
        // SAFETY: `type_equal` established that T is u8, so zero is a valid T
        // and the initialized allocation can be exposed to `Reader::read`.
        unsafe {
            std::ptr::write_bytes(values.as_mut_ptr(), 0, len);
            values.set_len(len);
            let bytes = std::slice::from_raw_parts_mut(values.as_mut_ptr().cast::<u8>(), len);
            decoder.reader().read(bytes)?;
        }
        return Ok(());
    }

    if crate::utils::can_memcpy::<T, D::C>() {
        if let Some(result) = copy_into_vec(decoder, len, values) {
            return result;
        }
    }
    if let Some(result) = decode_bool_vec_into(decoder, len, values) {
        return result;
    }
    if let Some(result) = decode_varint_vec_into(decoder, len, values) {
        return result;
    }

    values.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        let value = T::borrow_decode(decoder)?;
        // SAFETY: `reserve(len)` provides enough spare capacity for every
        // iteration. Advancing the length immediately after each write keeps
        // the successfully decoded prefix initialized and droppable on error.
        unsafe {
            values.as_mut_ptr().add(values.len()).write(value);
            values.set_len(values.len() + 1);
        }
    }
    Ok(())
}

pub(crate) fn decode_string_into<Context, D: Decoder<Context = Context>>(
    decoder: &mut D,
    value: &mut String,
) -> Result<(), DecodeError> {
    let mut bytes = std::mem::take(value).into_bytes();
    if let Err(error) = decode_vec_into(decoder, &mut bytes) {
        bytes.clear();
        // SAFETY: an empty byte vector is valid UTF-8. Moving the allocation
        // back also retains capacity on failed decodes.
        *value = unsafe { String::from_utf8_unchecked(bytes) };
        return Err(error);
    }

    match String::from_utf8(bytes) {
        Ok(decoded) => {
            *value = decoded;
            Ok(())
        }
        Err(error) => {
            let inner = error.utf8_error();
            let mut bytes = error.into_bytes();
            bytes.clear();
            // SAFETY: an empty byte vector is valid UTF-8. Moving the
            // allocation back keeps `value` reusable after invalid input.
            *value = unsafe { String::from_utf8_unchecked(bytes) };
            Err(DecodeError::Utf8 { inner })
        }
    }
}

impl<Context, T: Decode<Context>> Decode<Context> for Vec<T> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<T>(len)?;

        if let Some(values) = decode_u8_vec(decoder, len) {
            return values;
        }
        if let Some(values) = decode_raw_vec(decoder, len) {
            return values;
        }

        let mut values = Self::new();
        if let Some(result) = decode_bool_vec_into(decoder, len, &mut values) {
            result?;
            return Ok(values);
        }
        if let Some(result) = decode_varint_vec_into(decoder, len, &mut values) {
            result?;
            return Ok(values);
        }

        values.reserve(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            values.push(T::decode(decoder)?);
        }
        Ok(values)
    }
}

impl<'de, T: BorrowDecode<'de, Context>, Context> BorrowDecode<'de, Context> for Vec<T> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<T>(len)?;

        if let Some(values) = decode_u8_vec(decoder, len) {
            return values;
        }
        if let Some(values) = decode_raw_vec(decoder, len) {
            return values;
        }

        let mut values = Self::with_capacity(len);
        if let Some(result) = decode_bool_vec_into(decoder, len, &mut values) {
            result?;
            return Ok(values);
        }
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            values.push(T::borrow_decode(decoder)?);
        }
        Ok(values)
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.as_slice().encode(encoder)
    }
}

impl<Context> Decode<Context> for String {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Self::from_utf8(Vec::<u8>::decode(decoder)?).map_err(|error| DecodeError::Utf8 {
            inner: error.utf8_error(),
        })
    }
}

impl_borrow_decode!(String);

impl Encode for String {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.as_str().encode(encoder)
    }
}

impl<Context, T: Decode<Context> + 'static> Decode<Context> for Box<[T]> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Vec::decode(decoder)?.into_boxed_slice())
    }
}

impl<'de, T: BorrowDecode<'de, Context> + 'de, Context> BorrowDecode<'de, Context> for Box<[T]> {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        Ok(Vec::borrow_decode(decoder)?.into_boxed_slice())
    }
}

impl<T: Encode> Encode for Box<[T]> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        self.as_ref().encode(encoder)
    }
}
