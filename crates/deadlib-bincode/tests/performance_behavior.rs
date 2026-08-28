use bincode::{BorrowDecode, Decode, Encode};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

static BORROWED_DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq, Encode, BorrowDecode)]
struct BorrowedPayload<'a> {
    name: &'a str,
    bytes: &'a [u8],
}

struct DropTrackedBorrowed<'a>(&'a str);

impl Drop for DropTrackedBorrowed<'_> {
    fn drop(&mut self) {
        BORROWED_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

impl<'de> BorrowDecode<'de, ()> for DropTrackedBorrowed<'de> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = ()>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self(<&str>::borrow_decode(decoder)?))
    }
}

#[test]
fn reusable_encoding_matches_allocating_encoding() {
    let value = (
        "DeadSync persistence",
        vec![0u64, 250, 251, u64::from(u32::MAX), u64::MAX],
    );
    let config = bincode::config::standard();
    let expected = bincode::encode_to_vec(&value, config).unwrap();
    let mut bytes = Vec::with_capacity(expected.len() * 2);
    bytes.extend_from_slice(b"discarded contents");
    let allocation = bytes.as_ptr();

    bincode::encode_into_vec(&value, &mut bytes, config).unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(bytes.as_ptr(), allocation);

    bincode::encode_into_vec((1u8, 2u8), &mut bytes, config).unwrap();
    assert_eq!(bytes, [1, 2]);
    assert_eq!(bytes.as_ptr(), allocation);
}

#[test]
fn size_query_and_slice_encoding_match_allocating_encoding() {
    let value = (
        7u8,
        [
            0.0f32,
            -0.0,
            1.25,
            f32::INFINITY,
            f32::from_bits(0x7fc0_1234),
        ],
    );
    let config = bincode::config::standard();
    let expected = bincode::encode_to_vec(value, config).unwrap();
    assert_eq!(
        bincode::encoded_size(value, config).unwrap(),
        expected.len()
    );

    let mut destination = vec![0xa5; expected.len() + 8];
    let used = bincode::encode_into_slice(value, &mut destination, config).unwrap();
    assert_eq!(used, expected.len());
    assert_eq!(&destination[..used], expected);
    assert_eq!(&destination[used..], &[0xa5; 8]);

    let mut short = vec![0xa5; expected.len() - 1];
    let error = bincode::encode_into_slice(value, &mut short, config).unwrap_err();
    assert!(matches!(error, bincode::error::EncodeError::UnexpectedEnd));
    assert_eq!(short[0], 7);
    assert!(short[1..].iter().all(|byte| *byte == 0xa5));

    let big_endian = bincode::config::standard().with_big_endian();
    let expected = bincode::encode_to_vec(value, big_endian).unwrap();
    let mut destination = vec![0; expected.len()];
    assert_eq!(
        bincode::encoded_size(value, big_endian).unwrap(),
        expected.len()
    );
    assert_eq!(
        bincode::encode_into_slice(value, &mut destination, big_endian).unwrap(),
        expected.len()
    );
    assert_eq!(destination, expected);

    assert_eq!(bincode::encode_into_slice((), &mut [], config).unwrap(), 0);
}

#[test]
fn reusable_vector_decode_matches_allocating_decode() {
    let values = vec![0u64, 250, 251, u64::from(u16::MAX) + 1, u64::MAX];
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::decode_from_slice::<Vec<u64>, _>(&encoded, config).unwrap();
    let mut decoded = Vec::with_capacity(values.len() * 2);
    decoded.extend_from_slice(&[42; 3]);
    let allocation = decoded.as_ptr();

    let used = bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (expected.0.as_slice(), expected.1)
    );
    assert_eq!(decoded.as_ptr(), allocation);

    let shorter = bincode::encode_to_vec(vec![7u64, 8], config).unwrap();
    let used = bincode::decode_from_slice_into_vec(&shorter, &mut decoded, config).unwrap();
    assert_eq!(decoded, [7, 8]);
    assert_eq!(used, shorter.len());
    assert_eq!(decoded.as_ptr(), allocation);
}

