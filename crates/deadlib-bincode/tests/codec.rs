use bincode::{Decode, Encode};
use std::collections::{HashMap, HashSet};

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
