//! H.264 inter prediction / motion compensation (spec 8.4.2).
//!
//! Generates predicted blocks for P/B slices by interpolating pixels from
//! reference frames at quarter-pel (luma) or eighth-pel (chroma) precision.

use crate::dpb::{ChromaRef, DecodedPicture};
use crate::pixel::Pixel;

/// View of the published luma rows of a frame or field.
struct LumaView<'a> {
    y: &'a [u8],
    stride: usize,
    width: i32,
    height: i32,
}

/// Fetch a luma sample from the reference picture with boundary clipping.
/// Out-of-bounds coordinates are clamped to the picture edge (spec 8.4.2.2.1).
#[inline]
fn ref_luma(pic: &LumaView<'_>, x: i32, y: i32) -> i32 {
    let cx = x.clamp(0, pic.width - 1) as usize;
    let cy = y.clamp(0, pic.height - 1) as usize;
    pic.y[cy * pic.stride + cx] as i32
}

/// Fetch a chroma sample with boundary clipping.
#[inline]
fn ref_chroma(plane: &[u8], stride: usize, width: usize, height: usize, x: i32, y: i32) -> i32 {
    if width == 0 || height == 0 {
        return 0;
    }
    let cx = x.clamp(0, width as i32 - 1) as usize;
    let cy = y.clamp(0, height as i32 - 1) as usize;
    let idx = cy * stride + cx;
    if idx >= plane.len() {
        return 0;
    }
    plane[idx] as i32
}

/// Copy a short row (a block row of 2 to 16 samples). Block widths are a
/// few fixed sizes; copying them as fixed-size arrays avoids a `memmove`
/// call per row.
#[inline(always)]
pub(crate) fn copy_row(dst: &mut [u8], src: &[u8]) {
    match src.len() {
        16 => dst[..16].copy_from_slice(&src[..16]),
        8 => dst[..8].copy_from_slice(&src[..8]),
        4 => dst[..4].copy_from_slice(&src[..4]),
        2 => dst[..2].copy_from_slice(&src[..2]),
        n => dst[..n].copy_from_slice(src),
    }
}

#[inline]
pub(crate) fn clip_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline]
pub(crate) fn avg(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16 + 1) >> 1) as u8
}

/// 6-tap FIR filter coefficient application on 6 consecutive samples.
#[inline(always)]
pub(crate) fn fir6(s: &[u8], i: usize) -> i32 {
    s[i] as i32 - 5 * s[i + 1] as i32 + 20 * s[i + 2] as i32 + 20 * s[i + 3] as i32
        - 5 * s[i + 4] as i32
        + s[i + 5] as i32
}

/// 6-tap FIR on i32 intermediates (for hv second pass).
#[inline(always)]
pub(crate) fn fir6_i32(s0: i32, s1: i32, s2: i32, s3: i32, s4: i32, s5: i32) -> i32 {
    s0 - 5 * s1 + 20 * s2 + 20 * s3 - 5 * s4 + s5
}

/// 6-tap horizontal half-pel filter at integer position (x, y).
/// Returns clipped u8 result. Used only for boundary blocks.
fn half_pel_h(pic: &LumaView<'_>, x: i32, y: i32) -> u8 {
    let s = |dx: i32| ref_luma(pic, x + dx, y);
    clip_u8((s(-2) - 5 * s(-1) + 20 * s(0) + 20 * s(1) - 5 * s(2) + s(3) + 16) >> 5)
}

/// 6-tap vertical half-pel filter at integer position (x, y).
/// Used only for boundary blocks.
fn half_pel_v(pic: &LumaView<'_>, x: i32, y: i32) -> u8 {
    let s = |dy: i32| ref_luma(pic, x, y + dy);
    clip_u8((s(-2) - 5 * s(-1) + 20 * s(0) + 20 * s(1) - 5 * s(2) + s(3) + 16) >> 5)
}

/// Diagonal half-pel. Used only for boundary blocks.
fn half_pel_hv(pic: &LumaView<'_>, x: i32, y: i32) -> u8 {
    let mut h = [0i32; 6];
    for (i, dy) in (-2..=3).enumerate() {
        let s = |dx: i32| ref_luma(pic, x + dx, y + dy);
        h[i] = s(-2) - 5 * s(-1) + 20 * s(0) + 20 * s(1) - 5 * s(2) + s(3);
    }
    let val = h[0] - 5 * h[1] + 20 * h[2] + 20 * h[3] - 5 * h[4] + h[5];
    clip_u8((val + 512) >> 10)
}

/// Per-pixel interpolation fallback for boundary blocks.
fn luma_interp(pic: &LumaView<'_>, x: i32, y: i32, frac_x: i32, frac_y: i32) -> u8 {
    match (frac_x, frac_y) {
        (0, 0) => ref_luma(pic, x, y) as u8,
        (2, 0) => half_pel_h(pic, x, y),
        (0, 2) => half_pel_v(pic, x, y),
        (2, 2) => half_pel_hv(pic, x, y),
        (1, 0) => avg(ref_luma(pic, x, y) as u8, half_pel_h(pic, x, y)),
        (3, 0) => avg(half_pel_h(pic, x, y), ref_luma(pic, x + 1, y) as u8),
        (0, 1) => avg(ref_luma(pic, x, y) as u8, half_pel_v(pic, x, y)),
        (0, 3) => avg(half_pel_v(pic, x, y), ref_luma(pic, x, y + 1) as u8),
        (2, 1) => avg(half_pel_h(pic, x, y), half_pel_hv(pic, x, y)),
        (2, 3) => avg(half_pel_hv(pic, x, y), half_pel_h(pic, x, y + 1)),
        (1, 2) => avg(half_pel_v(pic, x, y), half_pel_hv(pic, x, y)),
        (3, 2) => avg(half_pel_hv(pic, x, y), half_pel_v(pic, x + 1, y)),
        (1, 1) => avg(half_pel_h(pic, x, y), half_pel_v(pic, x, y)),
        (3, 1) => avg(half_pel_h(pic, x, y), half_pel_v(pic, x + 1, y)),
        (1, 3) => avg(half_pel_v(pic, x, y), half_pel_h(pic, x, y + 1)),
        (3, 3) => avg(half_pel_v(pic, x + 1, y), half_pel_h(pic, x, y + 1)),
        _ => unreachable!(),
    }
}

