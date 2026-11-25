#[cfg(any(not(target_feature = "bmi2"), test))]
use super::inflate::{Inflater, WideTables};
use super::{Bits, Error, Output};

#[inline(never)]
pub(super) fn inflate<O: Output>(
    bits: &mut Bits<'_>,
    output: O,
    written: &mut usize,
    fixed: bool,
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    if std::is_x86_feature_detected!("bmi2") {
        // SAFETY: BMI2 availability is checked above. The implementation uses
        // checked slices and has no unsafe memory operations.
        unsafe { bmi2(bits, output, written, fixed, window, limit) }
    } else {
        super::inflate_sized_codes::<O, true>(bits, output, written, fixed, window, limit)
    }
}

#[target_feature(enable = "bmi2")]
unsafe fn bmi2<O: Output>(
    bits: &mut Bits<'_>,
    output: O,
    written: &mut usize,
    fixed: bool,
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    super::inflate_sized_codes::<O, true>(bits, output, written, fixed, window, limit)
}

#[cfg(not(target_feature = "bmi2"))]
#[inline]
pub(super) fn wide_codes(
    inflater: &mut Inflater,
    bits: &mut Bits<'_>,
    output: &mut [u8],
    written: &mut usize,
    tables: &mut Vec<WideTables>,
) -> Result<bool, Error> {
    if std::is_x86_feature_detected!("bmi2") {
        // SAFETY: BMI2 availability is checked above. The decoder uses the
        // same checked slices on both paths.
        unsafe { wide_codes_bmi2(inflater, bits, output, written, tables) }
    } else {
        inflater.decode_wide_codes(bits, output, written, tables)
    }
}

#[cfg(any(not(target_feature = "bmi2"), test))]
#[target_feature(enable = "bmi2")]
unsafe fn wide_codes_bmi2(
    inflater: &mut Inflater,
    bits: &mut Bits<'_>,
    output: &mut [u8],
    written: &mut usize,
    tables: &mut Vec<WideTables>,
) -> Result<bool, Error> {
    inflater.decode_wide_codes(bits, output, written, tables)
}

#[cfg(test)]
#[test]
fn bmi2_window_decodes_a_literal_and_match() {
    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    use super::FIXED_CODES;

    let (literal, n) = FIXED_CODES[b'A' as usize];
    let (length, m) = FIXED_CODES[257];
    // The length-three match has fixed distance code zero (five bits).
    // Its end-of-block code is also zero (seven bits).
    let word = u64::from(literal) | u64::from(length) << n;
    let mut input = [0; 24];
    input[..8].copy_from_slice(&word.to_le_bytes());
    let mut bits = Bits::new(&input);
    let mut inflater = Inflater::new(4).unwrap();
    let mut output = [0; 32];
    let mut written = 0;
    // SAFETY: Runtime detection above confirms BMI2.
    assert!(
        unsafe {
            wide_codes_bmi2(
                &mut inflater,
                &mut bits,
                &mut output,
                &mut written,
                &mut Vec::new(),
            )
        }
        .unwrap()
    );
    assert_eq!(&output[..written], b"AAAA");
    assert_eq!(
        bits.pos * 8 - usize::from(bits.count),
        usize::from(n + m + 12)
    );
}
