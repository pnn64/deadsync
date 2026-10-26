use crate::{AudioHeader, Error, Imdct, Limits, Setup, bitpack::BitReader, floor, residue};
use core::f64::consts::FRAC_PI_2;

/// Borrowed planar, unclipped f32 PCM in Vorbis mapping-zero channel order.
/// Frame positions are raw overlap output positions, before container trimming.
pub struct PcmBlock<'a> {
    data: &'a [f32],
    stride: usize,
    pub channels: usize,
    pub frames: usize,
    pub start_frame: u64,
    pub block_size: usize,
    /// A nominal end-of-packet condition after a valid mode/window prefix.
    pub truncated: bool,
}
impl<'a> PcmBlock<'a> {
    pub fn plane(&self, channel: usize) -> Option<&'a [f32]> {
        if channel >= self.channels {
            return None;
        }
        Some(&self.data[channel * self.stride..channel * self.stride + self.frames])
    }
    pub fn planes(&self) -> impl ExactSizeIterator<Item = &'a [f32]> + '_ {
        (0..self.channels).map(move |channel| self.plane(channel).unwrap())
    }
    pub(crate) fn crop(mut self, start: usize, end: usize) -> Self {
        debug_assert!(start <= end && end <= self.frames);
        self.data = &self.data[start..];
        self.start_frame += start as u64;
        self.frames = end - start;
        self
    }
}