/// Check if a block with the given filter margins is fully within bounds.
/// For half-pel filters, margin is 3 (needs x-2..x+w+2, y-2..y+h+2).
/// For full-pel, margin_left/top=0, margin_right/bottom=0.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn block_in_bounds(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pic_w: i32,
    pic_h: i32,
    margin_left: i32,
    margin_top: i32,
    margin_right: i32,
    margin_bottom: i32,
) -> bool {
    x - margin_left >= 0
        && y - margin_top >= 0
        && x + w + margin_right <= pic_w
        && y + h + margin_bottom <= pic_h
}

/// Luma motion compensation with explicit reference stride and buffer offset.
/// For field-coded MBAFF MBs, pass `ref_stride = ref_pic.width * 2`,
/// `ref_y_offset = ref_pic.width` (for bottom field) or `0` (for top field),
/// and `y` in field-line units.
#[allow(clippy::too_many_arguments)]
pub fn luma_mc_stride(
    ref_pic: &DecodedPicture,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
    ref_stride: usize,
    ref_y_offset: usize,
) {
    // Guard against malformed block sizes that would overrun the output buffer
    if block_w == 0 || block_h == 0 || block_w * block_h > output.len() {
        return;
    }
    let frac_x = dx.rem_euclid(4);
    let frac_y = dy.rem_euclid(4);
    // Integer part: arithmetic right shift gives floor division for negative values
    let x_int = x + (dx >> 2);
    let y_int = y + (dy >> 2);

    let pic_w = ref_pic.width as i32;
    let pic_h = if ref_stride == ref_pic.width as usize {
        ref_pic.height as i32
    } else {
        // Field-coded: effective height is half the frame
        (ref_pic.height / 2) as i32
    };
    let stride = ref_stride;
    let bw = block_w as i32;
    let bh = block_h as i32;
    // Wait until every reference row this call can read is final (the
    // filters reach 2 rows above and 3 below, plus one for quarter-pel
    // offsets; one more row of margin). Complete pictures return at once.
    let last_row = (y_int + bh + 4).clamp(0, (pic_h - 1).max(0)) as usize;
    ref_pic.wait_luma(ref_y_offset + (last_row + 1) * ref_stride);
    let ref_y = &ref_pic.luma()[ref_y_offset..];

    // Determine margins needed for the filter type
    // Half-pel filters need 2 pixels before and 3 after the block
    let needs_h = frac_x != 0; // horizontal filter needed
    let needs_v = frac_y != 0; // vertical filter needed
    let margin_l = if needs_h { 2 } else { 0 };
    let margin_r = if needs_h { 3 } else { 0 };
    let margin_t = if needs_v { 2 } else { 0 };
    let margin_b = if needs_v { 3 } else { 0 };

    // Quarter-pel positions that average with an offset integer/half-pel sample
    // may need +1 in a direction
    let extra_r: i32 = match frac_x {
        3 => 1,
        _ => 0,
    };
    let extra_b: i32 = match frac_y {
        3 => 1,
        _ => 0,
    };

    if block_in_bounds(
        x_int,
        y_int,
        bw + extra_r,
        bh + extra_b,
        pic_w,
        pic_h,
        margin_l,
        margin_t,
        margin_r,
        margin_b,
    ) {
        // Fast path: entire block + filter margins are in bounds
        // Access reference buffer directly without per-pixel clamping
        luma_mc_inner(
            ref_y,
            stride,
            x_int as usize,
            y_int as usize,
            block_w,
            block_h,
            frac_x,
            frac_y,
            output,
        );
    } else {
        let view = LumaView {
            y: ref_y,
            stride,
            width: pic_w,
            height: pic_h,
        };
        luma_mc_edge(
            &view, x_int, y_int, block_w, block_h, frac_x, frac_y, output,
        );
    }
}

/// Extend the reference edge into a macroblock-sized tile with a filter halo,
/// then run the same interpolation as for an in-bounds block.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn luma_mc_edge(
    pic: &LumaView<'_>,
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    frac_x: i32,
    frac_y: i32,
    output: &mut [u8],
) {
    if w > 16 || h > 16 {
        for row in 0..h {
            for col in 0..w {
                output[row * w + col] =
                    luma_interp(pic, x + col as i32, y + row as i32, frac_x, frac_y);
            }
        }
        return;
    }

    let left = 2 * usize::from(frac_x != 0);
    let top = 2 * usize::from(frac_y != 0);
    let tile_w = w + 5 * usize::from(frac_x != 0);
    let tile_h = h + 5 * usize::from(frac_y != 0);
    let tile_x = x - left as i32;
    let tile_y = y - top as i32;
    let begin = (-tile_x).clamp(0, tile_w as i32) as usize;
    let end = (pic.width - tile_x).clamp(0, tile_w as i32) as usize;
    let mut tile = [0u8; 21 * 21];

    for (dy, dst) in tile[..tile_w * tile_h].chunks_exact_mut(tile_w).enumerate() {
        let sy = (tile_y + dy as i32).clamp(0, pic.height - 1) as usize;
        let row_start = sy * pic.stride;
        let src = &pic.y[row_start..row_start + pic.width as usize];
        dst[..begin].fill(src[0]);
        if begin < end {
            let sx = (tile_x + begin as i32) as usize;
            dst[begin..end].copy_from_slice(&src[sx..sx + end - begin]);
        }
        dst[end..].fill(src[src.len() - 1]);
    }

    luma_mc_inner(&tile, tile_w, left, top, w, h, frac_x, frac_y, output);
}

