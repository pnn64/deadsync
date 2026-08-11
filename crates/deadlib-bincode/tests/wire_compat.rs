use bincode::{Decode, Encode};

// Generated with the crates.io bincode 2.0.1 artifact and standard config.
const BINCODE_2_FIXTURE: &[u8] = &[
    0x07, 0x01, 0xfb, 0xf4, 0x01, 0x04, b'D', b'S', b'Y', b'N', 0x03, 0x00, 0xfa, 0xfb, 0xfb, 0x00,
    0x01, 0x2a,
];

#[derive(Debug, PartialEq, Encode, Decode)]
struct WireFixture {
    schema: u8,
    enabled: bool,
    score: u32,
    name: String,
    values: Vec<u16>,
    state: FixtureState,
}

#[derive(Debug, PartialEq, Encode, Decode)]
enum FixtureState {
    Idle,
    Active(u16),
}

fn fixture() -> WireFixture {
    WireFixture {
        schema: 7,
        enabled: true,
        score: 500,
        name: "DSYN".to_owned(),
        values: vec![0, 250, 251],
        state: FixtureState::Active(42),
    }
}

#[test]
fn standard_config_matches_bincode_2_fixture() {
    let bytes = bincode::encode_to_vec(fixture(), bincode::config::standard()).unwrap();
    assert_eq!(bytes, BINCODE_2_FIXTURE);
}

#[test]
fn standard_config_reads_bincode_2_fixture() {
    let (decoded, used): (WireFixture, _) =
        bincode::decode_from_slice(BINCODE_2_FIXTURE, bincode::config::standard()).unwrap();
    assert_eq!(decoded, fixture());
    assert_eq!(used, BINCODE_2_FIXTURE.len());
}