#[test]
fn reusable_vector_decode_preserves_limits_and_error_prefix() {
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(vec![1u64, 251, 65_536], config).unwrap();
    let mut decoded = vec![99u64];
    let error =
        bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config.with_limit::<16>())
            .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());

    let truncated = [3, 1, 251, 0];
    let error = bincode::decode_from_slice_into_vec(&truncated, &mut decoded, config).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { additional: 1 }
    ));
    assert_eq!(decoded, [1]);
}

#[test]
fn reusable_string_vector_decode_reuses_element_allocations() {
    let values = vec![
        "chart-alpha-dead-sync".repeat(4),
        "chart-beta-dead-sync".repeat(4),
        "chart-gamma-dead-sync".repeat(4),
    ];
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::decode_from_slice::<Vec<String>, _>(&encoded, config).unwrap();
    let mut decoded = (0..values.len())
        .map(|_| String::with_capacity(256))
        .collect::<Vec<_>>();
    for value in &mut decoded {
        value.push_str("discarded");
    }
    let outer_allocation = decoded.as_ptr();
    let element_allocations = decoded
        .iter()
        .map(|value| value.as_ptr())
        .collect::<Vec<_>>();

    let used = bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (expected.0.as_slice(), expected.1)
    );
    assert_eq!(decoded.as_ptr(), outer_allocation);
    assert!(decoded
        .iter()
        .zip(&element_allocations)
        .all(|(value, allocation)| value.as_ptr() == *allocation));

    let shorter_values = values[..2].to_vec();
    let shorter = bincode::encode_to_vec(&shorter_values, config).unwrap();
    let used = bincode::decode_from_slice_into_vec(&shorter, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (shorter_values.as_slice(), shorter.len())
    );
    assert_eq!(decoded.as_ptr(), outer_allocation);
    assert!(decoded
        .iter()
        .zip(&element_allocations)
        .all(|(value, allocation)| value.as_ptr() == *allocation));

    let truncated = &encoded[..encoded.len() - b"trailing".len() - 1];
    let error = bincode::decode_from_slice_into_vec(truncated, &mut decoded, config).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { .. }
    ));
    assert_eq!(decoded, values[..2]);
    assert_eq!(decoded[0].as_ptr(), element_allocations[0]);
    assert_eq!(decoded[1].as_ptr(), element_allocations[1]);

    let error =
        bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config.with_limit::<16>())
            .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());
}

#[test]
fn reusable_nested_byte_vectors_reuse_inner_allocations() {
    let values = (0..16usize)
        .map(|outer| {
            (0..96usize)
                .map(|inner| outer.wrapping_mul(31).wrapping_add(inner) as u8)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&values, config).unwrap();
    let expected = bincode::decode_from_slice::<Vec<Vec<u8>>, _>(&encoded, config).unwrap();
    let mut decoded = (0..values.len())
        .map(|_| {
            let mut value = Vec::with_capacity(192);
            value.extend_from_slice(b"discarded");
            value
        })
        .collect::<Vec<_>>();
    let outer_allocation = decoded.as_ptr();
    let inner_allocations = decoded
        .iter()
        .map(|value| value.as_ptr())
        .collect::<Vec<_>>();

    let used = bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (expected.0.as_slice(), expected.1)
    );
    assert_eq!(decoded.as_ptr(), outer_allocation);
    assert!(decoded
        .iter()
        .zip(inner_allocations)
        .all(|(value, allocation)| value.as_ptr() == allocation));
}

