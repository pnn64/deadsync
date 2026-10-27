use crate::{Error, reserve};
use core::f64::consts::PI;

/// Unnormalised Vorbis inverse MDCT. Initialization owns all tables/scratch;
/// repeated transforms allocate nothing and never touch external codec code.
///
/// Pair even and reversed odd spectrum values into a complex FFT of length N/4,
/// recover the N/2 DCT-IV by phase rotation, then apply signed MDCT symmetry.
/// See MDCT.md for the derivation. The direct cosine/reference tests cover all
/// legal sizes independently of this factorization.
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
        let m = n / 2;
        let fft = n / 4;
        reserve(&mut rotation, fft)?;
        let mut twiddle = Vec::new();
        reserve(&mut twiddle, fft / 2)?;
        let mut scratch = Vec::new();
        reserve(&mut scratch, fft)?;
        scratch.resize(fft, [0.0; 2]);
        for k in 0..fft {
            let phase = -PI * k as f64 / m as f64;
            let post = -PI * (k as f64 + 0.25) / m as f64;
            rotation.push([
                phase.cos() as f32,
                phase.sin() as f32,
                post.cos() as f32,
                post.sin() as f32,
            ]);
            if k < fft / 2 {
                let phase = -2.0 * PI * k as f64 / fft as f64;
                twiddle.push([phase.cos() as f32, phase.sin() as f32]);
            }
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
        self.n * 7
    }
    pub fn inverse(&mut self, spectrum: &[f32], output: &mut [f32]) -> Result<(), Error> {
        let n = self.n;
        let m = n / 2;
        if spectrum.len() != m || output.len() != n {
            return Err(Error::Invalid("MDCT buffer length"));
        }
        let fft = n / 4;
        let shift = usize::BITS - fft.trailing_zeros();
        for (k, rotation) in self.rotation.iter().enumerate() {
            let a = spectrum[2 * k];
            let b = spectrum[m - 1 - 2 * k];
            self.scratch[k.reverse_bits() >> shift] = [
                a * rotation[0] - b * rotation[1],
                a * rotation[1] + b * rotation[0],
            ];
        }
        self.fft();
        // Store DCT-IV values in the middle region of the output, with sign
        // already inverted; the other two regions follow by exact symmetry.
        for k in 0..fft {
            let rotation = self.rotation[k];
            let [real, imag] = self.scratch[k];
            output[3 * m / 2 - 1 - 2 * k] = -(real * rotation[2] - imag * rotation[3]);
            output[m / 2 + 2 * k] = real * rotation[3] + imag * rotation[2];
        }
        for k in 0..m / 2 {
            output[k] = -output[m - 1 - k];
            output[3 * m / 2 + k] = output[3 * m / 2 - 1 - k];
        }
        Ok(())
    }
    fn fft(&mut self) {
        let fft = self.n / 4;
        let mut width = 2;
        while width <= fft {
            let half = width / 2;
            let step = fft / width;
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
    }
}