/// Inner loop for in-bounds luma MC. All reference accesses are unchecked
/// (bounds already verified by caller). Dispatches on fractional position
/// once, then processes all rows with direct buffer access.
#[allow(clippy::too_many_arguments)]
fn luma_mc_inner(
    ref_y: &[u8],
    stride: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    frac_x: i32,
    frac_y: i32,
    output: &mut [u8],
) {
    // Helper: get a row slice starting at (x + dx, y + dy) with length len
    let row = |dy: isize, dx: isize, len: usize| -> &[u8] {
        let off = (y as isize + dy) as usize * stride + (x as isize + dx) as usize;
        &ref_y[off..off + len]
    };

    // Helper: get 6 vertically adjacent rows for vertical/diagonal filters
    let vrows = |dx: isize, dy_base: isize, len: usize| -> [&[u8]; 6] {
        [
            row(dy_base - 2, dx, len),
            row(dy_base - 1, dx, len),
            row(dy_base, dx, len),
            row(dy_base + 1, dx, len),
            row(dy_base + 2, dx, len),
            row(dy_base + 3, dx, len),
        ]
    };

    match (frac_x, frac_y) {
        (0, 0) => {
            // Full-pel copy
            for r in 0..h {
                let src = row(r as isize, 0, w);
                copy_row(&mut output[r * w..(r + 1) * w], src);
            }
        }
        (2, 0) => {
            // Half-pel horizontal
            for r in 0..h {
                let src = row(r as isize, -2, w + 5);
                crate::simd::row_half_pel_h(src, &mut output[r * w..], w);
            }
        }
        (0, 2) => {
            // Half-pel vertical
            for r in 0..h {
                let rows = vrows(0, r as isize, w);
                crate::simd::row_half_pel_v(rows, &mut output[r * w..], w);
            }
        }
        (2, 2) => {
            // Half-pel diagonal
            for r in 0..h {
                let rows = vrows(-2, r as isize, w + 5);
                crate::simd::row_half_pel_hv(rows, &mut output[r * w..], w);
            }
        }
        (1, 0) => {
            // Quarter-pel: avg(integer, half_h)
            for r in 0..h {
                let int_row = row(r as isize, 0, w);
                let src_h = row(r as isize, -2, w + 5);
                crate::simd::row_avg_int_h(int_row, src_h, &mut output[r * w..], w);
            }
        }
        (3, 0) => {
            // Quarter-pel: avg(half_h, integer+1)
            for r in 0..h {
                let int_row = row(r as isize, 1, w);
                let src_h = row(r as isize, -2, w + 5);
                crate::simd::row_avg_h_int(src_h, int_row, &mut output[r * w..], w);
            }
        }
        (0, 1) => {
            // Quarter-pel: avg(integer, half_v)
            for r in 0..h {
                let int_row = row(r as isize, 0, w);
                let rows = vrows(0, r as isize, w);
                crate::simd::row_avg_int_v(int_row, rows, &mut output[r * w..], w);
            }
        }
        (0, 3) => {
            // Quarter-pel: avg(half_v, integer_below)
            for r in 0..h {
                let int_row = row(r as isize + 1, 0, w);
                let rows = vrows(0, r as isize, w);
                crate::simd::row_avg_int_v(int_row, rows, &mut output[r * w..], w);
            }
        }
        (2, 1) => {
            // avg(half_h, half_hv)
            for r in 0..h {
                let src_h = row(r as isize, -2, w + 5);
                let rows_hv = vrows(-2, r as isize, w + 5);
                crate::simd::row_avg_h_hv(src_h, rows_hv, &mut output[r * w..], w);
            }
        }
        (2, 3) => {
            // avg(half_hv, half_h_below)
            for r in 0..h {
                let src_h = row(r as isize + 1, -2, w + 5);
                let rows_hv = vrows(-2, r as isize, w + 5);
                crate::simd::row_avg_h_hv(src_h, rows_hv, &mut output[r * w..], w);
            }
        }
        (1, 2) => {
            // avg(half_v, half_hv)
            for r in 0..h {
                let rows_v = vrows(0, r as isize, w);
                let rows_hv = vrows(-2, r as isize, w + 5);
                crate::simd::row_avg_v_hv(rows_v, rows_hv, &mut output[r * w..], w);
            }
        }
        (3, 2) => {
            // avg(half_hv, half_v_right)
            for r in 0..h {
                let rows_v = vrows(1, r as isize, w);
                let rows_hv = vrows(-2, r as isize, w + 5);
                crate::simd::row_avg_v_hv(rows_v, rows_hv, &mut output[r * w..], w);
            }
        }
        (1, 1) => {
            // avg(half_h, half_v)
            for r in 0..h {
                let src_h = row(r as isize, -2, w + 5);
                let rows_v = vrows(0, r as isize, w);
                crate::simd::row_avg_h_v(src_h, rows_v, &mut output[r * w..], w);
            }
        }
        (3, 1) => {
            // avg(half_h, half_v_right)
            for r in 0..h {
                let src_h = row(r as isize, -2, w + 5);
                let rows_v = vrows(1, r as isize, w);
                crate::simd::row_avg_h_v(src_h, rows_v, &mut output[r * w..], w);
            }
        }
        (1, 3) => {
            // avg(half_v, half_h_below)
            for r in 0..h {
                let src_h = row(r as isize + 1, -2, w + 5);
                let rows_v = vrows(0, r as isize, w);
                crate::simd::row_avg_h_v(src_h, rows_v, &mut output[r * w..], w);
            }
        }
        (3, 3) => {
            // avg(half_v_right, half_h_below)
            for r in 0..h {
                let src_h = row(r as isize + 1, -2, w + 5);
                let rows_v = vrows(1, r as isize, w);
                crate::simd::row_avg_h_v(src_h, rows_v, &mut output[r * w..], w);
            }
        }
        _ => unreachable!(),
    }
}

