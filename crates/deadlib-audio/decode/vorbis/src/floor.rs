use crate::{
    Error, Floor, Floor0, Floor1,
    bitpack::{BitReader, ilog},
    codebook::Codebook,
    floor_table::INVERSE_DB,
};
use core::f64::consts::PI;

/// EndOfPacket is deliberately preserved so the packet decoder can implement
/// section 4.3.2's all-channel zeroing rule before overlap/add.
pub(crate) fn decode(
    config: &Floor,
    books: &[Codebook],
    bits: &mut BitReader<'_>,
    output: &mut [f32],
) -> Result<bool, Error> {
    match config {
        Floor::Type0(config) => decode0(config, books, bits, output),
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
            Some(book) => books[book as usize].decode_scalar(bits)?,
            None => 0,
        };
        let mask = (1 << class.subclasses) - 1;
        for value in &mut y[offset..offset + class.dimensions as usize] {
            *value = match class.books[(selection & mask) as usize] {
                Some(book) => books[book as usize].decode_scalar(bits)? as i64,
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
) -> Result<bool, Error> {
    let width = config.amplitude_bits as u32;
    let amplitude = if width <= 32 {
        bits.read(width)? as u64
    } else {
        bits.read(32)? as u64 | (bits.read(width - 32)? as u64) << 32
    };
    if amplitude == 0 {
        return Ok(false);
    }
    let book = bits.read(ilog(config.books.len() as u32))? as usize;
    let book = config
        .books
        .get(book)
        .ok_or(Error::Invalid("floor0 book number"))?;
    let book = &books[*book as usize];
    let order = config.order as usize;
    let mut coefficients = [0.0f32; 255];
    let mut position = 0;
    let mut last = 0.0;
    while position < order {
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
    // Evaluate the normative LSP products in f64 to preserve the full dynamic
    // range of high-order floors. Only the final linear gain is rounded to f32.
    let mut cosine = [0.0; 255];
    for (value, &coefficient) in cosine.iter_mut().zip(&coefficients[..order]) {
        *value = (coefficient as f64).cos();
    }
    let amplitude =
        amplitude as f64 * config.amplitude_offset as f64 / ((1u64 << width) - 1) as f64;
    let scale = config.bark_map_size as f64 / bark(config.rate as f64 * 0.5);
    let mut previous_map = usize::MAX;
    let mut gain = 0.0;
    let n = output.len();
    for (i, value) in output.iter_mut().enumerate() {
        let map = ((bark(config.rate as f64 * 0.5 * i as f64 / n as f64) * scale).floor() as usize)
            .min(config.bark_map_size as usize - 1);
        if map != previous_map {
            let omega = (PI * map as f64 / config.bark_map_size as f64).cos();
            let mut p = 1.0;
            let mut q = 1.0;
            for (j, &coefficient) in cosine[..order].iter().enumerate() {
                let term = 2.0 * (coefficient - omega);
                if j & 1 == 0 {
                    q *= term * term;
                } else {
                    p *= term * term;
                }
            }
            if order & 1 != 0 {
                p *= 1.0 - omega * omega;
                q *= 0.25;
            } else {
                p *= (1.0 - omega) * 0.5;
                q *= (1.0 + omega) * 0.5;
            }
            gain = ((amplitude / (p + q).sqrt() - config.amplitude_offset as f64) * 0.11512925)
                .exp() as f32;
            if !gain.is_finite() {
                return Err(Error::Invalid("nonfinite floor0 curve"));
            }
            previous_map = map;
        }
        *value = gain;
    }
    Ok(true)
}
fn bark(value: f64) -> f64 {
    13.1 * (0.00074 * value).atan() + 2.24 * (0.0000000185 * value * value).atan() + 0.0001 * value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_integer_line_and_prediction_use_spec_rounding() {
        let mut output = [0.0; 7];
        render_line(0, 10, 7, 0, &mut output);
        assert_eq!(output, [10, 9, 8, 6, 5, 3, 2].map(|n| INVERSE_DB[n]));
        assert_eq!(render_point(0, 10, 7, 0, 2), 8);
    }
    #[test]
    fn floor0_odd_even_products_match_closed_form_at_dc() {
        use crate::{Limits, bitpack::BitWriter};
        for order in [1, 2] {
            let mut book = BitWriter::new();
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
                book.write(value, width).unwrap();
            }
            for value in 1..=order {
                book.write(value, 2).unwrap();
            }
            let bytes = book.finish().unwrap();
            let book = Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).unwrap();
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
                        &mut output
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
