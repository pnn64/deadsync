use crate::{
    de::{BorrowDecode, BorrowDecoder, Decode, Decoder},
    enc::{Encode, Encoder},
    error::{DecodeError, EncodeError},
};
use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, Hash},
};

fn decode_string_in_place<Context, D>(
    decoder: &mut D,
    value: &mut String,
) -> Result<(), DecodeError>
where
    D: Decoder<Context = Context>,
{
    crate::impls::decode_string_into(decoder, value)
}

fn decode_bytes_in_place<Context, D>(
    decoder: &mut D,
    value: &mut Vec<u8>,
) -> Result<(), DecodeError>
where
    D: Decoder<Context = Context>,
{
    crate::impls::decode_vec_into(decoder, value)
}

fn decode_string_vector_in_place<Context, D>(
    decoder: &mut D,
    value: &mut Vec<String>,
) -> Result<(), DecodeError>
where
    D: Decoder<Context = Context>,
{
    crate::impls::decode_vec_into(decoder, value)
}

fn decode_reused_key_hash_map_into<Context, K, V, S, D, F>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
    mut decode_key: F,
) -> Result<(), DecodeError>
where
    K: Decode<Context> + Default + Eq + Hash,
    V: Decode<Context>,
    S: BuildHasher,
    D: Decoder<Context = Context>,
    F: FnMut(&mut D, &mut K) -> Result<(), DecodeError>,
{
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            map.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<(K, V)>(len) {
        map.clear();
        return Err(error);
    }

    let mut keys = map.drain().map(|(key, _)| key).collect::<Vec<_>>();
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        let mut key = keys.pop().unwrap_or_default();
        decode_key(decoder, &mut key)?;
        map.insert(key, V::decode(decoder)?);
    }
    Ok(())
}

fn decode_reused_value_hash_map_into<Context, K, V, S, D, F>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
    mut decode_value: F,
) -> Result<(), DecodeError>
where
    K: Decode<Context> + Eq + Hash,
    V: Decode<Context> + Default,
    S: BuildHasher,
    D: Decoder<Context = Context>,
    F: FnMut(&mut D, &mut V) -> Result<(), DecodeError>,
{
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            map.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<(K, V)>(len) {
        map.clear();
        return Err(error);
    }

    let mut values = map.drain().map(|(_, value)| value).collect::<Vec<_>>();
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        let key = K::decode(decoder)?;
        let mut value = values.pop().unwrap_or_default();
        decode_value(decoder, &mut value)?;
        map.insert(key, value);
    }
    Ok(())
}

fn decode_reused_pair_hash_map_into<Context, K, V, S, D, FK, FV>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
    mut decode_key: FK,
    mut decode_value: FV,
) -> Result<(), DecodeError>
where
    K: Decode<Context> + Default + Eq + Hash,
    V: Decode<Context> + Default,
    S: BuildHasher,
    D: Decoder<Context = Context>,
    FK: FnMut(&mut D, &mut K) -> Result<(), DecodeError>,
    FV: FnMut(&mut D, &mut V) -> Result<(), DecodeError>,
{
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            map.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<(K, V)>(len) {
        map.clear();
        return Err(error);
    }

    let mut entries = map.drain().collect::<Vec<_>>();
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        let (mut key, mut value) = entries.pop().unwrap_or_default();
        decode_key(decoder, &mut key)?;
        decode_value(decoder, &mut value)?;
        map.insert(key, value);
    }
    Ok(())
}