/// Perform chroma motion compensation for one plane (U or V).
///
/// Chroma MVs use the same quarter-pel values as luma, but since chroma is
/// half spatial resolution (4:2:0), these become eighth-pel for chroma.
/// Uses bilinear interpolation (spec 8.4.2.2.2).
///
/// `x`, `y`: chroma block top-left in full chroma-pel coordinates.
/// `dx`, `dy`: motion vector in eighth-pel units (= luma quarter-pel MV).
#[allow(clippy::too_many_arguments)]
pub(crate) fn chroma_mc(
    ref_plane: ChromaRef<'_>,
    ref_width: usize,
    ref_height: usize,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
) {
    let ChromaRef { pic, plane, offset } = ref_plane;
    // Wait until every row this call can read is final: rows y_int ..=
    // y_int + block_h (bilinear reads one row below), clamped to the
    // plane, plus margin so the published-length checks in
    // `chroma_mc_slice` make the same decisions as on a complete plane.
    if ref_width > 0 && block_w > 0 && block_h > 0 {
        let pic_width = (pic.width / 2) as usize;
        let avail_rows = (pic.chroma_len().saturating_sub(offset)
            + ref_width.saturating_sub(pic_width))
            / ref_width;
        let eff_h = ref_height.min(avail_rows).max(1) as i32;
        let y_int = y + (dy >> 3);
        let last_row = (y_int + block_h as i32 + 2).clamp(0, eff_h - 1) as usize;
        pic.wait_chroma(offset + (last_row + 1) * ref_width);
    }
    let data = pic.chroma(plane);
    let data = data.get(offset..).unwrap_or(&[]);
    // `ref_width` is the row stride, doubled for an MBAFF field MB; samples
    // are clamped at the picture's real chroma width.
    let pic_width = (pic.width / 2) as usize;
    chroma_mc_strided(
        data, ref_width, pic_width, ref_height, x, y, dx, dy, block_w, block_h, output,
    );
}

/// [`chroma_mc`] on a plain slice (complete data) whose row stride equals
/// its width.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn chroma_mc_slice(
    ref_plane: &[u8],
    ref_width: usize,
    ref_height: usize,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
) {
    chroma_mc_strided(
        ref_plane, ref_width, ref_width, ref_height, x, y, dx, dy, block_w, block_h, output,
    );
}