#[test]
fn reusable_nested_numeric_vectors_reuse_inner_allocations_across_configs() {
    let values = (0..16u64)
        .map(|outer| {
            (0..64u64)
                .map(|inner| outer.wrapping_mul(1_000_003) ^ inner.rotate_left(17))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    macro_rules! check_config {
        ($config:expr) => {{
            let config = $config;
            let encoded = bincode::encode_to_vec(&values, config).unwrap();
            let expected =
                bincode::decode_from_slice::<Vec<Vec<u64>>, _>(&encoded, config).unwrap();
            let mut decoded = (0..values.len())
                .map(|_| {
                    let mut value = Vec::with_capacity(128);
                    value.extend_from_slice(&[u64::MAX; 4]);
                    value
                })
                .collect::<Vec<_>>();
            let outer_allocation = decoded.as_ptr();
            let inner_allocations = decoded
                .iter()
                .map(|value| value.as_ptr())
                .collect::<Vec<_>>();

            let used = bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
            assert_eq!(
                (decoded.as_slice(), used),
                (expected.0.as_slice(), expected.1)
            );
            assert_eq!(decoded.as_ptr(), outer_allocation);
            assert!(decoded
                .iter()
                .zip(inner_allocations)
                .all(|(value, allocation)| value.as_ptr() == allocation));
        }};
    }

    check_config!(bincode::config::standard());
    check_config!(bincode::config::standard().with_big_endian());
    check_config!(bincode::config::legacy());
    check_config!(bincode::config::legacy().with_big_endian());
}

#[test]
fn reusable_nested_vectors_preserve_complete_prefix_on_error() {
    let values = vec![vec![1u8, 2, 3], vec![4, 5, 6, 7]];
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&values, config).unwrap();
    let truncated = &encoded[..encoded.len() - 1];
    let mut decoded = vec![vec![99; 16], vec![88; 16]];
    let outer_allocation = decoded.as_ptr();
    let first_allocation = decoded[0].as_ptr();

    let error = bincode::decode_from_slice_into_vec(truncated, &mut decoded, config).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { additional: 1 }
    ));
    assert_eq!(decoded, values[..1]);
    assert_eq!(decoded.as_ptr(), outer_allocation);
    assert_eq!(decoded[0].as_ptr(), first_allocation);
}

#[test]
fn deep_reuse_dispatch_preserves_same_size_generic_elements() {
    let values = vec![[1u64, 2, 3], [u64::MAX, 250, 251]];
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&values, config).unwrap();
    let mut decoded = vec![[99u64; 3]; 4];
    let allocation = decoded.as_ptr();

    let used = bincode::decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (values.as_slice(), encoded.len())
    );
    assert_eq!(decoded.as_ptr(), allocation);
}

#[test]
fn reusable_string_decode_matches_allocating_decode() {
    let value = "DeadSync allocation reuse ".repeat(4_096);
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&value, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::decode_from_slice::<String, _>(&encoded, config).unwrap();
    let mut decoded = String::with_capacity(value.len() * 2);
    decoded.push_str("discarded contents");
    let allocation = decoded.as_ptr();

    let used = bincode::decode_from_slice_into_string(&encoded, &mut decoded, config).unwrap();
    assert_eq!((&decoded, used), (&expected.0, expected.1));
    assert_eq!(decoded.as_ptr(), allocation);

    let shorter = bincode::encode_to_vec("short", config).unwrap();
    let used = bincode::decode_from_slice_into_string(&shorter, &mut decoded, config).unwrap();
    assert_eq!(decoded, "short");
    assert_eq!(used, shorter.len());
    assert_eq!(decoded.as_ptr(), allocation);

    let error =
        bincode::decode_from_slice_into_string(&[1, 0xff], &mut decoded, config).unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::Utf8 { .. }));
    assert!(decoded.is_empty());

    let error =
        bincode::decode_from_slice_into_string(&[3, b'a'], &mut decoded, config).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { additional: 2 }
    ));
    assert!(decoded.is_empty());
}

