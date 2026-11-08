use bincode::{
    de::{read::Reader, Decoder},
    Decode,
};

#[test]
fn large_boolean_blocks_preserve_error_prefix_and_reader_position() {
    let config = bincode::config::standard();
    let values: Vec<bool> = (0..769).map(|i| i % 3 == 0).collect();
    let mut encoded = bincode::encode_to_vec(&values, config).unwrap();
    let prefix = encoded.len() - values.len();
    encoded.push(0xa5);
    let mut output = Vec::<bool>::with_capacity(values.len());
    for index in 0..values.len() {
        for invalid in [2, 128, 255] {
            encoded[prefix + index] = invalid;
            // Two invalid bytes exercise first-error ordering across blocks.
            encoded[prefix + values.len() - 1] = invalid;
            let error =
                bincode::decode_from_slice_into_vec(&encoded, &mut output, config).unwrap_err();
            assert!(matches!(
                error, bincode::error::DecodeError::InvalidBooleanValue(v) if v == invalid
            ));
            assert_eq!(output, values[..index]);

            let mut decoder = bincode::de::DecoderImpl::new(
                bincode::de::read::SliceReader::new(&encoded),
                config,
                (),
            );
            assert!(matches!(
                Vec::<bool>::decode(&mut decoder),
                Err(bincode::error::DecodeError::InvalidBooleanValue(v)) if v == invalid
            ));
            let mut next = [0];
            decoder.reader().read(&mut next).unwrap();
            assert_eq!(next[0], encoded[prefix + index + 1]);
            encoded[prefix + index] = u8::from(values[index]);
            encoded[prefix + values.len() - 1] = u8::from(values[values.len() - 1]);
        }
    }
}

#[test]
fn boolean_validation_preserves_first_error_and_reader_position() {
    let config = bincode::config::standard();
    for index in 0..193 {
        for invalid in 2..=255u8 {
            let mut bytes: Vec<u8> = (0..193).map(|i| (i % 2) as u8).collect();
            bytes[index] = invalid;
            // A later error must not supersede the first error's value or prefix.
            bytes[192] = if index == 192 { invalid } else { 255 };
            let mut encoded = vec![193];
            encoded.extend_from_slice(&bytes);
            let mut output = vec![true; 100];
            let error =
                bincode::decode_from_slice_into_vec(&encoded, &mut output, config).unwrap_err();
            assert!(
                matches!(error, bincode::error::DecodeError::InvalidBooleanValue(v) if v == invalid)
            );
            assert_eq!(
                output,
                bytes[..index].iter().map(|&v| v != 0).collect::<Vec<_>>()
            );

            let mut decoder = bincode::de::DecoderImpl::new(
                bincode::de::read::SliceReader::new(&bytes),
                config,
                (),
            );
            let error = <[bool; 193]>::decode(&mut decoder).unwrap_err();
            assert!(
                matches!(error, bincode::error::DecodeError::InvalidBooleanValue(v) if v == invalid)
            );
            let mut next = [0];
            if index < 192 {
                decoder.reader().read(&mut next).unwrap();
                assert_eq!(next[0], bytes[index + 1]);
            } else {
                assert!(decoder.reader().read(&mut next).is_err());
            }
        }
    }
}

#[test]
fn boolean_validation_covers_word_tails_and_borrowed_decoding() {
    macro_rules! check_array {
        ($len:literal) => {
            let values: [bool; $len] = std::array::from_fn(|i| i % 3 == 0);
            let config = bincode::config::standard();
            let encoded = bincode::encode_to_vec(values, config).unwrap();
            assert_eq!(
                bincode::decode_from_slice::<[bool; $len], _>(&encoded, config).unwrap(),
                (values, $len)
            );
            assert_eq!(
                bincode::borrow_decode_from_slice::<[bool; $len], _>(&encoded, config).unwrap(),
                (values, $len)
            );
            for (end, _) in encoded.iter().enumerate() {
                assert!(
                    bincode::decode_from_slice::<[bool; $len], _>(&encoded[..end], config).is_err()
                );
            }
        };
    }
    check_array!(0);
    check_array!(1);
    check_array!(7);
    check_array!(8);
    check_array!(9);
    check_array!(15);
    check_array!(16);
    check_array!(17);
    check_array!(31);
    check_array!(32);
    check_array!(33);
    check_array!(255);
    check_array!(256);
    check_array!(257);
    check_array!(511);
    check_array!(512);
    check_array!(513);
    check_array!(63);
    check_array!(64);
    check_array!(65);
    check_array!(127);
    check_array!(128);
    check_array!(129);
    check_array!(193);

    for len in 0..=129 {
        let values: Vec<_> = (0..len).map(|i| i % 3 == 0).collect();
        let config = bincode::config::standard();
        let encoded = bincode::encode_to_vec(&values, config).unwrap();
        assert_eq!(
            bincode::decode_from_slice::<Vec<bool>, _>(&encoded, config).unwrap(),
            (values.clone(), encoded.len())
        );
        assert_eq!(
            bincode::borrow_decode_from_slice::<Vec<bool>, _>(&encoded, config).unwrap(),
            (values, encoded.len())
        );
    }
}
