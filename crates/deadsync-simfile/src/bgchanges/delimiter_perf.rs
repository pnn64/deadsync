use super::*;


#[test]
fn delimiter_matches_scalar_at_every_offset() {
    for len in 0..256 {
        for offset in 0..=len {
            for delimiter in *b"=,x" {
                let mut input = "é".repeat(len);
                input.insert(offset * 2, char::from(delimiter));
                for start in [0, offset * 2, input.len()] {
                    let expected = input.as_bytes()[start..]
                        .iter()
                        .position(|byte| matches!(byte, b'=' | b','))
                        .map(|index| (start + index, input.as_bytes()[start + index]));
                    assert_eq!(next_bgchange_delimiter(&input, start), expected);
                }
            }
        }
    }
}