/// Reusable native raw-packet Vorbis I synthesis context. No Ogg framing,
/// granule trimming, resampling, clipping or implicit channel reordering.
/// Normal packet decoding allocates nothing after initialization.
pub struct PacketDecoder {
    setup: Setup,
    limits: Limits,
    short: Imdct,
    long: Option<Imdct>,
    short_slope: Vec<f32>,
    long_slope: Vec<f32>,
    spectrum: Vec<f32>,
    floors: Vec<f32>,
    current: Vec<f32>,
    previous: Vec<f32>,
    pcm: Vec<f32>,
    classes: Vec<u8>,
    previous_size: usize,
    position: u64,
    storage: usize,
}
impl PacketDecoder {
    pub fn new(setup: Setup) -> Result<Self, Error> {
        Self::with_limits(setup, Limits::default())
    }
    pub fn with_limits(setup: Setup, limits: Limits) -> Result<Self, Error> {
        let channels = setup.channels() as usize;
        let [small, large] = setup.block_sizes();
        let half = large / 2;
        let storage = Self::required_storage(&setup);
        if storage > limits.max_decode_memory {
            return Err(Error::LimitExceeded("decode memory"));
        }
        let short = Imdct::new(small)?;
        let long = if large != small {
            Some(Imdct::new(large)?)
        } else {
            None
        };
        let short_slope = slope(small / 2)?;
        let long_slope = if large != small {
            slope(half)?
        } else {
            Vec::new()
        };
        Ok(Self {
            setup,
            limits,
            short,
            long,
            short_slope,
            long_slope,
            spectrum: zeroed(channels * half)?,
            floors: zeroed(channels * half)?,
            current: zeroed(channels * large)?,
            previous: zeroed(channels * half)?,
            pcm: zeroed(channels * half)?,
            classes: zeroed(channels * half)?,
            previous_size: 0,
            position: 0,
            storage,
        })
    }
    pub fn setup(&self) -> &Setup {
        &self.setup
    }
    pub fn required_storage(setup: &Setup) -> usize {
        let channels = setup.channels() as usize;
        let [small, large] = setup.block_sizes();
        let transforms = small + if large != small { large } else { 0 };
        channels * large * 12 + channels * (large / 2) + transforms * 22
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    /// Buffer/table elements excluding setup, allocator and struct metadata.
    pub fn storage_bytes(&self) -> usize {
        self.storage
    }
    /// Discard overlap and reset the raw output position. Keep setup, tables
    /// and allocations; the next audio packet primes again and returns 0 frames.
    pub fn reset(&mut self) {
        self.reset_at(0);
    }
    /// Reset for caller-managed raw-packet seeking. `position` is the raw end
    /// position of the packet that will prime overlap, before container trims.
    /// That next valid packet returns zero frames; subsequent output begins at
    /// this position. The caller must supply the actual preceding audio packet.
    pub fn reset_at(&mut self, position: u64) {
        self.previous_size = 0;
        self.position = position;
    }

    /// None means a non-audio packet or a truncated mode/window prefix was
    /// discarded without advancing overlap. Errors also preserve prior overlap.
    /// A valid first audio packet returns Some with zero frames (priming).
    /// End-of-packet after the prefix is nominal and produces a truncated block.
    /// Do not invent/flush a final block: a container supplies final sample count.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Option<PcmBlock<'_>>, Error> {
        let Some((header, mut bits)) = packet_header(&self.setup, &self.limits, packet)? else {
            return Ok(None);
        };
        let channels = self.setup.channels() as usize;
        let [small, large] = self.setup.block_sizes();
        let stride = large / 2;
        let n = header.block_size;
        let bins = n / 2;
        let mapping =
            &self.setup.mappings()[self.setup.modes()[header.mode as usize].mapping as usize];
        self.spectrum.fill(0.0);
        let mut nonzero = [false; 255];
        let mut truncated = false;
        for (ch, nonzero) in nonzero.iter_mut().enumerate().take(channels) {
            let floor_number = mapping.submaps[mapping.mux[ch] as usize].floor as usize;
            let output = &mut self.floors[ch * stride..ch * stride + bins];
            match floor::decode(
                &self.setup.floors()[floor_number],
                self.setup.codebooks(),
                &mut bits,
                output,
            ) {
                Ok(true) => *nonzero = true,
                Ok(false) => output.fill(0.0),
                Err(Error::EndOfPacket) => {
                    truncated = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if truncated {
            // Section 4.3.2: a truncated floor zeros every channel, then laps
            // with previously synthesized right-hand data at the normal step.
            for ch in 0..channels {
                self.current[ch * large..ch * large + n].fill(0.0);
            }
        } else {
            for &[magnitude, angle] in &mapping.coupling {
                if nonzero[magnitude as usize] || nonzero[angle as usize] {
                    nonzero[magnitude as usize] = true;
                    nonzero[angle as usize] = true;
                }
            }
            let mut bundle = [0; 255];
            for (submap_number, submap) in mapping.submaps.iter().enumerate() {
                let mut count = 0;
                for (ch, &mux) in mapping.mux.iter().enumerate() {
                    if mux as usize == submap_number {
                        bundle[count] = ch;
                        count += 1;
                    }
                }
                let result = residue::decode(
                    &self.setup.residues()[submap.residue as usize],
                    self.setup.codebooks(),
                    &mut bits,
                    &mut residue::Output {
                        spectrum: &mut self.spectrum,
                        stride,
                        bins,
                        bundle: &bundle[..count],
                        nonzero: &nonzero,
                        classes: &mut self.classes,
                    },
                );
                match result {
                    Ok(()) => {}
                    Err(Error::EndOfPacket) => {
                        truncated = true;
                        break;
                    }
                    Err(error) => return Err(error),
                }
            }
            for &[magnitude, angle] in mapping.coupling.iter().rev() {
                inverse_coupling(
                    &mut self.spectrum,
                    stride,
                    bins,
                    magnitude as usize,
                    angle as usize,
                );
            }
            for ch in 0..channels {
                let spectrum = &mut self.spectrum[ch * stride..ch * stride + bins];
                let floor = &self.floors[ch * stride..ch * stride + bins];
                for (value, &gain) in spectrum.iter_mut().zip(floor) {
                    *value *= gain;
                    if !value.is_finite() {
                        return Err(Error::Invalid("nonfinite audio spectrum"));
                    }
                }
                let transform = if n == small {
                    &mut self.short
                } else {
                    self.long.as_mut().unwrap()
                };
                let output = &mut self.current[ch * large..ch * large + n];
                transform.inverse(spectrum, output)?;
                if output.iter().any(|value| !value.is_finite()) {
                    return Err(Error::Invalid("nonfinite MDCT output"));
                }
                apply_window(output, header, small, &self.short_slope, &self.long_slope);
            }
        }
        let frames = if self.previous_size == 0 {
            0
        } else {
            (self.previous_size + n) / 4
        };
        let start_frame = self.position;
        let next_position = self
            .position
            .checked_add(frames as u64)
            .ok_or(Error::LimitExceeded("PCM frame position"))?;
        let current_start = frames as isize - bins as isize;
        for ch in 0..channels {
            let current = &self.current[ch * large..ch * large + n];
            let previous = &mut self.previous[ch * stride..ch * stride + stride];
            let output = &mut self.pcm[ch * stride..ch * stride + frames];
            for (i, value) in output.iter_mut().enumerate() {
                let prior = if i < self.previous_size / 2 {
                    previous[i]
                } else {
                    0.0
                };
                let index = i as isize - current_start;
                let next = if index >= 0 {
                    current[index as usize]
                } else {
                    0.0
                };
                *value = prior + next;
                if !value.is_finite() {
                    return Err(Error::Invalid("nonfinite overlap output"));
                }
            }
        }
        for ch in 0..channels {
            self.previous[ch * stride..ch * stride + bins]
                .copy_from_slice(&self.current[ch * large + bins..ch * large + n]);
        }
        self.previous_size = n;
        self.position = next_position;
        Ok(Some(PcmBlock {
            data: &self.pcm,
            stride,
            channels,
            frames,
            start_frame,
            block_size: n,
            truncated,
        }))
    }
}
pub(crate) fn packet_header<'a>(
    setup: &Setup,
    limits: &Limits,
    packet: &'a [u8],
) -> Result<Option<(AudioHeader, BitReader<'a>)>, Error> {
    if packet.len() > limits.max_packet_bytes {
        return Err(Error::LimitExceeded("audio packet bytes"));
    }
    if packet.is_empty() || packet[0] & 1 != 0 {
        return Ok(None);
    }
    let mut bits = BitReader::new(packet);
    match setup.read_audio_header(&mut bits) {
        Ok(header) => Ok(Some((header, bits))),
        Err(Error::EndOfPacket) => Ok(None),
        Err(error) => Err(error),
    }
}

fn zeroed<T: Default + Clone>(count: usize) -> Result<Vec<T>, Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| Error::Allocation)?;
    out.resize(count, T::default());
    Ok(out)
}
fn slope(length: usize) -> Result<Vec<f32>, Error> {
    let mut out = zeroed(length)?;
    for (i, value) in out.iter_mut().enumerate() {
        let sine = ((i as f64 + 0.5) / length as f64 * FRAC_PI_2).sin();
        *value = (FRAC_PI_2 * sine * sine).sin() as f32;
    }
    Ok(out)
}
fn apply_window(
    output: &mut [f32],
    header: AudioHeader,
    small: usize,
    short: &[f32],
    long: &[f32],
) {
    let n = output.len();
    let left = if n == small || !header.previous_long {
        short
    } else {
        long
    };
    let right = if n == small || !header.next_long {
        short
    } else {
        long
    };
    let left_start = n / 4 - left.len() / 2;
    let right_start = 3 * n / 4 - right.len() / 2;
    output[..left_start].fill(0.0);
    for (value, &gain) in output[left_start..left_start + left.len()]
        .iter_mut()
        .zip(left)
    {
        *value *= gain;
    }
    for (value, &gain) in output[right_start..right_start + right.len()]
        .iter_mut()
        .zip(right.iter().rev())
    {
        *value *= gain;
    }
    output[right_start + right.len()..].fill(0.0);
}
fn inverse_coupling(
    spectrum: &mut [f32],
    stride: usize,
    bins: usize,
    magnitude: usize,
    angle: usize,
) {
    let (m, a) = if magnitude < angle {
        let (left, right) = spectrum.split_at_mut(angle * stride);
        (
            &mut left[magnitude * stride..magnitude * stride + bins],
            &mut right[..bins],
        )
    } else {
        let (left, right) = spectrum.split_at_mut(magnitude * stride);
        (
            &mut right[..bins],
            &mut left[angle * stride..angle * stride + bins],
        )
    };
    for (m, a) in m.iter_mut().zip(a) {
        let (magnitude, angle) = (*m, *a);
        (*m, *a) = if magnitude > 0.0 {
            if angle > 0.0 {
                (magnitude, magnitude - angle)
            } else {
                (magnitude + angle, magnitude)
            }
        } else if angle > 0.0 {
            (magnitude, magnitude + angle)
        } else {
            (magnitude - angle, magnitude)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_coupling_covers_all_signs_and_channel_orders() {
        for (magnitude, angle, expected) in [
            (3.0, 2.0, [3.0, 1.0]),
            (3.0, -2.0, [1.0, 3.0]),
            (-3.0, 2.0, [-3.0, -1.0]),
            (-3.0, -2.0, [-1.0, -3.0]),
            (0.0, 0.0, [0.0, 0.0]),
        ] {
            for reverse in [false, true] {
                let mut spectrum = if reverse {
                    [angle, magnitude]
                } else {
                    [magnitude, angle]
                };
                inverse_coupling(
                    &mut spectrum,
                    1,
                    1,
                    usize::from(reverse),
                    usize::from(!reverse),
                );
                let result = if reverse {
                    [spectrum[1], spectrum[0]]
                } else {
                    spectrum
                };
                assert_eq!(result, expected);
            }
        }
    }
    #[test]
    fn windows_obey_complementary_slopes_and_hybrid_bounds() {
        let short = slope(32).unwrap();
        let long = slope(256).unwrap();
        for slope in [&short, &long] {
            for (&a, &b) in slope.iter().zip(slope.iter().rev()) {
                assert!((a * a + b * b - 1.0).abs() < 2.0e-7);
            }
        }
        for previous_long in [false, true] {
            for next_long in [false, true] {
                let mut data = vec![1.0; 512];
                apply_window(
                    &mut data,
                    AudioHeader {
                        mode: 1,
                        block_size: 512,
                        previous_long,
                        next_long,
                    },
                    64,
                    &short,
                    &long,
                );
                let left = if previous_long { 256 } else { 32 };
                let right = if next_long { 256 } else { 32 };
                let start = 128 - left / 2;
                let end = 384 + right / 2;
                assert!(data[..start].iter().all(|&x| x == 0.0));
                assert!(data[end..].iter().all(|&x| x == 0.0));
                assert!(data[start..end].iter().all(|&x| x > 0.0 && x <= 1.0));
            }
        }
    }
}