/// Chroma MC on a plane with row stride `ref_width` whose samples are
/// clamped at `pic_width` columns (they differ for an MBAFF field MB, which
/// reads every other line of the frame).
#[allow(clippy::too_many_arguments)]
fn chroma_mc_strided(
    ref_plane: &[u8],
    ref_width: usize,
    pic_width: usize,
    ref_height: usize,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
) {
    // Guard against malformed block sizes that would overrun the output buffer
    if block_w == 0 || block_h == 0 || block_w * block_h > output.len() {
        return;
    }
    // For field-coded MBAFF, the effective height is derived from the available
    // plane data, which may be smaller than the frame chroma height. A row
    // counts once its `pic_width` samples are there: the bottom field's last
    // row ends `ref_width - pic_width` short of a full stride.
    let ref_height =
        match (ref_plane.len() + ref_width.saturating_sub(pic_width)).checked_div(ref_width) {
            Some(rows) => ref_height.min(rows),
            None => ref_height,
        };
    let frac_x = dx.rem_euclid(8);
    let frac_y = dy.rem_euclid(8);
    let x_int = x + (dx >> 3);
    let y_int = y + (dy >> 3);

    // Full-pel fast path: direct copy when no interpolation needed
    if frac_x == 0 && frac_y == 0 {
        let w = pic_width as i32;
        let h = ref_height as i32;
        if x_int >= 0
            && y_int >= 0
            && x_int + block_w as i32 <= w
            && y_int + block_h as i32 <= h
            && (y_int as usize + block_h) * ref_width <= ref_plane.len()
        {
            let mut src_off = y_int as usize * ref_width + x_int as usize;
            for row in 0..block_h {
                if src_off + block_w > ref_plane.len() {
                    return;
                }
                copy_row(
                    &mut output[row * block_w..(row + 1) * block_w],
                    &ref_plane[src_off..src_off + block_w],
                );
                src_off += ref_width;
            }
        } else {
            for row in 0..block_h {
                for col in 0..block_w {
                    output[row * block_w + col] = ref_chroma(
                        ref_plane,
                        ref_width,
                        pic_width,
                        ref_height,
                        x_int + col as i32,
                        y_int + row as i32,
                    ) as u8;
                }
            }
        }
        return;
    }

    // In-bounds fast path: needs x_int..x_int+w+1 and y_int..y_int+h+1
    let w_i32 = pic_width as i32;
    let h_i32 = ref_height as i32;
    let in_bounds = x_int >= 0
        && y_int >= 0
        && (x_int + block_w as i32) < w_i32
        && (y_int + block_h as i32) < h_i32;

    if in_bounds {
        let c00 = ((8 - frac_x) * (8 - frac_y)) as u8;
        let c01 = (frac_x * (8 - frac_y)) as u8;
        let c10 = ((8 - frac_x) * frac_y) as u8;
        let c11 = (frac_x * frac_y) as u8;

        let x_u = x_int as usize;
        let y_u = y_int as usize;
        let top_off = y_u * ref_width + x_u;

        // Guard against ref_plane size mismatch (e.g. SPS change in malformed stream)
        let last_idx = top_off + block_h * ref_width + block_w;
        if last_idx >= ref_plane.len() {
            return; // ref plane too small, skip MC
        }

        crate::simd::chroma_bilinear_block(
            ref_plane, top_off, ref_width, block_w, block_h, output, c00, c01, c10, c11,
        );
        return;
    }

    // Boundary fallback: per-pixel with clamping
    for row in 0..block_h {
        for col in 0..block_w {
            let xf = x_int + col as i32;
            let yf = y_int + row as i32;
            let a = ref_chroma(ref_plane, ref_width, pic_width, ref_height, xf, yf);
            let b = ref_chroma(ref_plane, ref_width, pic_width, ref_height, xf + 1, yf);
            let c = ref_chroma(ref_plane, ref_width, pic_width, ref_height, xf, yf + 1);
            let d = ref_chroma(ref_plane, ref_width, pic_width, ref_height, xf + 1, yf + 1);

            let val = (8 - frac_x) * (8 - frac_y) * a
                + frac_x * (8 - frac_y) * b
                + (8 - frac_x) * frac_y * c
                + frac_x * frac_y * d;
            output[row * block_w + col] = ((val + 32) >> 6) as u8;
        }
    }
}

/// Bi-prediction averaging (spec 8.4.2.3.2).
/// `output[i] = (pred_l0[i] + pred_l1[i] + 1) >> 1` for each pixel.
pub fn bi_pred_avg(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
    crate::simd::bi_pred_avg(pred_l0, pred_l1, output);
}

/// Apply explicit weighted prediction to uni-directional MC output (spec 8.4.2.3.1).
/// `output[i] = clip((pred[i] * weight + (1 << (log2_denom - 1))) >> log2_denom + offset)`
/// When log2_denom == 0, the rounding term is 0.
pub fn weighted_uni(pred: &mut [u8], log2_denom: u32, weight: i32, offset: i32) {
    crate::simd::weighted_uni(pred, log2_denom, weight, offset);
}

/// Apply explicit weighted bi-prediction (spec 8.4.2.3.2).
/// Formula: `clip((p0*w0 + p1*w1 + round) >> (denom+1) + (o0+o1+1)>>1)`
#[allow(clippy::too_many_arguments)]
pub fn weighted_bi(
    pred_l0: &[u8],
    pred_l1: &[u8],
    output: &mut [u8],
    log2_denom: u32,
    w0: i32,
    o0: i32,
    w1: i32,
    o1: i32,
) {
    crate::simd::weighted_bi(pred_l0, pred_l1, output, log2_denom, w0, o0, w1, o1);
}

/// Apply implicit weighted bi-prediction for B-slices (spec 8.4.2.3.2).
/// Uses POC-distance-derived weights with fixed log2_denom=5.
pub fn weighted_bi_implicit(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8], w0: i32, w1: i32) {
    crate::simd::weighted_bi_implicit(pred_l0, pred_l1, output, w0, w1);
}