#[test]
fn reusable_hash_map_decode_matches_allocating_decode() {
    let values = (0..128u64)
        .map(|value| (value.wrapping_mul(1_000_003), value.rotate_left(17)))
        .collect::<HashMap<_, _>>();
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::decode_from_slice::<HashMap<u64, u64>, _>(&encoded, config).unwrap();
    let mut decoded = HashMap::with_capacity(values.len() * 2);
    decoded.insert(u64::MAX, 42);
    let capacity = decoded.capacity();

    let used = bincode::decode_from_slice_into_hash_map(&encoded, &mut decoded, config).unwrap();
    assert_eq!((&decoded, used), (&expected.0, expected.1));
    assert_eq!(decoded.capacity(), capacity);

    let error =
        bincode::decode_from_slice_into_hash_map(&encoded, &mut decoded, config.with_limit::<16>())
            .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());
    assert_eq!(decoded.capacity(), capacity);
}

#[test]
fn reusable_hash_set_decode_matches_allocating_decode() {
    let values = (0..256u64)
        .map(|value| value.wrapping_mul(1_000_003))
        .collect::<HashSet<_>>();
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::decode_from_slice::<HashSet<u64>, _>(&encoded, config).unwrap();
    let mut decoded = HashSet::with_capacity(values.len() * 2);
    decoded.insert(u64::MAX);
    let capacity = decoded.capacity();

    let used = bincode::decode_from_slice_into_hash_set(&encoded, &mut decoded, config).unwrap();
    assert_eq!((&decoded, used), (&expected.0, expected.1));
    assert_eq!(decoded.capacity(), capacity);

    let error =
        bincode::decode_from_slice_into_hash_set(&encoded, &mut decoded, config.with_limit::<16>())
            .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());
    assert_eq!(decoded.capacity(), capacity);
}

#[test]
fn borrowed_decode_points_into_source() {
    let value = BorrowedPayload {
        name: "allocation-free",
        bytes: b"borrowed bytes",
    };
    let encoded = bincode::encode_to_vec(&value, bincode::config::standard()).unwrap();
    let (decoded, used): (BorrowedPayload<'_>, _) =
        bincode::borrow_decode_from_slice(&encoded, bincode::config::standard()).unwrap();

    assert_eq!(decoded, value);
    assert_eq!(used, encoded.len());
    let source = encoded.as_ptr_range();
    assert!(source.contains(&decoded.name.as_ptr()));
    assert!(source.contains(&decoded.bytes.as_ptr()));
}

#[test]
fn reusable_borrowed_vector_decode_reuses_storage_and_source() {
    let values = (0..128)
        .map(|index| format!("chart-{index:03}-dead-sync"))
        .collect::<Vec<_>>();
    let shorter_values = vec!["short".to_owned(), "tail".to_owned()];
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let shorter = bincode::encode_to_vec(&shorter_values, config).unwrap();
    let expected = bincode::borrow_decode_from_slice::<Vec<&str>, _>(&encoded, config).unwrap();
    let mut decoded = Vec::with_capacity(values.len() * 2);
    decoded.push("discarded");
    let allocation = decoded.as_ptr();

    let used = bincode::borrow_decode_from_slice_into_vec(&encoded, &mut decoded, config).unwrap();
    assert_eq!(
        (decoded.as_slice(), used),
        (expected.0.as_slice(), expected.1)
    );
    assert_eq!(decoded.as_ptr(), allocation);
    let source = encoded.as_ptr_range();
    assert!(decoded.iter().all(|value| source.contains(&value.as_ptr())));

    let used = bincode::borrow_decode_from_slice_into_vec(&shorter, &mut decoded, config).unwrap();
    assert_eq!(decoded, ["short", "tail"]);
    assert_eq!(used, shorter.len());
    assert_eq!(decoded.as_ptr(), allocation);
    let source = shorter.as_ptr_range();
    assert!(decoded.iter().all(|value| source.contains(&value.as_ptr())));
}

#[test]
fn reusable_borrowed_vector_keeps_initialized_prefix_droppable_on_error() {
    BORROWED_DROPS.store(0, Ordering::SeqCst);
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(vec!["complete", "truncated"], config).unwrap();
    let truncated = &encoded[..encoded.len() - 1];
    let mut decoded = Vec::with_capacity(4);
    decoded.push(DropTrackedBorrowed("discarded"));
    let allocation = decoded.as_ptr();

    let error =
        bincode::borrow_decode_from_slice_into_vec(truncated, &mut decoded, config).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { .. }
    ));
    assert_eq!(BORROWED_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].0, "complete");
    assert_eq!(decoded.as_ptr(), allocation);

    drop(decoded);
    assert_eq!(BORROWED_DROPS.load(Ordering::SeqCst), 2);
}

