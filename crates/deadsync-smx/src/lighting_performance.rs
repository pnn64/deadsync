use super::{BYTES_PER_PAD_25, PLAYER_LIGHT_FRAME_BYTES};


#[test]
fn sdk_shape_matches_kernel_fixture() {
    assert_eq!(BYTES_PER_PAD_25, 675);
    assert_eq!(PLAYER_LIGHT_FRAME_BYTES, 1350);
}
