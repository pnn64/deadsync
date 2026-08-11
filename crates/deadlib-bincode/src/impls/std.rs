use crate::{
    de::{BorrowDecode, BorrowDecoder, Decode, Decoder},
    enc::{Encode, Encoder},
    error::{DecodeError, EncodeError},
};
use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, Hash},
};

impl std::error::Error for EncodeError {}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Utf8 { inner } => Some(inner),
            _ => None,
        }
    }
}

impl<K: Encode, V: Encode, S> Encode for HashMap<K, V, S> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        crate::enc::encode_slice_len(encoder, self.len())?;
        for (key, value) in self {
            key.encode(encoder)?;
            value.encode(encoder)?;
        }
        Ok(())
    }
}

impl<Context, K, V, S> Decode<Context> for HashMap<K, V, S>
where
    K: Decode<Context> + Eq + Hash,
    V: Decode<Context>,
    S: BuildHasher + Default,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<(K, V)>(len)?;
        let mut map = HashMap::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
            map.insert(K::decode(decoder)?, V::decode(decoder)?);
        }
        Ok(map)
    }
}

impl<'de, Context, K, V, S> BorrowDecode<'de, Context> for HashMap<K, V, S>
where
    K: BorrowDecode<'de, Context> + Eq + Hash,
    V: BorrowDecode<'de, Context>,
    S: BuildHasher + Default,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<(K, V)>(len)?;
        let mut map = HashMap::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
            map.insert(K::borrow_decode(decoder)?, V::borrow_decode(decoder)?);
        }
        Ok(map)
    }
}

impl<T: Encode, S> Encode for HashSet<T, S> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        crate::enc::encode_slice_len(encoder, self.len())?;
        for value in self {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

impl<Context, T, S> Decode<Context> for HashSet<T, S>
where
    T: Decode<Context> + Eq + Hash,
    S: BuildHasher + Default,
{
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<T>(len)?;
        let mut set = HashSet::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            set.insert(T::decode(decoder)?);
        }
        Ok(set)
    }
}

impl<'de, Context, T, S> BorrowDecode<'de, Context> for HashSet<T, S>
where
    T: BorrowDecode<'de, Context> + Eq + Hash,
    S: BuildHasher + Default,
{
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, DecodeError> {
        let len = crate::de::decode_slice_len(decoder)?;
        decoder.claim_container_read::<T>(len)?;
        let mut set = HashSet::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            set.insert(T::borrow_decode(decoder)?);
        }
        Ok(set)
    }
}