#[test]
fn reusable_borrowed_hash_map_decode_reuses_storage_and_source() {
    let values = (0..128u64)
        .map(|value| (format!("chart-{value:03}"), value.rotate_left(17)))
        .collect::<HashMap<_, _>>();
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected =
        bincode::borrow_decode_from_slice::<HashMap<&str, u64>, _>(&encoded, config).unwrap();
    let mut decoded = HashMap::with_capacity(values.len() * 2);
    decoded.insert("discarded", u64::MAX);
    let capacity = decoded.capacity();

    let used =
        bincode::borrow_decode_from_slice_into_hash_map(&encoded, &mut decoded, config).unwrap();
    assert_eq!((&decoded, used), (&expected.0, expected.1));
    assert_eq!(decoded.capacity(), capacity);
    let source = encoded.as_ptr_range();
    assert!(decoded.keys().all(|key| source.contains(&key.as_ptr())));

    let error = bincode::borrow_decode_from_slice_into_hash_map(
        &encoded,
        &mut decoded,
        config.with_limit::<16>(),
    )
    .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());
    assert_eq!(decoded.capacity(), capacity);
}

#[test]
fn reusable_borrowed_hash_set_decode_reuses_storage_and_source() {
    let values = (0..256u64)
        .map(|value| format!("chart-{value:03}-{}", value.rotate_left(11)))
        .collect::<HashSet<_>>();
    let config = bincode::config::standard();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    encoded.extend_from_slice(b"trailing");
    let expected = bincode::borrow_decode_from_slice::<HashSet<&str>, _>(&encoded, config).unwrap();
    let mut decoded = HashSet::with_capacity(values.len() * 2);
    decoded.insert("discarded");
    let capacity = decoded.capacity();

    let used =
        bincode::borrow_decode_from_slice_into_hash_set(&encoded, &mut decoded, config).unwrap();
    assert_eq!((&decoded, used), (&expected.0, expected.1));
    assert_eq!(decoded.capacity(), capacity);
    let source = encoded.as_ptr_range();
    assert!(decoded.iter().all(|value| source.contains(&value.as_ptr())));

    let error = bincode::borrow_decode_from_slice_into_hash_set(
        &encoded,
        &mut decoded,
        config.with_limit::<16>(),
    )
    .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
    assert!(decoded.is_empty());
    assert_eq!(decoded.capacity(), capacity);
}

