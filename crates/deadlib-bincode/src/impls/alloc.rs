use crate::{
    de::{read::Reader, BorrowDecode, BorrowDecoder, Decode, Decoder},
    enc::{self, write::SizeWriter, Encode, Encoder},
    error::{DecodeError, EncodeError},
    impl_borrow_decode, Config,
};
use std::{boxed::Box, string::String, vec::Vec};

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
    let size = {
        let mut encoder = enc::EncoderImpl::<_, C>::new(SizeWriter::default(), config);
        val.encode(&mut encoder)?;
        encoder.into_writer().bytes_written
    };
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
    // pattern is valid, the source contains exactly len complete values in
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

        let mut values = Vec::with_capacity(len);
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

        let mut values = Vec::with_capacity(len);
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
        String::from_utf8(Vec::<u8>::decode(decoder)?).map_err(|error| DecodeError::Utf8 {
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
