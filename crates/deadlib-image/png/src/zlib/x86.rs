use super::{Bits, Error};

#[inline(never)]
pub(super) fn inflate(
    bits: &mut Bits<'_>,
    output: &mut [u8],
    written: &mut usize,
    fixed: bool,
    window: usize,
) -> Result<(), Error> {
    if std::is_x86_feature_detected!("bmi2") {
        // SAFETY: BMI2 availability is checked above. The implementation uses
        // checked slices and has no unsafe memory operations.
        unsafe { bmi2(bits, output, written, fixed, window) }
    } else {
        super::inflate_sized_codes(bits, output, written, fixed, window)
    }
}

#[target_feature(enable = "bmi2")]
unsafe fn bmi2(
    bits: &mut Bits<'_>,
    output: &mut [u8],
    written: &mut usize,
    fixed: bool,
    window: usize,
) -> Result<(), Error> {
    super::inflate_sized_codes(bits, output, written, fixed, window)
}
