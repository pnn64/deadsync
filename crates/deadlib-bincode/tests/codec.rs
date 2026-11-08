use bincode::{Decode, Encode};
use std::collections::{HashMap, HashSet};

#[test]
fn signed_zigzag_matches_unsigned_wire_across_widths_and_endianness() {
    macro_rules! check {
        ($signed:ty, $unsigned:ty, $values:expr) => {
            for value in $values {
                // Independent arithmetic definition, including the signed minimum.
                let wire = if value < 0 {
                    !(value as $unsigned) * 2 + 1
                } else {
                    (value as $unsigned) * 2
                };
                for big in [false, true] {
                    let mut actual = [0xa5; 17];
                    let mut expected = [0xa5; 17];
                    macro_rules! compare {
                        ($config:expr) => {{
                            let config = $config;
                            let used =
                                bincode::encode_into_slice(value, &mut actual, config).unwrap();
                            let expected_used =
                                bincode::encode_into_slice(wire, &mut expected, config).unwrap();
                            assert_eq!((actual, used), (expected, expected_used));
                            assert_eq!(
                                bincode::decode_from_slice::<$signed, _>(&actual[..used], config)
                                    .unwrap(),
                                (value, used)
                            );
                        }};
                    }
                    if big {
                        compare!(bincode::config::standard().with_big_endian());
                    } else {
                        compare!(bincode::config::standard());
                    }
                }
            }
        };
    }

    check!(i16, u16, i16::MIN..=i16::MAX);
    let mut state = 0x1234_5678_9abc_def0_1357_9bdf_2468_ace0u128;
    let values = std::iter::once(i128::MIN)
        .chain(std::iter::once(i128::MAX))
        .chain((0..4096).map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as i128
        }))
        .collect::<Vec<_>>();
    check!(i32, u32, values.iter().map(|&value| value as i32));
    check!(i64, u64, values.iter().map(|&value| value as i64));
    check!(i128, u128, values);
}

#[derive(Debug, PartialEq, Encode, Decode)]
struct PersistedData {
    enabled: bool,
    signed: (i8, i16, i32),
    wide_signed: i64,
    unsigned: (u8, u16, u32),
    wide_unsigned: u64,
    index: usize,
    floats: (f32, f64),
    name: String,
    aliases: Vec<String>,
    samples: Box<[u16]>,
    optional: Option<[u8; 4]>,
    by_name: HashMap<String, u32>,
    seen: HashSet<u16>,
    state: State,
}

#[derive(Debug, PartialEq, Encode, Decode)]
enum State {
    Idle,
    Active { lane: u8, offset: i32 },
}

fn persisted_data() -> PersistedData {
    PersistedData {
        enabled: true,
        signed: (-1, -250, -70_000),
        wide_signed: -5_000_000_000,
        unsigned: (1, 250, 70_000),
        wide_unsigned: 5_000_000_000,
        index: 42,
        floats: (1.25, -0.5),
        name: "DeadSync".into(),
        aliases: vec!["DS".into(), "deadlib".into()],
        samples: vec![1, 2, 3].into_boxed_slice(),
        optional: Some([1, 2, 3, 4]),
        by_name: HashMap::from([("score".into(), 500)]),
        seen: HashSet::from([3, 7]),
        state: State::Active {
            lane: 2,
            offset: -15,
        },
    }
}

#[test]
fn dead_sync_surface_round_trips() {
    let input = persisted_data();
    let bytes = bincode::encode_to_vec(&input, bincode::config::standard()).unwrap();
    let (decoded, used): (PersistedData, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(decoded, input);
    assert_eq!(used, bytes.len());
}

#[test]
fn decode_limit_rejects_large_container() {
    let bytes = bincode::encode_to_vec(vec![0u64; 100], bincode::config::standard()).unwrap();
    let error = bincode::decode_from_slice::<Vec<u64>, _>(
        &bytes,
        bincode::config::standard().with_limit::<16>(),
    )
    .unwrap_err();
    assert!(matches!(error, bincode::error::DecodeError::LimitExceeded));
}

#[test]
fn malformed_values_are_rejected() {
    assert!(matches!(
        bincode::decode_from_slice::<bool, _>(&[2], bincode::config::standard()),
        Err(bincode::error::DecodeError::InvalidBooleanValue(2))
    ));
    assert!(matches!(
        bincode::decode_from_slice::<State, _>(&[2], bincode::config::standard()),
        Err(bincode::error::DecodeError::UnexpectedVariant { .. })
    ));
    assert!(matches!(
        bincode::decode_from_slice::<String, _>(&[1, 0xff], bincode::config::standard()),
        Err(bincode::error::DecodeError::Utf8 { .. })
    ));
}
