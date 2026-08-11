use bincode::{BorrowDecode, Decode, Encode};

#[derive(Debug, PartialEq, Encode, BorrowDecode)]
struct BorrowedPayload<'a> {
    name: &'a str,
    bytes: &'a [u8],
}

#[test]
fn reusable_encoding_matches_allocating_encoding() {
    let value = (
        "DeadSync persistence",
        vec![0u64, 250, 251, u32::MAX as u64, u64::MAX],
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
fn borrowed_decode_rejects_invalid_text() {
    assert!(matches!(
        bincode::borrow_decode_from_slice::<&str, _>(&[1, 0xff], bincode::config::standard()),
        Err(bincode::error::DecodeError::Utf8 { .. })
    ));
}
