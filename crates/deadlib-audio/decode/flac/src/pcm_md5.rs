//! Specialized PCM serialization kept outside the frame reconstruction code.
use crate::{Pcm, checksum::Md5};

#[inline(never)]
pub(crate) fn update(pcm: &Pcm<'_>, digest: &mut Md5, bits: u8) {
    macro_rules! channels {
        ($bytes:literal) => {
            match pcm.channels() {
                1 => packed::<$bytes, 1>(pcm, digest),
                2 => packed::<$bytes, 2>(pcm, digest),
                3 => packed::<$bytes, 3>(pcm, digest),
                4 => packed::<$bytes, 4>(pcm, digest),
                5 => packed::<$bytes, 5>(pcm, digest),
                6 => packed::<$bytes, 6>(pcm, digest),
                7 => packed::<$bytes, 7>(pcm, digest),
                8 => packed::<$bytes, 8>(pcm, digest),
                _ => unreachable!(),
            }
        };
    }
    match bits.div_ceil(8) {
        0 => (),
        1 => channels!(1),
        2 => channels!(2),
        3 => channels!(3),
        4 => channels!(4),
        _ => panic!("PCM MD5 depth exceeds 32 bits"),
    }
}

fn packed<const BYTES: usize, const CHANNELS: usize>(pcm: &Pcm<'_>, digest: &mut Md5) {
    let mut buffer = [0u8; 4096];
    let row_bytes = BYTES * CHANNELS;
    let batch = buffer.len() / row_bytes;
    for start in (0..pcm.samples()).step_by(batch) {
        let count = batch.min(pcm.samples() - start);
        let planes: [&[i32]; CHANNELS] =
            std::array::from_fn(|channel| &pcm.plane(channel)[start..start + count]);
        let output = &mut buffer[..count * row_bytes];
        for (row, sample) in output.chunks_exact_mut(row_bytes).zip(0..count) {
            for (channel, plane) in planes.iter().enumerate() {
                let value = plane[sample].to_le_bytes();
                row[channel * BYTES..channel * BYTES + BYTES].copy_from_slice(&value[..BYTES]);
            }
        }
        digest.update(output);
    }
}