fn decode_reused_hash_map_buffers<Context, K, V, S, D>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
) -> Option<Result<(), DecodeError>>
where
    K: Decode<Context> + Eq + Hash,
    V: Decode<Context>,
    S: BuildHasher,
    D: Decoder<Context = Context>,
{
    let string_size = std::mem::size_of::<String>();
    let string_key =
        std::mem::size_of::<K>() == string_size && crate::unty::type_equal::<K, String>();
    let string_value =
        std::mem::size_of::<V>() == string_size && crate::unty::type_equal::<V, String>();
    let bytes_key =
        std::mem::size_of::<K>() == string_size && crate::unty::type_equal::<K, Vec<u8>>();
    let bytes_value =
        std::mem::size_of::<V>() == string_size && crate::unty::type_equal::<V, Vec<u8>>();
    let string_vector_key =
        std::mem::size_of::<K>() == string_size && crate::unty::type_equal::<K, Vec<String>>();
    let string_vector_value =
        std::mem::size_of::<V>() == string_size && crate::unty::type_equal::<V, Vec<String>>();

    if string_key && string_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<String, String, S>>() };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_in_place,
            decode_string_in_place,
        ));
    }
    if string_key && bytes_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<String, Vec<u8>, S>>() };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_in_place,
            decode_bytes_in_place,
        ));
    }
    if string_key && string_vector_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map = unsafe {
            &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<String, Vec<String>, S>>()
        };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_in_place,
            decode_string_vector_in_place,
        ));
    }
    if bytes_key && string_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<u8>, String, S>>() };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_bytes_in_place,
            decode_string_in_place,
        ));
    }
    if bytes_key && bytes_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<u8>, Vec<u8>, S>>() };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_bytes_in_place,
            decode_bytes_in_place,
        ));
    }
    if bytes_key && string_vector_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map = unsafe {
            &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<u8>, Vec<String>, S>>()
        };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_bytes_in_place,
            decode_string_vector_in_place,
        ));
    }
    if string_vector_key && string_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map = unsafe {
            &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<String>, String, S>>()
        };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_vector_in_place,
            decode_string_in_place,
        ));
    }
    if string_vector_key && bytes_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map = unsafe {
            &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<String>, Vec<u8>, S>>()
        };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_vector_in_place,
            decode_bytes_in_place,
        ));
    }
    if string_vector_key && string_vector_value {
        // SAFETY: both type comparisons established the exact key and value
        // types, so this is the same HashMap instantiation.
        let map = unsafe {
            &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<String>, Vec<String>, S>>()
        };
        return Some(decode_reused_pair_hash_map_into(
            decoder,
            map,
            decode_string_vector_in_place,
            decode_string_vector_in_place,
        ));
    }
    if string_key {
        // SAFETY: the type comparison established that K is exactly String.
        let map = unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<String, V, S>>() };
        return Some(decode_reused_key_hash_map_into(
            decoder,
            map,
            decode_string_in_place,
        ));
    }
    if bytes_key {
        // SAFETY: the type comparison established that K is exactly Vec<u8>.
        let map = unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<u8>, V, S>>() };
        return Some(decode_reused_key_hash_map_into(
            decoder,
            map,
            decode_bytes_in_place,
        ));
    }
    if string_vector_key {
        // SAFETY: the type comparison established that K is exactly Vec<String>.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<Vec<String>, V, S>>() };
        return Some(decode_reused_key_hash_map_into(
            decoder,
            map,
            decode_string_vector_in_place,
        ));
    }
    if string_value {
        // SAFETY: the type comparison established that V is exactly String.
        let map = unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<K, String, S>>() };
        return Some(decode_reused_value_hash_map_into(
            decoder,
            map,
            decode_string_in_place,
        ));
    }
    if bytes_value {
        // SAFETY: the type comparison established that V is exactly Vec<u8>.
        let map = unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<K, Vec<u8>, S>>() };
        return Some(decode_reused_value_hash_map_into(
            decoder,
            map,
            decode_bytes_in_place,
        ));
    }
    if string_vector_value {
        // SAFETY: the type comparison established that V is exactly Vec<String>.
        let map =
            unsafe { &mut *(map as *mut HashMap<K, V, S>).cast::<HashMap<K, Vec<String>, S>>() };
        return Some(decode_reused_value_hash_map_into(
            decoder,
            map,
            decode_string_vector_in_place,
        ));
    }
    None
}

fn decode_reused_hash_set_into<Context, T, S, D, F>(
    decoder: &mut D,
    set: &mut HashSet<T, S>,
    mut decode_value: F,
) -> Result<(), DecodeError>
where
    T: Decode<Context> + Default + Eq + Hash,
    S: BuildHasher,
    D: Decoder<Context = Context>,
    F: FnMut(&mut D, &mut T) -> Result<(), DecodeError>,
{
    let len = match crate::de::decode_slice_len(decoder) {
        Ok(len) => len,
        Err(error) => {
            set.clear();
            return Err(error);
        }
    };
    if let Err(error) = decoder.claim_container_read::<T>(len) {
        set.clear();
        return Err(error);
    }

    let mut values = set.drain().collect::<Vec<_>>();
    set.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        let mut value = values.pop().unwrap_or_default();
        decode_value(decoder, &mut value)?;
        set.insert(value);
    }
    Ok(())
}

