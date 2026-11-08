use crate::{
    Error, Floor, Floor0, Floor0Behavior, Floor1,
    bitpack::{BitReader, ilog},
    codebook::Codebook,
    floor_table::INVERSE_DB,
};
use core::f64::consts::PI;
#[cfg(test)]
#[path = "floor_reference.rs"]
mod reference_tests;

/// EndOfPacket is deliberately preserved so the packet decoder can implement
/// section 4.3.2's all-channel zeroing rule before overlap/add, or the reference's
/// channel-local concealment when IEEE propagation is explicitly enabled.
pub(crate) fn decode(
    config: &Floor,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut [f32],
    allow_nonfinite: bool,
    behavior: Floor0Behavior,
) -> Result<bool, Error> {
    match config {
        Floor::Type0(config) => decode0(config, books, bits, output, allow_nonfinite, behavior),
        Floor::Type1(config) => decode1(config, books, bits, output),
    }
}
fn decode1(
    config: &Floor1,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut [f32],
) -> Result<bool, Error> {
    if !bits.flag()? {
        return Ok(false);
    }
    let range = [256i64, 128, 86, 64][config.multiplier as usize - 1];
    let mut y = [0i64; 65];
    let mut active = [false; 65];
    let width = ilog(range as u32 - 1);
    y[0] = bits.read(width)? as i64;
    y[1] = bits.read(width)? as i64;
    let mut offset = 2;
    for &class in &config.partition_classes {
        let class = &config.classes[class as usize];
        let mut selection = match class.masterbook {
            Some(book) => {
                let book = &books[book as usize];
                if book.is_empty() {
                    return Ok(false);
                }
                book.decode_scalar(bits)?
            }
            None => 0,
        };
        let mask = (1 << class.subclasses) - 1;
        for value in &mut y[offset..offset + class.dimensions as usize] {
            *value = match class.books[(selection & mask) as usize] {
                Some(book) => {
                    let book = &books[book as usize];
                    if book.is_empty() {
                        return Ok(false);
                    }
                    book.decode_scalar(bits)? as i64
                }
                None => 0,
            };
            selection >>= class.subclasses;
        }
        offset += class.dimensions as usize;
    }
    active[0] = true;
    active[1] = true;
    for i in 2..config.x.len() {
        let [low, high] = config.neighbors[i].map(|n| n as usize);
        let predicted = render_point(
            config.x[low] as i64,
            y[low],
            config.x[high] as i64,
            y[high],
            config.x[i] as i64,
        );
        let value = y[i];
        let high_room = range - predicted;
        let low_room = predicted;
        let room = high_room.min(low_room) * 2;
        y[i] = if value == 0 {
            predicted
        } else {
            active[low] = true;
            active[high] = true;
            active[i] = true;
            if value >= room {
                if high_room > low_room {
                    value - low_room + predicted
                } else {
                    predicted - value + high_room - 1
                }
            } else if value & 1 != 0 {
                predicted - (value + 1) / 2
            } else {
                predicted + value / 2
            }
        };
    }
    // The spec explicitly recommends guarding abused setup/codebook values.
    for value in &mut y[..config.x.len()] {
        *value = (*value).clamp(0, range - 1);
    }
    let mut lx = 0;
    let mut ly = y[0] * config.multiplier as i64;
    for &index in &config.sorted_order[1..] {
        let index = index as usize;
        if !active[index] {
            continue;
        }
        let hx = config.x[index] as usize;
        let hy = y[index] * config.multiplier as i64;
        render_line(lx, ly, hx, hy, output);
        lx = hx;
        ly = hy;
        if lx >= output.len() {
            break;
        }
    }
    if lx < output.len() {
        output[lx..].fill(INVERSE_DB[ly as usize]);
    }
    Ok(true)
}
fn render_point(x0: i64, y0: i64, x1: i64, y1: i64, x: i64) -> i64 {
    let dy = y1 - y0;
    let offset = dy.abs() * (x - x0) / (x1 - x0);
    if dy < 0 { y0 - offset } else { y0 + offset }
}
fn render_line(x0: usize, y0: i64, x1: usize, y1: i64, output: &mut [f32]) {
    if x0 >= output.len() {
        return;
    }
    let dx = (x1 - x0) as i64;
    let dy = y1 - y0;
    let base = dy / dx;
    let remainder = dy.abs() - base.abs() * dx;
    let step = base + if dy < 0 { -1 } else { 1 };
    let mut error = 0;
    let mut y = y0;
    let end = x1.min(output.len());
    for value in &mut output[x0..end] {
        *value = INVERSE_DB[y as usize];
        error += remainder;
        if error >= dx {
            error -= dx;
            y += step;
        } else {
            y += base;
        }
    }
}
fn decode0(
    config: &Floor0,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut [f32],
    allow_nonfinite: bool,
    behavior: Floor0Behavior,
) -> Result<bool, Error> {
    let width = config.amplitude_bits as u32;
    let amplitude = if behavior == Floor0Behavior::Libvorbis {
        if width > 32 {
            // The reference bit reader turns an unsupported width into sticky
            // EOF. Preserve the native reader's last-successful-position API.
            bits.mark_exhausted();
            return Err(Error::EndOfPacket);
        }
        let raw = bits.read(width)?;
        if (raw as i32) > 0 { raw as u64 } else { 0 }
    } else if width <= 32 {
        bits.read(width)? as u64
    } else {
        bits.read(32)? as u64 | (bits.read(width - 32)? as u64) << 32
    };
    if amplitude == 0 {
        return Ok(false);
    }
    let book = bits.read(ilog(config.books.len() as u32))? as usize;
    let Some(book) = config.books.get(book) else {
        // Invalid selectors conceal this channel's floor. They do not exhaust
        // the packet or prevent following channels from reading their floors.
        return Ok(false);
    };
    let book = &books[*book as usize];
    let order = config.order as usize;
    let mut coefficients = [0.0f32; 255];
    let mut position = 0;
    let mut last = 0.0;
    while position < order && !book.is_empty() {
        let values = book.decode_vector(bits)?;
        let count = (order - position).min(values.len());
        for (&value, coefficient) in values
            .iter()
            .zip(&mut coefficients[position..position + count])
        {
            *coefficient = value + last;
        }
        position += count;
        last = coefficients[position - 1];
    }
    // Match the reference's f32 LSP/product boundaries before its double
    // square root and exponential. Small rounding differences become audible
    // at the sharp resonances reachable through damaged packet codewords.
    let mut cosine = [0.0f32; 255];
    for (value, &coefficient) in cosine.iter_mut().zip(&coefficients[..order]) {
        *value = (2.0 * (coefficient as f64).cos()) as f32;
    }
    let mapping = BarkMap::new(config.rate, config.bark_map_size, behavior);
    let mut previous_map = usize::MAX;
    let mut gain = 0.0;
    let n = output.len();
    for (i, value) in output.iter_mut().enumerate() {
        let map = mapping.index(i, n);
        if map != previous_map {
            gain = floor0_gain(
                config,
                amplitude,
                &coefficients[..order],
                &cosine[..order],
                map,
                allow_nonfinite,
                behavior,
            );
            if !gain.is_finite() && !allow_nonfinite {
                return Err(Error::Invalid("nonfinite floor0 curve"));
            }
            previous_map = map;
        }
        *value = gain;
    }
    Ok(true)
}
fn floor0_gain(
    config: &Floor0,
    raw_amplitude: u64,
    coefficients: &[f32],
    cosine: &[f32],
    map: usize,
    allow_nonfinite: bool,
    behavior: Floor0Behavior,
) -> f32 {
    let denominator = if behavior == Floor0Behavior::Libvorbis {
        // Deterministic model of the pinned x86 build's variable-width shift;
        // no Rust integer overflow or undefined shift is performed.
        1u32.wrapping_shl(config.amplitude_bits as u32)
            .wrapping_sub(1) as u64
    } else {
        (1u64 << config.amplitude_bits) - 1
    };
    let amplitude = raw_amplitude as f32 / denominator as f32 * config.amplitude_offset as f32;
    let angle_step = (PI / config.bark_map_size as f64) as f32;
    let omega = (2.0 * ((angle_step * map as f32) as f64).cos()) as f32;
    let mut p = 0.5f32;
    let mut q = 0.5f32;
    for (j, &coefficient) in cosine.iter().enumerate() {
        if j & 1 == 0 {
            q *= omega - coefficient;
        } else {
            p *= omega - coefficient;
        }
    }
    // Preserve the inner multiplication's f32 rounding before the outer one.
    if coefficients.len() & 1 != 0 {
        let scaled = p * (4.0 - omega * omega);
        p *= scaled;
        q *= q;
    } else {
        let scaled_p = p * (2.0 - omega);
        let scaled_q = q * (2.0 + omega);
        p *= scaled_p;
        q *= scaled_q;
    }
    let gain = ((amplitude as f64 / ((p + q) as f64).sqrt() - config.amplitude_offset as f64)
        * 0.11512925f32 as f64)
        .exp() as f32;
    if gain.is_finite() || allow_nonfinite {
        return gain;
    }
    // Preserve the existing wide-range synthesis for high-order floors whose
    // f32 products overflow/underflow. This default finite-output policy rejects
    // nonfinite final gains; explicit IEEE propagation returns above instead.
    let omega = (PI * map as f64 / config.bark_map_size as f64).cos();
    let mut p = 1.0;
    let mut q = 1.0;
    for (j, &coefficient) in coefficients.iter().enumerate() {
        let term = 2.0 * ((coefficient as f64).cos() - omega);
        if j & 1 == 0 {
            q *= term * term;
        } else {
            p *= term * term;
        }
    }
    if coefficients.len() & 1 != 0 {
        p *= 1.0 - omega * omega;
        q *= 0.25;
    } else {
        p *= (1.0 - omega) * 0.5;
        q *= (1.0 + omega) * 0.5;
    }
    let amplitude = raw_amplitude as f64 * config.amplitude_offset as f64 / denominator as f64;
    ((amplitude / (p + q).sqrt() - config.amplitude_offset as f64) * 0.11512925).exp() as f32
}
fn bark(value: f64) -> f64 {
    13.1 * (0.00074 * value).atan() + 2.24 * (0.0000000185 * value * value).atan() + 0.0001 * value
}