/// Luma motion compensation for any sample type (spec 8.4.2.2.1): the
/// scalar path for high bit depths (8-bit uses [`luma_mc_stride`]). The
/// (w + 5) x (h + 5) reference window is read once, with coordinates clamped
/// to the picture, and the half-sample planes are filtered from it.
/// Arguments as for [`luma_mc_stride`], in samples.
#[allow(clippy::too_many_arguments)]
pub(crate) fn luma_mc_px<P: Pixel>(
    ref_pic: &DecodedPicture,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [P],
    ref_stride: usize,
    ref_y_offset: usize,
) {
    // Window stride: up to 16 + 5 samples
    const WN: usize = 21;
    if block_w == 0 || block_h == 0 || block_w > 16 || block_h > 16 {
        return;
    }
    if block_w * block_h > output.len() {
        return;
    }
    let (fx, fy) = (dx & 3, dy & 3);
    let (xi, yi) = (x + (dx >> 2), y + (dy >> 2));
    let pic_w = ref_pic.width as i32;
    let pic_h = if ref_stride == ref_pic.width as usize {
        ref_pic.height as i32
    } else {
        (ref_pic.height / 2) as i32
    };
    let last_row = (yi + block_h as i32 + 3).clamp(0, (pic_h - 1).max(0)) as usize;
    ref_pic.wait_luma((ref_y_offset + (last_row + 1) * ref_stride) * P::BYTES);
    let plane = ref_pic.luma();
    let (ww, wh) = (block_w + 5, block_h + 5);
    // win[r * WN + c] = sample (xi - 2 + c, yi - 2 + r)
    let mut win = [0i32; WN * WN];
    let (x0, y0) = (xi - 2, yi - 2);
    let inside = x0 >= 0 && y0 >= 0 && x0 + ww as i32 <= pic_w && y0 + wh as i32 <= pic_h;
    let end = ref_y_offset
        + (y0 + wh as i32 - 1).max(0) as usize * ref_stride
        + (x0 + ww as i32).max(0) as usize;
    if inside && end * P::BYTES <= plane.len() {
        let mut row = [P::default(); WN];
        for r in 0..wh {
            let o = ref_y_offset + (y0 as usize + r) * ref_stride + x0 as usize;
            P::load_row(plane, o, &mut row[..ww]);
            for c in 0..ww {
                win[r * WN + c] = row[c].to_i32();
            }
        }
    } else {
        for r in 0..wh {
            let yy = (y0 + r as i32).clamp(0, pic_h - 1) as usize;
            for c in 0..ww {
                let xx = (x0 + c as i32).clamp(0, pic_w - 1) as usize;
                let i = ref_y_offset + yy * ref_stride + xx;
                win[r * WN + c] = if (i + 1) * P::BYTES <= plane.len() {
                    P::load(plane, i).to_i32()
                } else {
                    0
                };
            }
        }
    }
    let tap =
        |a: i32, b: i32, c: i32, d: i32, e: i32, f: i32| a - 5 * b + 20 * c + 20 * d - 5 * e + f;
    let clip = |v: i32| v.clamp(0, P::MAX);
    let avg = |a: i32, b: i32| (a + b + 1) >> 1;
    // Full sample at block position (c, r), c and r may be one past the block
    let g = |c: usize, r: usize| win[(r + 2) * WN + c + 2];
    // Unrounded horizontal half-sample sums, for every window row
    let mut h1 = [0i32; WN * 16];
    if fx != 0 {
        for r in 0..wh {
            let w = &win[r * WN..];
            for c in 0..block_w {
                h1[r * 16 + c] = tap(w[c], w[c + 1], w[c + 2], w[c + 3], w[c + 4], w[c + 5]);
            }
        }
    }
    // Rounded vertical half samples h, one column past the block
    let mut hv = [0i32; 16 * 17];
    if fy != 0 && fx != 2 {
        for r in 0..block_h {
            for c in 0..=block_w {
                let k = |rr: usize| win[(r + rr) * WN + c + 2];
                hv[r * 17 + c] = clip((tap(k(0), k(1), k(2), k(3), k(4), k(5)) + 16) >> 5);
            }
        }
    }
    // b (rounded) at (c, r), r up to block_h
    let b = |c: usize, r: usize| clip((h1[(r + 2) * 16 + c] + 16) >> 5);
    let h = |c: usize, r: usize| hv[r * 17 + c];
    let j = |c: usize, r: usize| {
        let k = |rr: usize| h1[(r + rr) * 16 + c];
        clip((tap(k(0), k(1), k(2), k(3), k(4), k(5)) + 512) >> 10)
    };
    for r in 0..block_h {
        let out = &mut output[r * block_w..(r + 1) * block_w];
        for (c, o) in out.iter_mut().enumerate() {
            // Table 8-12: G, a b c / d e f g / h i j k / n p q r
            let v = match (fx, fy) {
                (0, 0) => g(c, r),
                (1, 0) => avg(g(c, r), b(c, r)),
                (2, 0) => b(c, r),
                (3, 0) => avg(g(c + 1, r), b(c, r)),
                (0, 1) => avg(g(c, r), h(c, r)),
                (0, 2) => h(c, r),
                (0, 3) => avg(g(c, r + 1), h(c, r)),
                (1, 1) => avg(b(c, r), h(c, r)),
                (3, 1) => avg(b(c, r), h(c + 1, r)),
                (1, 3) => avg(h(c, r), b(c, r + 1)),
                (3, 3) => avg(h(c + 1, r), b(c, r + 1)),
                (2, 1) => avg(b(c, r), j(c, r)),
                (2, 3) => avg(j(c, r), b(c, r + 1)),
                (1, 2) => avg(h(c, r), j(c, r)),
                (3, 2) => avg(j(c, r), h(c + 1, r)),
                _ => j(c, r),
            };
            *o = P::from_i32(v);
        }
    }
}

/// Chroma motion compensation for any sample type (spec 8.4.2.2.2), sample
/// by sample with clamped coordinates: the scalar path for high bit
/// depths. Arguments as for [`chroma_mc`], in samples.
#[allow(clippy::too_many_arguments)]
pub(crate) fn chroma_mc_px<P: Pixel>(
    ref_plane: ChromaRef<'_>,
    ref_width: usize,
    ref_height: usize,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
    block_w: usize,
    block_h: usize,
    output: &mut [P],
) {
    let ChromaRef { pic, plane, offset } = ref_plane;
    if ref_width == 0 || block_w == 0 || block_h == 0 || block_w * block_h > output.len() {
        return;
    }
    let pic_width = (pic.width / 2) as usize;
    // Rows present from `offset` (an MBAFF bottom field's last row ends
    // short of a full stride), as chroma_mc
    let len = pic.chroma_len() / P::BYTES;
    let avail_rows = (len.saturating_sub(offset) + ref_width.saturating_sub(pic_width)) / ref_width;
    let eff_h = ref_height.min(avail_rows).max(1) as i32;
    let (fx, fy) = (dx & 7, dy & 7);
    let (xi, yi) = (x + (dx >> 3), y + (dy >> 3));
    let last_row = (yi + block_h as i32 + 2).clamp(0, eff_h - 1) as usize;
    pic.wait_chroma((offset + (last_row + 1) * ref_width) * P::BYTES);
    let data = pic.chroma(plane);
    let g = |xx: i32, yy: i32| -> i32 {
        let xx = xx.clamp(0, pic_width as i32 - 1) as usize;
        let yy = yy.clamp(0, eff_h - 1) as usize;
        let i = offset + yy * ref_width + xx;
        if (i + 1) * P::BYTES <= data.len() {
            P::load(data, i).to_i32()
        } else {
            0
        }
    };
    let (w00, w01, w10, w11) = ((8 - fx) * (8 - fy), fx * (8 - fy), (8 - fx) * fy, fx * fy);
    for r in 0..block_h {
        for c in 0..block_w {
            let (x, y) = (xi + c as i32, yi + r as i32);
            let v = w00 * g(x, y) + w01 * g(x + 1, y) + w10 * g(x, y + 1) + w11 * g(x + 1, y + 1);
            output[r * block_w + c] = P::from_i32((v + 32) >> 6);
        }
    }
}

