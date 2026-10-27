//! Stateful Layer III hybrid and polyphase reconstruction. Input/output uses
//! normalized f64 values; the initial scalar implementation prioritizes parity.
use crate::Error;
#[path = "dsp_tables.rs"]
mod tables;

#[derive(Clone)]
pub struct Hybrid {
    overlap: [[f64; 18]; 32],
}
impl Default for Hybrid {
    fn default() -> Self {
        Self::new()
    }
}
impl Hybrid {
    pub fn new() -> Self {
        Self {
            overlap: [[0.0; 18]; 32],
        }
    }
    pub fn reset(&mut self) {
        self.overlap.fill([0.0; 18]);
    }
    /// Input is frequency order after short-block reordering. Output is
    /// time-major: 18 consecutive sets of 32 polyphase subband samples.
    /// `long_subbands` is zero for pure short blocks, two for standard mixed
    /// blocks; the MPEG-2.5 8 kHz compatibility profile may select four.
    pub fn process(
        &mut self,
        input: &mut [f64; 576],
        block_type: u8,
        long_subbands: usize,
        output: &mut [f64; 576],
    ) -> Result<(), Error> {
        if block_type > 3 || long_subbands > 32 || (block_type == 0 && long_subbands != 0) {
            return Err(Error::Invalid("hybrid block shape"));
        }
        let aliases = if block_type == 2 {
            long_subbands.saturating_sub(1)
        } else {
            31
        };
        for boundary in 1..=aliases {
            for n in 0..8 {
                let a = 18 * boundary - 1 - n;
                let b = 18 * boundary + n;
                let up = input[a];
                let down = input[b];
                input[a] = up * tables::ALIAS_CS[n] - down * tables::ALIAS_CA[n];
                input[b] = down * tables::ALIAS_CS[n] + up * tables::ALIAS_CA[n];
            }
        }
        for band in 0..32 {
            let samples = &input[band * 18..band * 18 + 18];
            let mut transformed = [0.0; 36];
            if samples.iter().any(|&v| v != 0.0) {
                if block_type == 2 && band >= long_subbands {
                    for window in 0..3 {
                        for n in 0..12 {
                            let mut value = 0.0;
                            for k in 0..6 {
                                value += samples[3 * k + window] * tables::IMDCT_SHORT[n][k];
                            }
                            transformed[6 + 6 * window + n] += value * tables::WINDOW_SHORT[n];
                        }
                    }
                } else {
                    let window = if band < long_subbands {
                        0
                    } else {
                        block_type as usize
                    };
                    for (n, result) in transformed.iter_mut().enumerate() {
                        let mut value = 0.0;
                        for (k, &sample) in samples.iter().enumerate() {
                            value += sample * tables::IMDCT_LONG[n][k];
                        }
                        *result = value * tables::WINDOW_LONG[window][n];
                    }
                }
            }
            for n in 0..18 {
                let value = self.overlap[band][n] + transformed[n];
                output[n * 32 + band] = if band & 1 != 0 && n & 1 != 0 {
                    -value
                } else {
                    value
                };
                self.overlap[band][n] = transformed[n + 18];
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Synthesis {
    values: [f64; 1024],
    offset: usize,
}
impl Default for Synthesis {
    fn default() -> Self {
        Self::new()
    }
}
impl Synthesis {
    pub fn new() -> Self {
        Self {
            values: [0.0; 1024],
            offset: 0,
        }
    }
    pub fn reset(&mut self) {
        self.values.fill(0.0);
        self.offset = 0;
    }
    /// One time slot: 32 subband values produce 32 PCM samples. The ring retains
    /// sixteen 64-value matrix outputs, avoiding a full history copy per slot.
    pub fn process(&mut self, subbands: &[f64; 32], pcm: &mut [f64; 32]) {
        self.offset = (self.offset + 960) & 1023;
        for n in 0..64 {
            let mut value = 0.0;
            for (k, &sample) in subbands.iter().enumerate() {
                value += sample * tables::SYNTHESIS[n][k];
            }
            self.values[self.offset + n] = value;
        }
        for (n, sample) in pcm.iter_mut().enumerate() {
            let mut value = 0.0;
            for group in 0..8 {
                value += tables::DEWINDOW[64 * group + n]
                    * self.values[(self.offset + 128 * group + n) & 1023];
                value += tables::DEWINDOW[64 * group + 32 + n]
                    * self.values[(self.offset + 128 * group + 96 + n) & 1023];
            }
            *sample = value;
        }
    }
}