#[test]
fn owned_byte_decode_keeps_fallback_reader_behavior() {
    struct NoPeek<'a>(&'a [u8]);

    impl bincode::de::read::Reader for NoPeek<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
            if bytes.len() > self.0.len() {
                return Err(bincode::error::DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.0.len(),
                });
            }
            let (read, remaining) = self.0.split_at(bytes.len());
            bytes.copy_from_slice(read);
            self.0 = remaining;
            Ok(())
        }
    }

    let value = (0u8..=255).cycle().take(16_384).collect::<Vec<_>>();
    let encoded = bincode::encode_to_vec(&value, bincode::config::standard()).unwrap();
    let reader = NoPeek(&encoded);
    let mut decoder = bincode::de::DecoderImpl::new(reader, bincode::config::standard(), ());

    assert_eq!(Vec::<u8>::decode(&mut decoder).unwrap(), value);

    let floats = vec![
        0.0f32,
        -0.0,
        1.25,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
    ];
    let encoded = bincode::encode_to_vec(&floats, bincode::config::standard()).unwrap();
    let reader = NoPeek(&encoded);
    let mut decoder = bincode::de::DecoderImpl::new(reader, bincode::config::standard(), ());
    let decoded = Vec::<f32>::decode(&mut decoder).unwrap();
    assert_eq!(
        decoded
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        floats
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );

    let integers = vec![0u64, 250, 251, 65_536, u64::MAX];
    let encoded = bincode::encode_to_vec(&integers, bincode::config::standard()).unwrap();
    let reader = NoPeek(&encoded);
    let mut decoder = bincode::de::DecoderImpl::new(reader, bincode::config::standard(), ());
    assert_eq!(Vec::<u64>::decode(&mut decoder).unwrap(), integers);

    let float_array = [
        0.0f32,
        -0.0,
        1.25,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
    ];
    let encoded = bincode::encode_to_vec(float_array, bincode::config::standard()).unwrap();
    let reader = NoPeek(&encoded);
    let mut decoder = bincode::de::DecoderImpl::new(reader, bincode::config::standard(), ());
    let decoded = <[f32; 5]>::decode(&mut decoder).unwrap();
    assert!(decoded
        .iter()
        .zip(float_array)
        .all(|(decoded, expected)| decoded.to_bits() == expected.to_bits()));
}

#[test]
fn numeric_batch_rejects_short_peek_buffer() {
    struct ShortPeek<'a>(&'a [u8]);

    impl bincode::de::read::Reader for ShortPeek<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
            if bytes.len() > self.0.len() {
                return Err(bincode::error::DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.0.len(),
                });
            }
            let (read, remaining) = self.0.split_at(bytes.len());
            bytes.copy_from_slice(read);
            self.0 = remaining;
            Ok(())
        }

        fn peek_read(&mut self, n: usize) -> Option<&[u8]> {
            let returned = n - usize::from(n > 1);
            self.0.get(..returned)
        }

        fn consume(&mut self, n: usize) {
            self.0 = self.0.get(n..).unwrap_or_default();
        }
    }

    let encoded =
        bincode::encode_to_vec(vec![0.0f32, 1.0, 2.0], bincode::config::standard()).unwrap();
    let mut decoder =
        bincode::de::DecoderImpl::new(ShortPeek(&encoded), bincode::config::standard(), ());
    let error = Vec::<f32>::decode(&mut decoder).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { additional: 1 }
    ));

    let encoded = bincode::encode_to_vec([0.0f32, 1.0, 2.0], bincode::config::standard()).unwrap();
    let mut decoder =
        bincode::de::DecoderImpl::new(ShortPeek(&encoded), bincode::config::standard(), ());
    let error = <[f32; 3]>::decode(&mut decoder).unwrap_err();
    assert!(matches!(
        error,
        bincode::error::DecodeError::UnexpectedEnd { additional: 1 }
    ));
}

#[test]
fn numeric_batches_keep_wire_format() {
    let floats = vec![0.0f32, -0.0, 1.25, f32::from_bits(0x7fc0_1234)];
    let expected = [
        4, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 160, 63, 0x34, 0x12, 0xc0, 0x7f,
    ];
    let encoded = bincode::encode_to_vec(&floats, bincode::config::standard()).unwrap();
    assert_eq!(encoded, expected);
    let decoded = bincode::decode_from_slice::<Vec<f32>, _>(&expected, bincode::config::standard())
        .unwrap()
        .0;
    assert_eq!(
        decoded
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        floats
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );

    let integers = vec![0x0102u16, 0x0304, 0x0506];
    let expected = [3, 0, 0, 0, 0, 0, 0, 0, 2, 1, 4, 3, 6, 5];
    let config = bincode::config::legacy();
    assert_eq!(bincode::encode_to_vec(&integers, config).unwrap(), expected);
    assert_eq!(
        bincode::decode_from_slice::<Vec<u16>, _>(&expected, config)
            .unwrap()
            .0,
        integers
    );
}