/// Weighted prediction for any sample type (spec 8.4.2.3): `o` offsets are
/// as coded and scaled by 1 << (BitDepth - 8) here.
pub(crate) fn weighted_uni_px<P: Pixel>(pred: &mut [P], log2_denom: u32, weight: i32, offset: i32) {
    let o = offset << (P::BIT_DEPTH - 8);
    for p in pred.iter_mut() {
        let v = p.to_i32() * weight;
        let v = if log2_denom >= 1 {
            ((v + (1 << (log2_denom - 1))) >> log2_denom) + o
        } else {
            v + o
        };
        *p = P::clip(v);
    }
}

/// Explicit weighted bi-prediction for any sample type (spec 8.4.2.3).
#[allow(clippy::too_many_arguments)]
pub(crate) fn weighted_bi_px<P: Pixel>(
    a: &[P],
    b: &[P],
    out: &mut [P],
    log2_denom: u32,
    w0: i32,
    o0: i32,
    w1: i32,
    o1: i32,
) {
    let shift = P::BIT_DEPTH - 8;
    let o = ((o0 << shift) + (o1 << shift) + 1) >> 1;
    for (r, (&p, &q)) in out.iter_mut().zip(a.iter().zip(b)) {
        let v = (p.to_i32() * w0 + q.to_i32() * w1 + (1 << log2_denom)) >> (log2_denom + 1);
        *r = P::clip(v + o);
    }
}

/// Implicit weighted bi-prediction for any sample type (logWD 5, no
/// offsets).
pub(crate) fn weighted_bi_implicit_px<P: Pixel>(a: &[P], b: &[P], out: &mut [P], w0: i32, w1: i32) {
    for (r, (&p, &q)) in out.iter_mut().zip(a.iter().zip(b)) {
        *r = P::clip((p.to_i32() * w0 + q.to_i32() * w1 + 32) >> 6);
    }
}

