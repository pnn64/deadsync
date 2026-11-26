use std::{arch::x86_64::*, mem::MaybeUninit};

use super::{Error, Header, display_rows, reserve};

pub(super) fn rgb24_rgba(source: &[u8], header: &Header) -> Option<Result<Vec<u8>, Error>> {
    if !std::arch::is_x86_feature_detected!("ssse3") {
        return None;
    }
    let mut samples = Vec::<[u8; 4]>::new();
    if let Err(error) = reserve(&mut samples, header.pixel_count) {
        return Some(Err(error));
    }
    let output = &mut samples.spare_capacity_mut()[..header.pixel_count];
    let width = header.info.width as usize;
    let row_bytes = width * 3;
    if header.info.top_down && header.row_pitch == row_bytes {
        // SAFETY: SSSE3 was detected above. The parsed raster describes one
        // tightly packed BGR triple per output pixel.
        unsafe { swizzle_bgr(source, output) };
    } else {
        let rows = display_rows(source, header.row_pitch, header.info.top_down);
        for (row, out) in rows.zip(output.chunks_exact_mut(width)) {
            // SAFETY: SSSE3 was detected above. Trimming the validated row
            // to its pixel bytes leaves exactly one triple per output pixel.
            unsafe { swizzle_bgr(&row[..row_bytes], out) };
        }
    }
    // SAFETY: The validated raster and output have the same dimensions.
    // The conversion initialized four bytes for every output pixel.
    unsafe { samples.set_len(header.pixel_count) };
    Some(Ok(samples.into_flattened()))
}

#[target_feature(enable = "ssse3")]
unsafe fn swizzle_bgr(input: &[u8], output: &mut [MaybeUninit<[u8; 4]>]) {
    debug_assert_eq!(input.len(), output.len() * 3);
    let shuffle = _mm_setr_epi8(2, 1, 0, -1, 5, 4, 3, -1, 8, 7, 6, -1, 11, 10, 9, -1);
    let alpha = _mm_set1_epi32(0xff00_0000u32 as i32);
    // Each iteration consumes 12 bytes. Leave four extra readable bytes
    // for the 16-byte load and finish the remaining pixels individually.
    let groups = input.len().saturating_sub(4) / 12;
    for group in 0..groups {
        // SAFETY: The load stays entirely within input. Each 16-byte store
        // initializes four distinct output pixels, within output's length.
        unsafe {
            let bgr = _mm_loadu_si128(input.as_ptr().add(group * 12).cast());
            let rgba = _mm_or_si128(_mm_shuffle_epi8(bgr, shuffle), alpha);
            _mm_storeu_si128(output.as_mut_ptr().add(group * 4).cast(), rgba);
        }
    }
    let tail = input[groups * 12..].as_chunks::<3>().0;
    for (out, &[b, g, r]) in output[groups * 4..].iter_mut().zip(tail) {
        out.write([r, g, b, 255]);
    }
}
