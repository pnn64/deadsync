use crate::{
    de::{read::Reader, BorrowDecode, BorrowDecoder, Decode, Decoder},
    enc::{self, write::SizeWriter, Encode, Encoder},
    error::{DecodeError, EncodeError},
    impl_borrow_decode, Config,
};
use std::{boxed::Box, string::String, vec::Vec};

#[derive(Default)]
struct VecWriter {
    inner: Vec<u8>,
}

impl VecWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Vec::with_capacity(capacity),
        }
    }
}

impl enc::write::Writer for VecWriter {
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
    let writer = VecWriter::with_capacity(size);
    let mut encoder = enc::EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)?;
    Ok(encoder.into_writer().inner)
}

impl<Context, T: Decode<Context>> Decode<Context> for Vec<T> {
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<T>(len)?;

        if crate::unty::type_equal::<T, u8>() {
            let mut bytes = vec![0u8; len];
            decoder.reader().read(&mut bytes)?;
            // SAFETY: type_equal established that T and u8 are identical types.
            return Ok(unsafe { std::mem::transmute::<Vec<u8>, Vec<T>>(bytes) });
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

        if crate::unty::type_equal::<T, u8>() {
            let mut bytes = vec![0u8; len];
            decoder.reader().read(&mut bytes)?;
            // SAFETY: type_equal established that T and u8 are identical types.
            return Ok(unsafe { std::mem::transmute::<Vec<u8>, Vec<T>>(bytes) });
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
