use std::{arch::x86_64::*, mem::MaybeUninit};

use super::{Error, Header, display_rows, reserve};

pub(super) fn bgr_rgba<const INPUT: usize>(
    source: &[u8],
    header: &Header,
) -> Option<Result<Vec<u8>, Error>> {
    const { assert!(matches!(INPUT, 3 | 4)) };
    if header.pixel_count < 512 || !std::arch::is_x86_feature_detected!("ssse3") {
        return None;
    }
    let swizzle: unsafe fn(&[u8], &mut [MaybeUninit<[u8; 4]>]) =
        if std::arch::is_x86_feature_detected!("avx2") {
            swizzle_avx2::<INPUT>
        } else {
            swizzle_ssse3::<INPUT>
        };
    let mut samples = Vec::<[u8; 4]>::new();
    if let Err(error) = reserve(&mut samples, header.pixel_count) {
        return Some(Err(error));
    }
    let output = &mut samples.spare_capacity_mut()[..header.pixel_count];
    let width = header.info.width as usize;
    let row_bytes = width * INPUT;
    assert_eq!(source.len(), header.source_bytes);
    assert!(row_bytes <= header.row_pitch);
    if header.info.top_down && header.row_pitch == row_bytes {
        // SAFETY: The selected CPU feature was detected above. The validated
        // raster contains exactly INPUT bytes for every output pixel.
        unsafe { swizzle(source, output) };
    } else {
        let rows = display_rows(source, header.row_pitch, header.info.top_down);
        for (row, out) in rows.zip(output.chunks_exact_mut(width)) {
            // SAFETY: The selected CPU feature was detected above. Each
            // validated source row has INPUT bytes for every output pixel.
            unsafe { swizzle(&row[..row_bytes], out) };
        }
    }
    // SAFETY: The validated raster and output have the same dimensions.
    // The conversion initialized four bytes for every output pixel.
    unsafe { samples.set_len(header.pixel_count) };
    Some(Ok(samples.into_flattened()))
}

#[target_feature(enable = "ssse3")]
fn shuffle_mask<const INPUT: usize>() -> __m128i {
    if INPUT == 3 {
        _mm_setr_epi8(2, 1, 0, -1, 5, 4, 3, -1, 8, 7, 6, -1, 11, 10, 9, -1)
    } else {
        _mm_setr_epi8(2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15)
    }
}

#[target_feature(enable = "ssse3")]
unsafe fn swizzle_ssse3<const INPUT: usize>(input: &[u8], output: &mut [MaybeUninit<[u8; 4]>]) {
    debug_assert_eq!(input.len(), output.len() * INPUT);
    let shuffle = shuffle_mask::<INPUT>();
    let alpha = _mm_set1_epi32(0xff00_0000u32 as i32);
    let block_bytes = 4 * INPUT;
    // The BGR load needs four more readable bytes than its payload.
    let groups = input.len().saturating_sub(16 - block_bytes) / block_bytes;
    for group in 0..groups {
        // SAFETY: The load stays entirely within input. Each 16-byte store
        // initializes four distinct output pixels, within output's length.
        unsafe {
            let bgr = _mm_loadu_si128(input.as_ptr().add(group * block_bytes).cast());
            let lanes = _mm_shuffle_epi8(bgr, shuffle);
            let rgba = if INPUT == 3 {
                _mm_or_si128(lanes, alpha)
            } else {
                lanes
            };
            _mm_storeu_si128(output.as_mut_ptr().add(group * 4).cast(), rgba);
        }
    }
    swizzle_tail::<INPUT>(&input[groups * block_bytes..], &mut output[groups * 4..]);
}

