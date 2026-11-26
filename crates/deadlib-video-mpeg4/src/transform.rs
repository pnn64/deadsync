// Cosine weights for the 8-bit Simple IDCT, scaled by sqrt(2) * 2^14
// with quantized DC and pi/4 weights. Rows are indexed by output sample.
const BASIS: [[i32; 8]; 8] = [
    [16383, 22725, 21407, 19266, 16383, 12873, 8867, 4520],
    [16383, 19266, 8867, -4520, -16383, -22725, -21407, -12873],
    [16383, 12873, -8867, -22725, -16383, 4520, 21407, 19266],
    [16383, 4520, -21407, -12873, 16383, 19266, -8867, -22725],
    [16383, -4520, -21407, 12873, 16383, -19266, -8867, 22725],
    [16383, -12873, -8867, 22725, -16383, -4520, 21407, -19266],
    [16383, -19266, 8867, 4520, -16383, 22725, -21407, 12873],
    [16383, -22725, 21407, -19266, 16383, -12873, 8867, -4520],
];
const ROW_SHIFT: i32 = 11;
const ROW_BIAS: i32 = 1 << (ROW_SHIFT - 1);
const COLUMN_SHIFT: i32 = 20;
const COLUMN_BIAS: i16 = 32;

pub(crate) fn inverse(coefficients: &[i16; 64]) -> [i16; 64] {
    if coefficients[1..].iter().all(|&coeff| coeff == 0) {
        let dc = i32::from(coefficients[0]) * 8 + i32::from(COLUMN_BIAS);
        return [((dc * BASIS[0][0]) >> COLUMN_SHIFT) as i16; 64];
    }

    let mut horizontal = [[0i16; 8]; 8];
    for (row, transformed) in coefficients.as_chunks::<8>().0.iter().zip(&mut horizontal) {
        if row[1..].iter().all(|&coeff| coeff == 0) {
            *transformed = [row[0] * 8; 8];
            continue;
        }
        for (output, basis) in transformed.iter_mut().zip(BASIS) {
            let product = row
                .iter()
                .zip(basis)
                .map(|(&coeff, weight)| i32::from(coeff) * weight)
                .sum::<i32>();
            let value = (product + ROW_BIAS) >> ROW_SHIFT;
            *output = value.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        }
    }
    for dc in &mut horizontal[0] {
        *dc = dc.wrapping_add(COLUMN_BIAS);
    }

    let mut samples = [0; 64];
    for (row, basis) in samples.as_chunks_mut::<8>().0.iter_mut().zip(BASIS) {
        for (x, output) in row.iter_mut().enumerate() {
            let product = horizontal
                .iter()
                .zip(basis)
                .map(|(row, weight)| i32::from(row[x]) * weight)
                .fold(0i32, i32::wrapping_add);
            *output = (product >> COLUMN_SHIFT) as i16;
        }
    }
    samples
}
