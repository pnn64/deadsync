use crate::{Error, reserve};
use core::f64::consts::PI;

/// Unnormalised Vorbis inverse MDCT. Initialization owns all tables/scratch;
/// repeated transforms allocate nothing and never touch external codec code.
///
/// The transform is derived by a phase-rotated, zero-padded complex FFT of
/// length N, yielding an N/2 DCT-IV, then applying the MDCT's signed symmetry.
/// This deliberately transparent O(N log N) baseline is independently tested
/// against the direct cosine equation and libvorbis, before further tuning.
pub struct Imdct {
    n: usize,
    rotation: Vec<[f32; 4]>,
    twiddle: Vec<[f32; 2]>,
    scratch: Vec<[f32; 2]>,
}
impl Imdct {
    pub fn new(block_size: usize) -> Result<Self, Error> {
        let n = block_size;
        if !(64..=8192).contains(&n) || !n.is_power_of_two() {
            return Err(Error::Invalid("MDCT block size"));
        }
        let mut rotation = Vec::new();
        reserve(&mut rotation, n / 2)?;
        let mut twiddle = Vec::new();
        reserve(&mut twiddle, n / 2)?;
        let mut scratch = Vec::new();
        reserve(&mut scratch, n)?;
        scratch.resize(n, [0.0; 2]);
        for k in 0..n / 2 {
            let phase = -PI * k as f64 / n as f64;
            let post = -PI * (k as f64 + 0.5) / n as f64;
            rotation.push([
                phase.cos() as f32,
                phase.sin() as f32,
                post.cos() as f32,
                post.sin() as f32,
            ]);
            let phase = -2.0 * PI * k as f64 / n as f64;
            twiddle.push([phase.cos() as f32, phase.sin() as f32]);
        }
        Ok(Self {
            n,
            rotation,
            twiddle,
            scratch,
        })
    }
    pub fn block_size(&self) -> usize {
        self.n
    }
    /// Owned table and scratch elements, excluding allocator/Vec metadata.
    pub fn storage_bytes(&self) -> usize {
        self.n * 20
    }
    pub fn inverse(&mut self, spectrum: &[f32], output: &mut [f32]) -> Result<(), Error> {
        let n = self.n;
        let m = n / 2;
        if spectrum.len() != m || output.len() != n {
            return Err(Error::Invalid("MDCT buffer length"));
        }
        self.scratch.fill([0.0; 2]);
        let shift = usize::BITS - n.trailing_zeros();
        for (k, (&value, rotation)) in spectrum.iter().zip(&self.rotation).enumerate() {
            self.scratch[k.reverse_bits() >> shift] = [value * rotation[0], value * rotation[1]];
        }
        let mut width = 2;
        while width <= n {
            let half = width / 2;
            let step = n / width;
            for group in self.scratch.chunks_exact_mut(width) {
                let (left, right) = group.split_at_mut(half);
                for (j, (a, b)) in left.iter_mut().zip(right).enumerate() {
                    let [wr, wi] = self.twiddle[j * step];
                    let [br, bi] = *b;
                    let real = br * wr - bi * wi;
                    let imag = br * wi + bi * wr;
                    *b = [a[0] - real, a[1] - imag];
                    a[0] += real;
                    a[1] += imag;
                }
            }
            width *= 2;
        }
        // Store DCT-IV values in the middle region of the output, with sign
        // already inverted; the other two regions follow by exact symmetry.
        for k in 0..m {
            let rotation = self.rotation[k];
            let [real, imag] = self.scratch[k];
            output[3 * m / 2 - 1 - k] = -(real * rotation[2] - imag * rotation[3]);
        }
        for k in 0..m / 2 {
            output[k] = -output[m - 1 - k];
            output[3 * m / 2 + k] = output[3 * m / 2 - 1 - k];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fft_imdct_matches_direct_cosine_for_all_vorbis_sizes() {
        let mut random = 0x91e1_0da5u32;
        for n in [64, 128, 256, 512, 1024, 2048, 4096, 8192] {
            let mut transform = Imdct::new(n).unwrap();
            let spectrum: Vec<f32> = (0..n / 2)
                .map(|_| {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    (random as i32 as f64 / i32::MAX as f64 / (n as f64).sqrt()) as f32
                })
                .collect();
            let mut output = vec![0.0; n];
            transform.inverse(&spectrum, &mut output).unwrap();
            // Check every sample at small sizes and a deterministic spread at
            // large sizes, keeping the independent quadratic oracle bounded.
            let step = (n / 128).max(1);
            for t in (0..n).step_by(step) {
                let expected: f64 = spectrum
                    .iter()
                    .enumerate()
                    .map(|(k, &x)| {
                        x as f64
                            * (PI / (n / 2) as f64
                                * (t as f64 + 0.5 + n as f64 / 4.0)
                                * (k as f64 + 0.5))
                                .cos()
                    })
                    .sum();
                assert!(
                    (output[t] as f64 - expected).abs() < 1.0e-6,
                    "n={n}, t={t}, native={}, direct={expected}",
                    output[t]
                );
            }
            output.fill(f32::NAN);
            transform.inverse(&vec![0.0; n / 2], &mut output).unwrap();
            assert!(output.iter().all(|&x| x == 0.0));
        }
    }
}