/// Bi-prediction average for any sample type.
pub(crate) fn bi_pred_avg_px<P: Pixel>(a: &[P], b: &[P], out: &mut [P]) {
    for (r, (&p, &q)) in out.iter_mut().zip(a.iter().zip(b)) {
        *r = P::from_i32((p.to_i32() + q.to_i32() + 1) >> 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use alloc::vec::Vec;

    fn make_ref_pic(width: u32, height: u32, y_data: Vec<u8>) -> Arc<DecodedPicture> {
        let uv_size = (width / 2 * height / 2) as usize;
        Arc::new(DecodedPicture::new_complete(
            crate::dpb::PictureMeta {
                width,
                height,
                frame_num: 0,
                pic_order_cnt: 0,
                field_poc: [0, 0],
                mb_width: width / 16,
                is_intra: false,
                structure: crate::dpb::PictureStructure::Frame,
                chroma_422: false,
                sample_bytes: 1,
            },
            y_data,
            vec![128; uv_size],
            vec![128; uv_size],
            crate::dpb::Motion::default(),
        ))
    }

    #[test]
    fn test_integer_pel_copy() {
        // 8x8 reference with gradient
        let mut y = vec![0u8; 64];
        for r in 0..8 {
            for c in 0..8 {
                y[r * 8 + c] = (r * 16 + c * 4) as u8;
            }
        }
        let pic = make_ref_pic(8, 8, y.clone());

        // Integer-pel MC (dx=0, dy=0) should be a direct copy
        let mut out = vec![0u8; 16]; // 4x4 block at (2,1)
        luma_mc_stride(&pic, 1, 2, 0, 0, 4, 4, &mut out, pic.width as usize, 0);
        for r in 0..4 {
            for c in 0..4 {
                assert_eq!(out[r * 4 + c], y[(r + 2) * 8 + (c + 1)]);
            }
        }
    }

    #[test]
    fn test_integer_pel_with_mv() {
        // 8x8 uniform reference
        let y = vec![42u8; 64];
        let pic = make_ref_pic(8, 8, y);

        // MV = (4, 8) in quarter-pel = (1, 2) full-pel offset
        let mut out = vec![0u8; 16];
        luma_mc_stride(&pic, 0, 0, 4, 8, 4, 4, &mut out, pic.width as usize, 0);
        // With uniform reference, all outputs should be 42
        assert!(out.iter().all(|&v| v == 42));
    }

    #[test]
    fn test_half_pel_horizontal() {
        // 16x1 reference: known values for 6-tap filter
        let mut y = vec![128u8; 16];
        // Set a step edge: left half = 0, right half = 255
        y[..8].fill(0);
        y[8..].fill(255);
        let pic = make_ref_pic(16, 1, y);

        // Half-pel horizontal at x=7 (the edge): MV dx=2 (half-pel), dy=0
        let mut out = [0u8; 1];
        luma_mc_stride(&pic, 7, 0, 2, 0, 1, 1, &mut out, pic.width as usize, 0);
        // 6-tap at x=7: samples at x=5..10 = [0, 0, 0, 255, 255, 255]
        // (0 - 0 + 0 + 20*255 - 5*255 + 255 + 16) >> 5 = (0 + 5100 - 1275 + 255 + 16) >> 5
        // = 4096 >> 5 = 128
        assert_eq!(out[0], 128);
    }

    #[test]
    fn test_half_pel_vertical() {
        // 1x16 reference with step edge at row 8
        let mut y = vec![0u8; 16];
        y[8..].fill(255);
        let pic = make_ref_pic(1, 16, y);

        let mut out = [0u8; 1];
        luma_mc_stride(&pic, 0, 7, 0, 2, 1, 1, &mut out, pic.width as usize, 0);
        // Same as horizontal but vertical: should also give 128
        assert_eq!(out[0], 128);
    }

    #[test]
    fn test_uniform_ref_all_frac_positions() {
        // Uniform reference: all fractional positions should give the same value
        let y = vec![100u8; 256];
        let pic = make_ref_pic(16, 16, y);

        for frac_x in 0..4 {
            for frac_y in 0..4 {
                let mut out = [0u8; 1];
                luma_mc_stride(
                    &pic,
                    4,
                    4,
                    frac_x,
                    frac_y,
                    1,
                    1,
                    &mut out,
                    pic.width as usize,
                    0,
                );
                assert_eq!(
                    out[0], 100,
                    "frac ({},{}) should give 100 for uniform ref",
                    frac_x, frac_y
                );
            }
        }
    }

    #[test]
    fn test_chroma_mc_integer() {
        let plane = vec![200u8; 64]; // 8x8 chroma
        let mut out = vec![0u8; 16]; // 4x4 block
        chroma_mc_slice(&plane, 8, 8, 0, 0, 0, 0, 4, 4, &mut out);
        assert!(out.iter().all(|&v| v == 200));
    }

    #[test]
    fn test_chroma_mc_half_pel() {
        // 4x1 chroma: [0, 255, 0, 255]
        let plane = vec![0, 255, 0, 255];
        let mut out = [0u8; 1];
        // dx=4 = half-pel (4/8 = 0.5), dy=0
        chroma_mc_slice(&plane, 4, 1, 0, 0, 4, 0, 1, 1, &mut out);
        // Bilinear: (8-4)*8*0 + 4*8*255 + 0 + 0 = 8160. (8160+32)>>6 = 128 (approx)
        assert_eq!(out[0], 128);
    }

    #[test]
    fn test_boundary_clipping() {
        // 4x4 reference, MV pointing outside
        let y: Vec<u8> = (0..16).collect();
        let pic = make_ref_pic(4, 4, y);

        // MC at (0,0) with MV=(-4, -4) in quarter-pel = (-1, -1) full-pel
        // Should clamp to (0,0) and read the top-left corner value
        let mut out = [0u8; 1];
        luma_mc_stride(&pic, 0, 0, -4, -4, 1, 1, &mut out, pic.width as usize, 0);
        assert_eq!(out[0], 0); // clamped to (0,0)
    }

    #[test]
    fn edge_tiles_match_scalar_for_frames_and_fields() {
        let width = 32;
        let height = 32;
        let mut seed = 7u32;
        let samples: Vec<_> = (0..width * height)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed >> 24) as u8
            })
            .collect();
        let pic = make_ref_pic(width as u32, height as u32, samples.clone());
        for (first_row, step) in [(0, 1), (0, 2), (1, 2)] {
            let packed: Vec<_> = samples
                .chunks_exact(width)
                .skip(first_row)
                .step_by(step)
                .flatten()
                .copied()
                .collect();
            let oracle = LumaView {
                y: &packed,
                stride: width,
                width: width as i32,
                height: (height / step) as i32,
            };
            for (w, h) in [(4, 4), (8, 4), (16, 8), (16, 16)] {
                for x in [-20, -1, 0, 4, 31, 44] {
                    for y in [-20, -1, 0, 4, oracle.height - 1, oracle.height + 12] {
                        for fy in 0..4 {
                            for fx in 0..4 {
                                let mut actual = [0u8; 16 * 16];
                                luma_mc_stride(
                                    &pic,
                                    x,
                                    y,
                                    fx,
                                    fy,
                                    w,
                                    h,
                                    &mut actual,
                                    width * step,
                                    width * first_row,
                                );
                                for (i, &sample) in actual[..w * h].iter().enumerate() {
                                    let sx = x + (i % w) as i32;
                                    let sy = y + (i / w) as i32;
                                    assert_eq!(sample, luma_interp(&oracle, sx, sy, fx, fy));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_negative_mv_fractional() {
        // Verify negative MV fractional extraction
        let y = vec![128u8; 256];
        let pic = make_ref_pic(16, 16, y);

        // MV = (-1, -1) quarter-pel: frac should be (3, 3), int offset = (-1, -1)
        let mut out = [0u8; 1];
        luma_mc_stride(&pic, 8, 8, -1, -1, 1, 1, &mut out, pic.width as usize, 0);
        // Uniform ref, so result should be 128 regardless
        assert_eq!(out[0], 128);
    }
}