/// Vorbis forward MDCT with 4/N analysis normalization. The caller supplies a
/// windowed block of N time samples and receives N/2 spectral coefficients.
/// Uses the same independently derived N/4 FFT tables as inverse synthesis;
/// repeated transforms allocate nothing. See MDCT.md for the transpose fold.
pub struct Mdct {
    core: Imdct,
}
impl Mdct {
    pub fn new(block_size: usize) -> Result<Self, Error> {
        Ok(Self {
            core: Imdct::new(block_size)?,
        })
    }
    pub fn block_size(&self) -> usize {
        self.core.block_size()
    }
    pub fn storage_bytes(&self) -> usize {
        self.core.storage_bytes()
    }
    pub fn forward(&mut self, input: &[f32], spectrum: &mut [f32]) -> Result<(), Error> {
        let n = self.core.n;
        let m = n / 2;
        if input.len() != n || spectrum.len() != m {
            return Err(Error::Invalid("MDCT buffer length"));
        }
        // Transpose of the inverse's signed symmetry permutation. DCT-IV is
        // symmetric, so the same complex factorization computes analysis.
        let fold = |r: usize| {
            if r < m / 2 {
                -input[3 * m / 2 - 1 - r] - input[3 * m / 2 + r]
            } else {
                let q = r - m / 2;
                input[q] - input[m - 1 - q]
            }
        };
        let fft = n / 4;
        let shift = usize::BITS - fft.trailing_zeros();
        for (k, rotation) in self.core.rotation.iter().enumerate() {
            let a = fold(2 * k);
            let b = fold(m - 1 - 2 * k);
            self.core.scratch[k.reverse_bits() >> shift] = [
                a * rotation[0] - b * rotation[1],
                a * rotation[1] + b * rotation[0],
            ];
        }
        self.core.fft();
        let scale = 4.0 / n as f32;
        for k in 0..fft {
            let rotation = self.core.rotation[k];
            let [real, imag] = self.core.scratch[k];
            spectrum[2 * k] = (real * rotation[2] - imag * rotation[3]) * scale;
            spectrum[m - 1 - 2 * k] = -(real * rotation[3] + imag * rotation[2]) * scale;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fft_mdct_matches_direct_analysis_at_every_legal_size() {
        let mut seed = 19u32;
        for n in [64, 128, 256, 512, 1024, 2048, 4096, 8192] {
            let input: Vec<f32> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    seed as i32 as f32 / i32::MAX as f32
                })
                .collect();
            let mut transform = Mdct::new(n).unwrap();
            let mut output = vec![0.0; n / 2];
            transform.forward(&input, &mut output).unwrap();
            for k in (0..n / 2).step_by((n / 256).max(1)) {
                let expected: f64 = input
                    .iter()
                    .enumerate()
                    .map(|(t, &x)| {
                        x as f64
                            * (PI / (n / 2) as f64
                                * (t as f64 + 0.5 + n as f64 / 4.0)
                                * (k as f64 + 0.5))
                                .cos()
                    })
                    .sum::<f64>()
                    * 4.0
                    / n as f64;
                assert!(
                    (output[k] as f64 - expected).abs() < 2e-7,
                    "N={n}, k={k}: {} != {expected}",
                    output[k]
                );
            }
            transform.forward(&vec![0.0; n], &mut output).unwrap();
            assert!(output.iter().all(|&x| x == 0.0));
        }
    }
    #[test]
    fn analysis_window_and_synthesis_overlap_reconstruct_unquantized_pcm() {
        for n in [64, 128, 256, 512, 1024, 2048, 4096, 8192] {
            let half = n / 2;
            let input: Vec<f32> = (0..half * 3 + 17)
                .map(|i| {
                    (i as f64 * 0.117).sin() as f32 * 0.7 + (i as f64 * 0.031).cos() as f32 * 0.2
                })
                .collect();
            let window: Vec<f32> = (0..n)
                .map(|i| ((PI * (i as f64 + 0.5) / n as f64).sin().powi(2) * PI / 2.0).sin() as f32)
                .collect();
            let mut forward = Mdct::new(n).unwrap();
            let mut inverse = Imdct::new(n).unwrap();
            let mut time = vec![0.0; n];
            let mut spectrum = vec![0.0; half];
            let mut current = vec![0.0; n];
            let mut previous = vec![0.0; half];
            for block in 0..input.len().div_ceil(half) + 1 {
                for t in 0..n {
                    let position = block as isize * half as isize + t as isize - half as isize;
                    time[t] = input.get(position as usize).copied().unwrap_or(0.0) * window[t];
                }
                forward.forward(&time, &mut spectrum).unwrap();
                inverse.inverse(&spectrum, &mut current).unwrap();
                if block != 0 {
                    let start = (block - 1) * half;
                    for i in 0..half.min(input.len().saturating_sub(start)) {
                        let value = previous[i] + current[i] * window[i];
                        assert!(
                            (value - input[start + i]).abs() < 2e-6,
                            "N={n}, sample={}: {value} != {}",
                            start + i,
                            input[start + i]
                        );
                    }
                }
                for i in 0..half {
                    previous[i] = current[half + i] * window[half + i];
                }
            }
        }
    }
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