pub(crate) fn decode_hash_map_into<Context, K, V, S, D>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
) -> Result<(), DecodeError>
where
    K: Decode<Context> + Eq + Hash,
    V: Decode<Context>,
    S: BuildHasher,
    D: Decoder<Context = Context>,
{
    let string_size = std::mem::size_of::<String>();
    if (std::mem::size_of::<K>() == string_size || std::mem::size_of::<V>() == string_size)
        && !map.is_empty()
    {
        if let Some(result) = decode_reused_hash_map_buffers(decoder, map) {
            return result;
        }
    }

    map.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<(K, V)>(len)?;
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        map.insert(K::decode(decoder)?, V::decode(decoder)?);
    }
    Ok(())
}

pub(crate) fn decode_hash_set_into<Context, T, S, D>(
    decoder: &mut D,
    set: &mut HashSet<T, S>,
) -> Result<(), DecodeError>
where
    T: Decode<Context> + Eq + Hash,
    S: BuildHasher,
    D: Decoder<Context = Context>,
{
    if std::mem::size_of::<T>() == std::mem::size_of::<String>() && !set.is_empty() {
        if crate::unty::type_equal::<T, String>() {
            // SAFETY: the type comparison established that T is exactly String.
            let set = unsafe { &mut *(set as *mut HashSet<T, S>).cast::<HashSet<String, S>>() };
            return decode_reused_hash_set_into(decoder, set, decode_string_in_place);
        }
        if crate::unty::type_equal::<T, Vec<u8>>() {
            // SAFETY: the type comparison established that T is exactly Vec<u8>.
            let set = unsafe { &mut *(set as *mut HashSet<T, S>).cast::<HashSet<Vec<u8>, S>>() };
            return decode_reused_hash_set_into(decoder, set, decode_bytes_in_place);
        }
        if crate::unty::type_equal::<T, Vec<String>>() {
            // SAFETY: the type comparison established that T is exactly Vec<String>.
            let set =
                unsafe { &mut *(set as *mut HashSet<T, S>).cast::<HashSet<Vec<String>, S>>() };
            return decode_reused_hash_set_into(decoder, set, decode_string_vector_in_place);
        }
    }

    set.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<T>(len)?;
    set.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        set.insert(T::decode(decoder)?);
    }
    Ok(())
}

pub(crate) fn borrow_decode_hash_map_into<'de, Context, K, V, S, D>(
    decoder: &mut D,
    map: &mut HashMap<K, V, S>,
) -> Result<(), DecodeError>
where
    K: BorrowDecode<'de, Context> + Eq + Hash,
    V: BorrowDecode<'de, Context>,
    S: BuildHasher,
    D: BorrowDecoder<'de, Context = Context>,
{
    map.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<(K, V)>(len)?;
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        map.insert(K::borrow_decode(decoder)?, V::borrow_decode(decoder)?);
    }
    Ok(())
}

pub(crate) fn borrow_decode_hash_set_into<'de, Context, T, S, D>(
    decoder: &mut D,
    set: &mut HashSet<T, S>,
) -> Result<(), DecodeError>
where
    T: BorrowDecode<'de, Context> + Eq + Hash,
    S: BuildHasher,
    D: BorrowDecoder<'de, Context = Context>,
{
    set.clear();
    let len = crate::de::decode_slice_len(decoder)?;
    decoder.claim_container_read::<T>(len)?;
    set.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        set.insert(T::borrow_decode(decoder)?);
    }
    Ok(())
}

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
        let mut map = Self::with_capacity_and_hasher(len, S::default());
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
        let mut map = Self::with_capacity_and_hasher(len, S::default());
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
        let mut set = Self::with_capacity_and_hasher(len, S::default());
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
        let mut set = Self::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            set.insert(T::borrow_decode(decoder)?);
        }
        Ok(set)
    }
}