#[target_feature(enable = "avx2,ssse3")]
unsafe fn swizzle_avx2<const INPUT: usize>(input: &[u8], output: &mut [MaybeUninit<[u8; 4]>]) {
    debug_assert_eq!(input.len(), output.len() * INPUT);
    let shuffle = _mm256_broadcastsi128_si256(shuffle_mask::<INPUT>());
    let alpha = _mm256_set1_epi32(0xff00_0000u32 as i32);
    let block_bytes = 8 * INPUT;
    // For BGR, the last 16-byte load begins 12 bytes into the block.
    let groups = input.len().saturating_sub(if INPUT == 3 { 4 } else { 0 }) / block_bytes;
    for group in 0..groups {
        // SAFETY: The full loads fit the input extent, including the four
        // extra readable bytes for BGR. Each store initializes eight pixels.
        unsafe {
            let ptr = input.as_ptr().add(group * block_bytes);
            let bgr = if INPUT == 3 {
                let lo = _mm_loadu_si128(ptr.cast());
                let hi = _mm_loadu_si128(ptr.add(12).cast());
                _mm256_set_m128i(hi, lo)
            } else {
                _mm256_loadu_si256(ptr.cast())
            };
            let lanes = _mm256_shuffle_epi8(bgr, shuffle);
            let rgba = if INPUT == 3 {
                _mm256_or_si256(lanes, alpha)
            } else {
                lanes
            };
            _mm256_storeu_si256(output.as_mut_ptr().add(group * 8).cast(), rgba);
        }
    }
    swizzle_tail::<INPUT>(&input[groups * block_bytes..], &mut output[groups * 8..]);
}

fn swizzle_tail<const INPUT: usize>(input: &[u8], output: &mut [MaybeUninit<[u8; 4]>]) {
    for (out, pixel) in output.iter_mut().zip(input.as_chunks::<INPUT>().0) {
        let alpha = if INPUT == 3 { 255 } else { pixel[3] };
        out.write([pixel[2], pixel[1], pixel[0], alpha]);
    }
}

pub(super) fn expand_small_palette(
    source: &[u8],
    header: &Header,
    colors: &[[u8; 4]; 256],
    palette_len: usize,
    output: &mut [MaybeUninit<[u8; 4]>],
) -> Option<Result<(), Error>> {
    if palette_len > 16 || header.pixel_count < 4096 || !std::arch::is_x86_feature_detected!("avx2")
    {
        return None;
    }
    assert_eq!(output.len(), header.pixel_count);
    assert_eq!(source.len(), header.source_bytes);
    assert_ne!(palette_len, 0);
    // SAFETY: AVX2 was detected above. The parsed 8-bit raster has one
    // index per output pixel and palette_len is in 1..=16.
    Some(unsafe { expand_avx2_palette(source, header, colors, palette_len, output) })
}

