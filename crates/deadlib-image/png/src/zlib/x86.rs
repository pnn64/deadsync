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
