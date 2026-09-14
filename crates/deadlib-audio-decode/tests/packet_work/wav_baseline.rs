// Frozen from 0.5.1218 (c5fb50443).
#[derive(Clone, Copy)]
enum Encoding {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
    Float64,
}

impl Encoding {
    const fn sample_bytes(self) -> usize {
        match self {
            Self::Pcm8 => 1,
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Pcm32 | Self::Float32 => 4,
            Self::Float64 => 8,
        }
    }
}

fn decode_packet_into(
    bytes: &[u8],
    encoding: Encoding,
    out: &mut Vec<i16>,
) -> Result<(), &'static str> {
    if !bytes.len().is_multiple_of(encoding.sample_bytes()) {
        return Err("WAV packet ended mid-sample");
    }
    let samples = bytes.len() / encoding.sample_bytes();
    if matches!(encoding, Encoding::Float32 | Encoding::Float64) {
        resize_output(out, samples);
    } else {
        out.clear();
        out.reserve(samples);
    }
    match encoding {
        Encoding::Pcm8 => out.extend(bytes.iter().map(|sample| (i16::from(*sample) - 128) << 8)),
        Encoding::Pcm16 => out.extend(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes(*sample)),
        ),
        Encoding::Pcm24 => out.extend(bytes.as_chunks::<3>().0.iter().map(pcm24_to_i16)),
        Encoding::Pcm32 => out.extend(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|sample| (i32::from_le_bytes(*sample) >> 16) as i16),
        ),
        Encoding::Float32 => {
            for (output, sample) in out.iter_mut().zip(bytes.as_chunks::<4>().0) {
                *output = float_to_i16(f64::from(f32::from_le_bytes(*sample)));
            }
        }
        Encoding::Float64 => {
            for (output, sample) in out.iter_mut().zip(bytes.as_chunks::<8>().0) {
                *output = float_to_i16(f64::from_le_bytes(*sample));
            }
        }
    }
    Ok(())
}

#[inline(always)]
const fn pcm24_to_i16(sample: &[u8; 3]) -> i16 {
    let signed = i32::from_le_bytes([
        sample[0],
        sample[1],
        sample[2],
        if sample[2] & 0x80 != 0 { 0xff } else { 0x00 },
    ]);
    (signed >> 8) as i16
}

#[inline(always)]
fn float_to_i16(value: f64) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    (value * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

#[inline(always)]
fn resize_output(out: &mut Vec<i16>, samples: usize) {
    if out.len() < samples {
        out.resize(samples, 0);
    } else {
        out.truncate(samples);
    }
}