struct BarkMap {
    rate: u16,
    size: u16,
    scale: f64,
    behavior: Floor0Behavior,
}
impl BarkMap {
    fn new(rate: u16, size: u16, behavior: Floor0Behavior) -> Self {
        let scale = if behavior == Floor0Behavior::Libvorbis {
            (size as f64 / bark_reference(rate as f32 / 2.0)) as f32 as f64
        } else {
            size as f64 / bark(rate as f64 * 0.5)
        };
        Self {
            rate,
            size,
            scale,
            behavior,
        }
    }
    fn index(&self, i: usize, n: usize) -> usize {
        let value = if self.behavior == Floor0Behavior::Libvorbis {
            bark_reference((self.rate as f32 / 2.0) / n as f32 * i as f32)
        } else {
            bark(self.rate as f64 * 0.5 * i as f64 / n as f64)
        };
        ((value * self.scale).floor() as usize).min(self.size as usize - 1)
    }
}
fn bark_reference(value: f32) -> f64 {
    13.1f32 as f64 * ((0.00074f32 * value) as f64).atan()
        + 2.24f32 as f64 * ((value * value * 1.85e-8f32) as f64).atan()
        + (1e-4f32 * value) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsigned_amplitude_default_and_reference_integer_edges_stay_separate() {
        use crate::bitpack::BitWriter;
        let book = lsp_book(1);
        for width in [32u8, 33, 63] {
            let config = Floor0 {
                order: 1,
                rate: 8000,
                bark_map_size: 63,
                amplitude_bits: width,
                amplitude_offset: 30,
                books: vec![0],
            };
            let amplitude = 1u64 << (width - 1);
            let mut packet = BitWriter::new();
            packet
                .write(amplitude as u32, (width as u32).min(32))
                .unwrap();
            if width > 32 {
                packet
                    .write((amplitude >> 32) as u32, width as u32 - 32)
                    .unwrap();
            }
            packet.write(0, 2).unwrap();
            packet.write(21, 5).unwrap();
            let data = packet.finish().unwrap();
            let mut bits = BitReader::new(&data);
            let mut output = [0.0; 32];
            assert!(
                decode0(
                    &config,
                    std::slice::from_ref(&book),
                    &mut bits,
                    &mut output,
                    false,
                    Floor0Behavior::Vorbis
                )
                .unwrap()
            );
            assert_eq!(bits.position(), width as usize + 2);
            assert_eq!(bits.read(5), Ok(21));
            let amp = amplitude as f64 / ((1u64 << width) - 1) as f64 * 30.0;
            let expected = ((amp / (1.0 - 1.0f64.cos()) - 30.0) * 0.11512925).exp();
            assert!((output[0] as f64 - expected).abs() < 2e-6 * expected.max(1.0));
            let mut bits = BitReader::new(&data);
            let result = decode0(
                &config,
                std::slice::from_ref(&book),
                &mut bits,
                &mut output,
                true,
                Floor0Behavior::Libvorbis,
            );
            if width == 32 {
                assert_eq!(result, Ok(false));
                assert_eq!(bits.position(), 32);
                assert!(!bits.is_exhausted());
                assert_eq!(bits.read(7), Ok(21 << 2));
            } else {
                assert_eq!(result, Err(Error::EndOfPacket));
                assert!(bits.is_exhausted());
                assert_eq!(bits.read(0), Err(Error::EndOfPacket));
            }
        }
        let config = Floor0 {
            order: 1,
            rate: 8000,
            bark_map_size: 63,
            amplitude_bits: 32,
            amplitude_offset: 30,
            books: vec![0],
        };
        let mut packet = BitWriter::new();
        packet.write(1, 32).unwrap();
        packet.write(0, 2).unwrap();
        let data = packet.finish().unwrap();
        for allow in [false, true] {
            let mut output = [0.0; 32];
            let result = decode0(
                &config,
                std::slice::from_ref(&book),
                &mut BitReader::new(&data),
                &mut output,
                allow,
                Floor0Behavior::Libvorbis,
            );
            if allow {
                assert_eq!(result, Ok(true));
                assert!(output.iter().all(|v| *v == f32::INFINITY));
            } else {
                assert_eq!(result, Err(Error::Invalid("nonfinite floor0 curve")));
            }
        }
    }
    #[test]
    fn empty_floor_books_preserve_following_channel_bits() {
        use crate::{FloorClass, bitpack::BitWriter, codebook::empty_test_book};
        let books = [empty_test_book(1, 2)];
        for master in [false, true] {
            let config = Floor1 {
                partition_classes: vec![0],
                classes: vec![FloorClass {
                    dimensions: 1,
                    subclasses: u8::from(master),
                    masterbook: master.then_some(0),
                    books: [Some(0); 8],
                }],
                multiplier: 1,
                x: vec![0, 32, 16],
                neighbors: vec![[0, 0], [0, 0], [0, 1]],
                sorted_order: vec![0, 2, 1],
            };
            let mut packet = BitWriter::new();
            for (value, width) in [(1, 1), (200, 8), (200, 8), (21, 5)] {
                packet.write(value, width).unwrap();
            }
            let bytes = packet.finish().unwrap();
            let mut bits = BitReader::new(&bytes);
            let mut output = [0.125; 32];
            assert!(!decode1(&config, &books, &mut bits, &mut output).unwrap());
            assert_eq!(output, [0.125; 32]);
            assert_eq!(bits.position(), 17);
            assert!(!bits.is_exhausted());
            assert_eq!(bits.read(5), Ok(21));
        }
        let config = Floor0 {
            order: 1,
            rate: 8000,
            bark_map_size: 63,
            amplitude_bits: 6,
            amplitude_offset: 30,
            books: vec![0],
        };
        let mut packet = BitWriter::new();
        for (value, width) in [(7, 6), (0, 1), (21, 5)] {
            packet.write(value, width).unwrap();
        }
        let bytes = packet.finish().unwrap();
        for allow in [false, true] {
            let mut bits = BitReader::new(&bytes);
            let mut output = [0.0; 32];
            let result = decode0(
                &config,
                &books,
                &mut bits,
                &mut output,
                allow,
                Floor0Behavior::Vorbis,
            );
            if allow {
                assert_eq!(result, Ok(true));
                assert_eq!(output[0], f32::INFINITY);
                assert!(output[1..].iter().all(|n| n.is_finite()));
            } else {
                assert_eq!(result, Err(Error::Invalid("nonfinite floor0 curve")));
            }
            assert_eq!(bits.position(), 7);
            assert!(!bits.is_exhausted());
            assert_eq!(bits.read(5), Ok(21));
        }
    }
    fn lsp_book(order: u32) -> Codebook {
        use crate::{Limits, bitpack::BitWriter};
        let mut bits = BitWriter::new();
        for (value, width) in [
            (0x564342, 24),
            (order, 16),
            (1, 24),
            (0, 1),
            (0, 1),
            (0, 5),
            (2, 4),
            (0, 32),
            ((788 << 21) | 1, 32),
            (1, 4),
            (0, 1),
        ] {
            bits.write(value, width).unwrap();
        }
        for value in 1..=order {
            bits.write(value, 2).unwrap();
        }
        let bytes = bits.finish().unwrap();
        Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).unwrap()
    }
    #[test]
    fn floor0_high_order_gain_keeps_wide_range_fallback() {
        let config = Floor0 {
            order: 254,
            rate: 8000,
            bark_map_size: 96,
            amplitude_bits: 6,
            amplitude_offset: 30,
            books: vec![0],
        };
        let mut coefficients = [PI as f32; 254];
        coefficients[253] = (PI / 96.0) as f32 * 32.0;
        let cosine = coefficients.map(|x| (2.0 * (x as f64).cos()) as f32);
        let gain = floor0_gain(
            &config,
            7,
            &coefficients,
            &cosine,
            32,
            false,
            Floor0Behavior::Vorbis,
        );
        let expected = (-30.0f64 * 0.11512925).exp() as f32;
        assert!(gain.is_finite());
        assert_eq!(gain, expected);
    }
    #[test]
    fn floor0_invalid_selectors_leave_following_floor_bits_available() {
        use crate::bitpack::BitWriter;
        let book = lsp_book(1);
        for count in [1usize, 2, 3, 4, 7, 8, 15, 16] {
            for selector in count..1 << ilog(count as u32) {
                let config = Floor0 {
                    order: 1,
                    rate: 8000,
                    bark_map_size: 63,
                    amplitude_bits: 6,
                    amplitude_offset: 30,
                    books: vec![0; count],
                };
                let mut packet = BitWriter::new();
                packet.write(7, 6).unwrap();
                packet.write(selector as u32, ilog(count as u32)).unwrap();
                packet.write(21, 5).unwrap();
                let bytes = packet.finish().unwrap();
                let mut bits = BitReader::new(&bytes);
                let mut output = [0.125; 32];
                assert!(
                    !decode0(
                        &config,
                        std::slice::from_ref(&book),
                        &mut bits,
                        &mut output,
                        false,
                        Floor0Behavior::Vorbis,
                    )
                    .unwrap()
                );
                assert_eq!(output, [0.125; 32]);
                assert_eq!(bits.position(), 6 + ilog(count as u32) as usize);
                assert!(!bits.is_exhausted());
                assert_eq!(bits.read(5), Ok(21));
            }
        }
    }
    #[test]
    fn render_integer_line_and_prediction_use_spec_rounding() {
        let mut output = [0.0; 7];
        render_line(0, 10, 7, 0, &mut output);
        assert_eq!(output, [10, 9, 8, 6, 5, 3, 2].map(|n| INVERSE_DB[n]));
        assert_eq!(render_point(0, 10, 7, 0, 2), 8);
    }
    #[test]
    fn floor0_odd_even_products_match_closed_form_at_dc() {
        use crate::bitpack::BitWriter;
        for order in [1, 2] {
            let book = lsp_book(order);
            for amplitude_bits in [6, 63] {
                let config = Floor0 {
                    order: order as u8,
                    rate: 8000,
                    bark_map_size: 63,
                    amplitude_bits,
                    amplitude_offset: 30,
                    books: vec![0],
                };
                let mut packet = BitWriter::new();
                packet.write(7, amplitude_bits.min(32) as u32).unwrap();
                if amplitude_bits > 32 {
                    packet.write(0, amplitude_bits as u32 - 32).unwrap();
                }
                packet.write(0, 1).unwrap();
                packet.write(0, 1).unwrap();
                let packet = packet.finish().unwrap();
                let mut output = [0.0; 32];
                assert!(
                    decode0(
                        &config,
                        std::slice::from_ref(&book),
                        &mut BitReader::new(&packet),
                        &mut output,
                        false,
                        Floor0Behavior::Vorbis,
                    )
                    .unwrap()
                );
                // At omega=0, the odd-order product is (cos(1)-1)^2;
                // for order two it is four times that value. No Bark mapping
                // or implementation product loop is used in this oracle.
                let denominator = (1.0 - 1.0f64.cos()) * order as f64;
                let amplitude = 7.0 * 30.0 / ((1u64 << amplitude_bits) - 1) as f64;
                let expected = ((amplitude / denominator - 30.0) * 0.11512925).exp();
                assert!((output[0] as f64 - expected).abs() < 1e-7 * expected);
                assert!(output.iter().all(|value| value.is_finite()));
            }
        }
    }
}