#[target_feature(enable = "avx2")]
unsafe fn expand_avx2_palette(
    source: &[u8],
    header: &Header,
    colors: &[[u8; 4]; 256],
    palette_len: usize,
    output: &mut [MaybeUninit<[u8; 4]>],
) -> Result<(), Error> {
    let planes: [[u8; 16]; 3] = std::array::from_fn(|c| std::array::from_fn(|i| colors[i][c]));
    let [red, green, blue] = planes.each_ref().map(|plane| {
        // SAFETY: A plane contains exactly the 16 bytes loaded here.
        unsafe { _mm256_broadcastsi128_si256(_mm_loadu_si128(plane.as_ptr().cast())) }
    });
    let alpha = _mm256_set1_epi8(-1);
    let mut largest = _mm256_setzero_si256();
    let mut last = 0;
    let mut expand = |row: &[u8], out: &mut [MaybeUninit<[u8; 4]>]| {
        let (blocks, tail) = row.as_chunks::<32>();
        let (output, remaining) = out.as_chunks_mut::<32>();
        for (indices, out) in blocks.iter().zip(output) {
            // SAFETY: The input block contains 32 indices and the output block
            // has room for all 32 four-byte pixels. AVX2 is enabled here.
            unsafe {
                let indices = _mm256_loadu_si256(indices.as_ptr().cast());
                largest = _mm256_max_epu8(largest, indices);
                let r = _mm256_shuffle_epi8(red, indices);
                let g = _mm256_shuffle_epi8(green, indices);
                let b = _mm256_shuffle_epi8(blue, indices);
                let rg_lo = _mm256_unpacklo_epi8(r, g);
                let rg_hi = _mm256_unpackhi_epi8(r, g);
                let ba_lo = _mm256_unpacklo_epi8(b, alpha);
                let ba_hi = _mm256_unpackhi_epi8(b, alpha);
                let p0 = _mm256_unpacklo_epi16(rg_lo, ba_lo);
                let p1 = _mm256_unpackhi_epi16(rg_lo, ba_lo);
                let p2 = _mm256_unpacklo_epi16(rg_hi, ba_hi);
                let p3 = _mm256_unpackhi_epi16(rg_hi, ba_hi);
                let out = out.as_mut_ptr().cast::<__m256i>();
                _mm256_storeu_si256(out, _mm256_permute2x128_si256::<0x20>(p0, p1));
                _mm256_storeu_si256(out.add(1), _mm256_permute2x128_si256::<0x20>(p2, p3));
                _mm256_storeu_si256(out.add(2), _mm256_permute2x128_si256::<0x31>(p0, p1));
                _mm256_storeu_si256(out.add(3), _mm256_permute2x128_si256::<0x31>(p2, p3));
            }
        }
        for (&index, pixel) in tail.iter().zip(remaining) {
            last = last.max(index);
            pixel.write(colors[usize::from(index)]);
        }
    };
    let width = header.info.width as usize;
    if header.info.top_down && header.row_pitch == width {
        expand(source, output);
    } else {
        let rows = display_rows(source, header.row_pitch, header.info.top_down);
        for (row, out) in rows.zip(output.chunks_exact_mut(width)) {
            expand(&row[..width], out);
        }
    }
    // Check the real indices even though the byte shuffle uses their low bits.
    let above = _mm256_subs_epu8(largest, _mm256_set1_epi8((palette_len - 1) as i8));
    if usize::from(last) >= palette_len || _mm256_testz_si256(above, above) == 0 {
        return Err(Error::Invalid("pixel index exceeds the palette"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_rows<const INPUT: usize>(swizzle: unsafe fn(&[u8], &mut [MaybeUninit<[u8; 4]>])) {
        for width in 0..65 {
            let expected: Vec<_> = (0..width)
                .map(|x| {
                    let alpha = if INPUT == 3 { 255 } else { (x * 37) as u8 };
                    [
                        (x * 19 + 13) as u8,
                        (x * 11 + 41) as u8,
                        (x * 71 + 7) as u8,
                        alpha,
                    ]
                })
                .collect();
            let input: Vec<_> = expected
                .iter()
                .flat_map(|&[r, g, b, a]| [b, g, r, a].into_iter().take(INPUT))
                .collect();
            let mut actual = vec![MaybeUninit::new([0; 4]); width];
            // SAFETY: The test detects the kernel's feature before calling
            // this helper. There are exactly INPUT bytes per output pixel.
            unsafe { swizzle(&input, &mut actual) };
            for (pixel, expected) in actual.into_iter().zip(expected) {
                // SAFETY: Every slot started initialized, and the kernel
                // only replaces slots with initialized pixels.
                assert_eq!(unsafe { pixel.assume_init() }, expected, "width {width}");
            }
        }
    }

    #[test]
    fn vector_and_scalar_tails_agree_at_every_block_boundary() {
        if std::arch::is_x86_feature_detected!("ssse3") {
            check_rows::<3>(swizzle_ssse3::<3>);
            check_rows::<4>(swizzle_ssse3::<4>);
            if std::arch::is_x86_feature_detected!("avx2") {
                check_rows::<3>(swizzle_avx2::<3>);
                check_rows::<4>(swizzle_avx2::<4>);
            }
        }
    }
}