#[test]
fn numeric_array_batches_keep_wire_format_and_limits() {
    let floats = [0.0f32, -0.0, 1.25, f32::from_bits(0x7fc0_1234)];
    let expected = [
        0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 160, 63, 0x34, 0x12, 0xc0, 0x7f,
    ];
    let config = bincode::config::standard();
    assert_eq!(bincode::encode_to_vec(floats, config).unwrap(), expected);
    let decoded = bincode::decode_from_slice::<[f32; 4], _>(&expected, config)
        .unwrap()
        .0;
    assert!(decoded
        .iter()
        .zip(floats)
        .all(|(decoded, expected)| decoded.to_bits() == expected.to_bits()));
    let borrowed = bincode::borrow_decode_from_slice::<[f32; 4], _>(&expected, config)
        .unwrap()
        .0;
    assert!(borrowed
        .iter()
        .zip(floats)
        .all(|(decoded, expected)| decoded.to_bits() == expected.to_bits()));

    let fixed = [0x0102_0304_0506_0708u64, 0x1112_1314_1516_1718, u64::MAX];
    for big_endian in [false, true] {
        if big_endian {
            let config = bincode::config::legacy().with_big_endian();
            let encoded = bincode::encode_to_vec(fixed, config).unwrap();
            assert_eq!(
                bincode::decode_from_slice::<[u64; 3], _>(&encoded, config)
                    .unwrap()
                    .0,
                fixed
            );
        } else {
            let config = bincode::config::legacy();
            let encoded = bincode::encode_to_vec(fixed, config).unwrap();
            assert_eq!(
                bincode::decode_from_slice::<[u64; 3], _>(&encoded, config)
                    .unwrap()
                    .0,
                fixed
            );
        }
    }

    let error = bincode::decode_from_slice::<[f32; 4], _>(&expected, config.with_limit::<15>())
        .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
}

#[test]
fn specialized_varint_vectors_round_trip_boundaries() {
    macro_rules! round_trip {
        ($values:expr) => {{
            let values = $values;
            {
                let config = bincode::config::standard();
                let encoded = bincode::encode_to_vec(&values, config).unwrap();
                let decoded = bincode::decode_from_slice(&encoded, config).unwrap();
                assert_eq!(decoded, (values.clone(), encoded.len()));
            }
            {
                let config = bincode::config::standard().with_big_endian();
                let encoded = bincode::encode_to_vec(&values, config).unwrap();
                let decoded = bincode::decode_from_slice(&encoded, config).unwrap();
                assert_eq!(decoded, (values.clone(), encoded.len()));
            }
        }};
    }

    round_trip!(vec![0u16, 250, 251, u16::MAX]);
    round_trip!(vec![0u32, 250, 251, u32::from(u16::MAX) + 1, u32::MAX]);
    round_trip!(vec![
        0u64,
        250,
        251,
        u64::from(u16::MAX) + 1,
        u64::from(u32::MAX) + 1,
        u64::MAX,
    ]);
    round_trip!(vec![0i64, -1, 1, i64::MIN, i64::MAX]);
    round_trip!(vec![0u128, u128::from(u64::MAX) + 1, u128::MAX]);
    round_trip!(vec![0i128, -1, 1, i128::MIN, i128::MAX]);
    round_trip!(vec![0usize, 250, 251, usize::MAX]);
    round_trip!(vec![0isize, -1, 1, isize::MIN, isize::MAX]);
}

#[test]
fn borrowed_decode_rejects_invalid_text() {
    assert!(matches!(
        bincode::borrow_decode_from_slice::<&str, _>(&[1, 0xff], bincode::config::standard()),
        Err(bincode::error::DecodeError::Utf8 { .. })
    ));
}
