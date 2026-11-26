use crate::pixel::Pixel;

/// I16x16 intra prediction (H.264 spec 8.3.3).
/// `mode`: 0=vertical, 1=horizontal, 2=DC, 3=plane.
/// For pixels at the top-left of the frame with no neighbors, only DC mode
/// with value 128 is valid.
pub(crate) fn predict_intra_16x16_i32(
    (mid, max): (i32, i32),
    mode: u8,
    above: Option<&[i32]>, // 16 pixels from row above
    left: Option<&[i32]>,  // 16 pixels from column to left
    above_left: Option<i32>,
    output: &mut [i32; 256],
) {
    let mids = [mid; 16];
    match mode {
        0 => {
            // Vertical: copy above row to all rows
            let above = above.unwrap_or(&mids[..16]);
            for row in 0..16 {
                output[row * 16..row * 16 + 16].copy_from_slice(&above[..16]);
            }
        }
        1 => {
            // Horizontal: copy left column to all columns
            let left = left.unwrap_or(&mids[..16]);
            for row in 0..16 {
                for col in 0..16 {
                    output[row * 16 + col] = left[row];
                }
            }
        }
        2 => {
            // DC: average of available above and left samples
            let dc = match (above, left) {
                (Some(a), Some(l)) => {
                    let sum: u32 = a[..16].iter().map(|&x| x as u32).sum::<u32>()
                        + l[..16].iter().map(|&x| x as u32).sum::<u32>();
                    ((sum + 16) >> 5) as i32
                }
                (Some(a), None) => {
                    let sum: u32 = a[..16].iter().map(|&x| x as u32).sum();
                    ((sum + 8) >> 4) as i32
                }
                (None, Some(l)) => {
                    let sum: u32 = l[..16].iter().map(|&x| x as u32).sum();
                    ((sum + 8) >> 4) as i32
                }
                (None, None) => mid,
            };
            output.fill(dc);
        }
        3 => {
            // Plane prediction
            let above = above.unwrap_or(&mids[..16]);
            let left = left.unwrap_or(&mids[..16]);
            let _p = above_left.unwrap_or(mid);

            let mut h: i32 = 0;
            let mut v: i32 = 0;
            for i in 0..8 {
                // p[6-i, -1] and p[-1, 6-i]: when i==7, index -1 = above_left pixel
                let above_neg = if i < 7 { above[6 - i] } else { _p };
                let left_neg = if i < 7 { left[6 - i] } else { _p };
                h += (i as i32 + 1) * (above[8 + i] - above_neg);
                v += (i as i32 + 1) * (left[8 + i] - left_neg);
            }
            let a_val = 16 * (above[15] + left[15]);
            let b_val = (5 * h + 32) >> 6;
            let c_val = (5 * v + 32) >> 6;

            for y in 0..16 {
                for x in 0..16 {
                    let val = (a_val + b_val * (x as i32 - 7) + c_val * (y as i32 - 7) + 16) >> 5;
                    output[y * 16 + x] = val.clamp(0, max);
                }
            }
        }
        _ => {
            output.fill(mid);
        }
    }
}

/// I4x4 intra prediction (H.264 spec 8.3.1.2).
/// `mode`: 0-8 prediction modes.
/// `above`: 8 pixels above (4 directly above + 4 above-right), None if unavailable.
/// `left`: 4 pixels to the left, None if unavailable.
/// `above_left`: pixel diagonally above-left, None if unavailable.
/// `output`: 4x4 block in raster order (16 bytes).
pub(crate) fn predict_intra_4x4_i32(
    (mid, _max): (i32, i32),
    mode: u8,
    above: Option<&[i32]>,
    left: Option<&[i32]>,
    above_left: Option<i32>,
    output: &mut [i32; 16],
) {
    let mids = [mid; 16];
    match mode {
        0 => {
            // Vertical
            let a = above.unwrap_or(&mids[..8]);
            for row in 0..4 {
                output[row * 4..row * 4 + 4].copy_from_slice(&a[..4]);
            }
        }
        1 => {
            // Horizontal
            let l = left.unwrap_or(&mids[..4]);
            for row in 0..4 {
                for col in 0..4 {
                    output[row * 4 + col] = l[row];
                }
            }
        }
        2 => {
            // DC
            let dc = match (above, left) {
                (Some(a), Some(l)) => {
                    let sum: u32 = a[..4].iter().map(|&x| x as u32).sum::<u32>()
                        + l[..4].iter().map(|&x| x as u32).sum::<u32>();
                    ((sum + 4) >> 3) as i32
                }
                (Some(a), None) => {
                    let sum: u32 = a[..4].iter().map(|&x| x as u32).sum();
                    ((sum + 2) >> 2) as i32
                }
                (None, Some(l)) => {
                    let sum: u32 = l[..4].iter().map(|&x| x as u32).sum();
                    ((sum + 2) >> 2) as i32
                }
                (None, None) => mid,
            };
            output.fill(dc);
        }
        3 => {
            // Diagonal Down-Left
            let a = above.unwrap_or(&mids[..8]);
            for y in 0..4 {
                for x in 0..4 {
                    if x == 3 && y == 3 {
                        output[y * 4 + x] = ((a[6] as u16 + 3 * a[7] as u16 + 2) >> 2) as i32;
                    } else {
                        let i = x + y;
                        output[y * 4 + x] =
                            ((a[i] as u16 + 2 * a[i + 1] as u16 + a[i + 2] as u16 + 2) >> 2) as i32;
                    }
                }
            }
        }
        4 => {
            // Diagonal Down-Right (spec 8.3.1.2.5)
            // Build reference pixel array: [left[3], left[2], left[1], left[0],
            //                               above_left, above[0..3]]
            // pred[x,y] = (ref[3-y+x] + 2*ref[4-y+x] + ref[5-y+x] + 2) >> 2
            let a = above.unwrap_or(&mids[..8]);
            let l = left.unwrap_or(&mids[..4]);
            let p = above_left.unwrap_or(mid);
            let r = [
                l[3] as u16,
                l[2] as u16,
                l[1] as u16,
                l[0] as u16,
                p as u16,
                a[0] as u16,
                a[1] as u16,
                a[2] as u16,
                a[3] as u16,
            ];
            for y in 0..4usize {
                for x in 0..4usize {
                    let i = 3 + x - y;
                    output[y * 4 + x] = ((r[i] + 2 * r[i + 1] + r[i + 2] + 2) >> 2) as i32;
                }
            }
        }
        5 => {
            // Vertical-Right (spec 8.3.1.2.6)
            let a = above.unwrap_or(&mids[..8]);
            let l = left.unwrap_or(&mids[..4]);
            let p = above_left.unwrap_or(mid);
            let (lt, t0, t1, t2, t3) =
                (p as u16, a[0] as u16, a[1] as u16, a[2] as u16, a[3] as u16);
            let (l0, l1, l2) = (l[0] as u16, l[1] as u16, l[2] as u16);
            // Row 0: avg of above pairs
            output[0] = ((lt + t0 + 1) >> 1) as i32;
            output[1] = ((t0 + t1 + 1) >> 1) as i32;
            output[2] = ((t1 + t2 + 1) >> 1) as i32;
            output[3] = ((t2 + t3 + 1) >> 1) as i32;
            // Row 1: filtered above
            output[4] = ((l0 + 2 * lt + t0 + 2) >> 2) as i32;
            output[5] = ((lt + 2 * t0 + t1 + 2) >> 2) as i32;
            output[6] = ((t0 + 2 * t1 + t2 + 2) >> 2) as i32;
            output[7] = ((t1 + 2 * t2 + t3 + 2) >> 2) as i32;
            // Row 2: shifted from row 0
            output[8] = ((lt + 2 * l0 + l1 + 2) >> 2) as i32;
            output[9] = output[0]; // same as (lt + t0 + 1) >> 1
            output[10] = output[1];
            output[11] = output[2];
            // Row 3: shifted from row 1
            output[12] = ((l0 + 2 * l1 + l2 + 2) >> 2) as i32;
            output[13] = output[4]; // same as (l0 + 2*lt + t0 + 2) >> 2
            output[14] = output[5];
            output[15] = output[6];
        }
        6 => {
            // Horizontal-Down (spec 8.3.1.2.7)
            let a = above.unwrap_or(&mids[..8]);
            let l = left.unwrap_or(&mids[..4]);
            let p = above_left.unwrap_or(mid);
            let (lt, t0, t1, t2) = (p as u16, a[0] as u16, a[1] as u16, a[2] as u16);
            let (l0, l1, l2, l3) = (l[0] as u16, l[1] as u16, l[2] as u16, l[3] as u16);
            // Row 0
            output[0] = ((lt + l0 + 1) >> 1) as i32;
            output[1] = ((l0 + 2 * lt + t0 + 2) >> 2) as i32;
            output[2] = ((lt + 2 * t0 + t1 + 2) >> 2) as i32;
            output[3] = ((t0 + 2 * t1 + t2 + 2) >> 2) as i32;
            // Row 1
            output[4] = ((l0 + l1 + 1) >> 1) as i32;
            output[5] = ((lt + 2 * l0 + l1 + 2) >> 2) as i32;
            output[6] = output[0]; // same as (lt + l0 + 1) >> 1
            output[7] = output[1]; // same as (l0 + 2*lt + t0 + 2) >> 2
                                   // Row 2
            output[8] = ((l1 + l2 + 1) >> 1) as i32;
            output[9] = ((l0 + 2 * l1 + l2 + 2) >> 2) as i32;
            output[10] = output[4]; // same as (l0 + l1 + 1) >> 1
            output[11] = output[5]; // same as (lt + 2*l0 + l1 + 2) >> 2
                                    // Row 3
            output[12] = ((l2 + l3 + 1) >> 1) as i32;
            output[13] = ((l1 + 2 * l2 + l3 + 2) >> 2) as i32;
            output[14] = output[8]; // same as (l1 + l2 + 1) >> 1
            output[15] = output[9]; // same as (l0 + 2*l1 + l2 + 2) >> 2
        }
        7 => {
            // Vertical-Left (spec 8.3.1.2.8)
            let a = above.unwrap_or(&mids[..8]);
            for y in 0..4 {
                for x in 0..4 {
                    let i = x + (y >> 1);
                    output[y * 4 + x] = if y % 2 == 0 {
                        ((a[i] as u16 + a[i + 1] as u16 + 1) >> 1) as i32
                    } else {
                        ((a[i] as u16 + 2 * a[i + 1] as u16 + a[i + 2] as u16 + 2) >> 2) as i32
                    };
                }
            }
        }
        8 => {
            // Horizontal-Up (spec 8.3.1.2.9)
            let l = left.unwrap_or(&mids[..4]);
            for y in 0..4 {
                for x in 0..4 {
                    let zh = x + 2 * y;
                    output[y * 4 + x] = if zh < 5 {
                        let i = y + (x >> 1);
                        if zh % 2 == 0 {
                            ((l[i] as u16 + l[i + 1] as u16 + 1) >> 1) as i32
                        } else {
                            ((l[i] as u16 + 2 * l[i + 1] as u16 + l[i + 2] as u16 + 2) >> 2) as i32
                        }
                    } else if zh == 5 {
                        ((l[2] as u16 + 3 * l[3] as u16 + 2) >> 2) as i32
                    } else {
                        l[3]
                    };
                }
            }
        }
        _ => {
            output.fill(mid);
        }
    }
}

/// Intra chroma prediction (spec 8.3.4) of an 8-wide block of `h` rows: 8
/// for 4:2:0, 16 for 4:2:2. `above` holds the 8 samples above, `left` the
/// `h` samples to the left, `left_avail` whether the left samples of the
/// upper and lower half of the macroblock may be used (they can differ in
/// an MBAFF frame with constrained intra prediction). DC is predicted per
/// 4x4 block (spec 8.3.4.1-3); the other modes need whole columns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_chroma_i32(
    (mid, max): (i32, i32),
    mode: u8,
    above: Option<&[i32]>,
    left: Option<&[i32]>,
    left_avail: [bool; 2],
    above_left: Option<i32>,
    h: usize,
    output: &mut [i32],
) {
    let mids = [mid; 16];
    let output = &mut output[..8 * h];
    let left_all = left.filter(|_| left_avail[0] && left_avail[1]);
    match mode {
        0 => {
            let sum = |v: &[i32]| -> i32 { v.iter().sum() };
            for yo in (0..h).step_by(4) {
                let l = left
                    .filter(|_| left_avail[yo * 2 / h])
                    .map(|l| sum(&l[yo..yo + 4]));
                for xo in [0, 4] {
                    let a = above.map(|a| sum(&a[xo..xo + 4]));
                    let dc = if (xo == 0) == (yo == 0) {
                        // Both, else either
                        match (a, l) {
                            (Some(a), Some(l)) => (a + l + 4) >> 3,
                            (None, Some(v)) | (Some(v), None) => (v + 2) >> 2,
                            (None, None) => mid,
                        }
                    } else if xo > 0 {
                        // Above first
                        a.or(l).map_or(mid, |v| (v + 2) >> 2)
                    } else {
                        // Left first
                        l.or(a).map_or(mid, |v| (v + 2) >> 2)
                    };
                    for r in yo..yo + 4 {
                        output[r * 8 + xo..r * 8 + xo + 4].fill(dc);
                    }
                }
            }
        }
        1 => {
            // Horizontal
            for (r, row) in output.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                row.fill(left_all.map_or(mid, |l| l[r]));
            }
        }
        2 => {
            // Vertical
            for row in output.as_chunks_mut::<8>().0 {
                match above {
                    Some(a) => row.copy_from_slice(&a[..8]),
                    None => row.fill(mid),
                }
            }
        }
        3 => {
            // Plane; p[-1, -1] is the above-left sample
            let above = above.unwrap_or(&mids[..8]);
            let left = left_all.unwrap_or(&mids[..16]);
            let corner = above_left.unwrap_or(mid);
            let at = |x: i32| {
                if x < 0 {
                    corner
                } else {
                    above[x as usize]
                }
            };
            let lt = |y: i32| {
                if y < 0 {
                    corner
                } else {
                    left[y as usize]
                }
            };
            // yCF: 4 for 4:2:2 (16 rows), 0 for 4:2:0
            let ycf = (h as i32 - 8) / 2;
            let mut hh = 0;
            for x in 0..4 {
                hh += (x + 1) * (at(4 + x) - at(2 - x));
            }
            let mut vv = 0;
            for y in 0..4 + ycf {
                vv += (y + 1) * (lt(4 + ycf + y) - lt(2 + ycf - y));
            }
            let a = 16 * (lt(h as i32 - 1) + at(7));
            let b = (34 * hh + 32) >> 6;
            let c = ((if h == 16 { 5 } else { 34 }) * vv + 32) >> 6;
            for (y, row) in output.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                for (x, px) in row.iter_mut().enumerate() {
                    let v = (a + b * (x as i32 - 3) + c * (y as i32 - 3 - ycf) + 16) >> 5;
                    *px = v.clamp(0, max);
                }
            }
        }
        _ => output.fill(mid),
    }
}

/// I8x8 intra prediction (H.264 spec 8.3.2.2).
/// Same 9 modes as I4x4 but at 8×8 granularity with filtered reference samples.
///
/// `above`: 16 pixels above (8 directly above + 8 above-right), None if unavailable.
/// `left`: 8 pixels to the left, None if unavailable.
/// `above_left`: pixel diagonally above-left, None if unavailable.
/// `has_topright`: whether above-right 8 pixels are available.
/// `output`: 8×8 block in raster order (64 bytes).
pub(crate) fn predict_intra_8x8_i32(
    (mid, _max): (i32, i32),
    mode: u8,
    above: Option<&[i32]>,
    left: Option<&[i32]>,
    above_left: Option<i32>,
    has_topright: bool,
    output: &mut [i32; 64],
) {
    // Build filtered reference samples (low-pass: (a + 2b + c + 2) >> 2)
    let def = mid;
    let al = above_left.unwrap_or(def);

    // Filtered left samples (l0..l7)
    let mut fl = [mid; 8];
    if let Some(l) = left {
        fl[0] = (if above_left.is_some() { al } else { l[0] } + 2 * l[0] + l[1] + 2) >> 2;
        for i in 1..7 {
            fl[i] = (l[i - 1] + 2 * l[i] + l[i + 1] + 2) >> 2;
        }
        fl[7] = (l[6] + 3 * l[7] + 2) >> 2;
    }

    // Filtered top samples (t0..t7)
    let mut ft = [mid; 8];
    if let Some(a) = above {
        ft[0] = (if above_left.is_some() { al } else { a[0] } + 2 * a[0] + a[1] + 2) >> 2;
        for i in 1..7 {
            ft[i] = (a[i - 1] + 2 * a[i] + a[i + 1] + 2) >> 2;
        }
        ft[7] = if has_topright {
            (a[6] + 2 * a[7] + a[8] + 2) >> 2
        } else {
            (a[6] + 3 * a[7] + 2) >> 2
        };
    }

    // Filtered top-right samples (t8..t15)
    let mut ftr = [0i32; 8];
    if let Some(a) = above {
        if has_topright {
            for i in 0..7 {
                ftr[i] = (a[7 + i] + 2 * a[8 + i] + a[9 + i] + 2) >> 2;
            }
            ftr[7] = (a[14] + 3 * a[15] + 2) >> 2;
        } else {
            ftr.fill(a[7]);
        }
    }

    // Filtered top-left
    let flt = match (above_left, left, above) {
        (Some(_), Some(l), Some(a)) => (l[0] + 2 * al + a[0] + 2) >> 2,
        (Some(_), _, _) => al,
        _ => mid,
    };

    match mode {
        0 => {
            // Vertical: replicate filtered top row
            for y in 0..8 {
                for x in 0..8 {
                    output[y * 8 + x] = ft[x];
                }
            }
        }
        1 => {
            // Horizontal: replicate filtered left column
            for y in 0..8 {
                for x in 0..8 {
                    output[y * 8 + x] = fl[y];
                }
            }
        }
        2 => {
            // DC
            let dc = match (above, left) {
                (Some(_), Some(_)) => {
                    let sum: i32 = ft.iter().sum::<i32>() + fl.iter().sum::<i32>();
                    (sum + 8) >> 4
                }
                (Some(_), None) => {
                    let sum: i32 = ft.iter().sum();
                    (sum + 4) >> 3
                }
                (None, Some(_)) => {
                    let sum: i32 = fl.iter().sum();
                    (sum + 4) >> 3
                }
                (None, None) => mid,
            };
            output.fill(dc);
        }
        3 => {
            // Diagonal Down-Left
            // Combine t0..t7 and t8..t15 (top-right)
            let mut t = [0i32; 16];
            t[..8].copy_from_slice(&ft);
            t[8..].copy_from_slice(&ftr);
            for y in 0..8 {
                for x in 0..8 {
                    let i = x + y;
                    output[y * 8 + x] = if x == 7 && y == 7 {
                        (t[14] + 3 * t[15] + 2) >> 2
                    } else {
                        (t[i] + 2 * t[i + 1] + t[i + 2] + 2) >> 2
                    };
                }
            }
        }
        4 => {
            // Diagonal Down-Right (spec 8.3.2.2.6)
            let mut t = [0i32; 16];
            t[..8].copy_from_slice(&ft);
            t[8..].copy_from_slice(&ftr);
            for y in 0..8 {
                for x in 0..8 {
                    output[y * 8 + x] = if x > y {
                        let i = x - y - 1;
                        let p0 = if i == 0 { flt } else { t[i - 1] };
                        (p0 + 2 * t[i] + t[i + 1] + 2) >> 2
                    } else if x < y {
                        let i = y - x - 1;
                        let p0 = if i == 0 { flt } else { fl[i - 1] };
                        (p0 + 2 * fl[i] + fl.get(i + 1).copied().unwrap_or(fl[7]) + 2) >> 2
                    } else {
                        (fl[0] + 2 * flt + ft[0] + 2) >> 2
                    };
                }
            }
        }
        5 => {
            // Vertical-Right
            for y in 0..8 {
                for x in 0..8 {
                    let zv = 2 * x as i32 - y as i32;
                    output[y * 8 + x] = if zv >= 0 {
                        let i = x - (y >> 1);
                        if zv & 1 == 0 {
                            (ft.get(i.wrapping_sub(1)).copied().unwrap_or(flt) + ft[i] + 1) >> 1
                        } else {
                            let p0 = if i >= 2 {
                                ft[i - 2]
                            } else if i == 1 {
                                flt
                            } else {
                                fl[0]
                            };
                            (p0 + 2 * ft.get(i.wrapping_sub(1)).copied().unwrap_or(flt) + ft[i] + 2)
                                >> 2
                        }
                    } else if zv == -1 {
                        (ft[0] + 2 * flt + fl[0] + 2) >> 2
                    } else {
                        // zVR < -1: 3-tap filter toward top-left (spec 8.3.2.2.7)
                        let i = y - 2 * x - 1;
                        let p0 = fl[i];
                        let p1 = if i >= 1 { fl[i - 1] } else { flt };
                        let p2 = if i >= 2 {
                            fl[i - 2]
                        } else if i == 1 {
                            flt
                        } else {
                            ft[0]
                        };
                        (p0 + 2 * p1 + p2 + 2) >> 2
                    };
                }
            }
        }
        6 => {
            // Horizontal-Down
            for y in 0..8 {
                for x in 0..8 {
                    let zh = 2 * y as i32 - x as i32;
                    output[y * 8 + x] = if zh >= 0 {
                        let i = y - (x >> 1);
                        if zh & 1 == 0 {
                            (fl.get(i.wrapping_sub(1)).copied().unwrap_or(flt) + fl[i] + 1) >> 1
                        } else {
                            let p0 = if i >= 2 {
                                fl[i - 2]
                            } else if i == 1 {
                                flt
                            } else {
                                ft[0]
                            };
                            (p0 + 2 * fl.get(i.wrapping_sub(1)).copied().unwrap_or(flt) + fl[i] + 2)
                                >> 2
                        }
                    } else if zh == -1 {
                        (fl[0] + 2 * flt + ft[0] + 2) >> 2
                    } else {
                        // zHD < -1: use filtered above samples in descending order
                        // ft[-1] is flt; for index -1, use flt
                        let i = x as i32 - 2 * y as i32;
                        let get_ft = |idx: i32| -> i32 {
                            if idx < 0 {
                                flt
                            } else {
                                ft[idx as usize]
                            }
                        };
                        (get_ft(i - 3) + 2 * get_ft(i - 2) + get_ft(i - 1) + 2) >> 2
                    };
                }
            }
        }
        7 => {
            // Vertical-Left
            let mut t = [0i32; 16];
            t[..8].copy_from_slice(&ft);
            t[8..].copy_from_slice(&ftr);
            for y in 0..8 {
                for x in 0..8 {
                    let i = x + (y >> 1);
                    output[y * 8 + x] = if y & 1 == 0 {
                        (t[i] + t[i + 1] + 1) >> 1
                    } else {
                        (t[i] + 2 * t[i + 1] + t[i + 2] + 2) >> 2
                    };
                }
            }
        }
        8 => {
            // Horizontal-Up
            for y in 0..8 {
                for x in 0..8 {
                    let i = y + (x >> 1);
                    output[y * 8 + x] = if i >= 7 {
                        fl[7]
                    } else if x & 1 == 0 {
                        (fl[i] + fl[i + 1] + 1) >> 1
                    } else {
                        (fl[i] + 2 * fl[i + 1] + fl.get(i + 2).copied().unwrap_or(fl[7]) + 2) >> 2
                    };
                }
            }
        }
        _ => output.fill(mid),
    }
}

/// Neighbouring samples as i32, up to `N` of them.
#[inline(always)]
fn widen<P: Pixel, const N: usize>(s: &[P]) -> [i32; N] {
    let mut out = [0i32; N];
    for (o, &v) in out.iter_mut().zip(s) {
        *o = v.to_i32();
    }
    out
}

/// Store an i32 prediction as samples.
#[inline(always)]
fn narrow<P: Pixel>(src: &[i32], out: &mut [P]) {
    for (o, &v) in out.iter_mut().zip(src) {
        *o = P::from_i32(v);
    }
}

/// I16x16 intra prediction (spec 8.3.3) for any sample type.
pub fn predict_intra_16x16<P: Pixel>(
    mode: u8,
    above: Option<&[P]>,
    left: Option<&[P]>,
    above_left: Option<P>,
    output: &mut [P; 256],
) {
    let (a, l) = (above.map(widen::<P, 16>), left.map(widen::<P, 16>));
    let mut out = [0i32; 256];
    predict_intra_16x16_i32(
        (P::MID, P::MAX),
        mode,
        a.as_ref().map(|v| &v[..]),
        l.as_ref().map(|v| &v[..]),
        above_left.map(P::to_i32),
        &mut out,
    );
    narrow(&out, output);
}

/// I4x4 intra prediction (spec 8.3.1.2) for any sample type: `above` holds
/// 8 samples (4 above, 4 above-right), `left` 4.
pub fn predict_intra_4x4<P: Pixel>(
    mode: u8,
    above: Option<&[P]>,
    left: Option<&[P]>,
    above_left: Option<P>,
    output: &mut [P; 16],
) {
    let (a, l) = (above.map(widen::<P, 8>), left.map(widen::<P, 4>));
    let mut out = [0i32; 16];
    predict_intra_4x4_i32(
        (P::MID, P::MAX),
        mode,
        a.as_ref().map(|v| &v[..]),
        l.as_ref().map(|v| &v[..]),
        above_left.map(P::to_i32),
        &mut out,
    );
    narrow(&out, output);
}

/// I8x8 intra prediction (spec 8.3.2.2) for any sample type: `above` holds
/// 16 samples (8 above, 8 above-right), `left` 8.
pub fn predict_intra_8x8<P: Pixel>(
    mode: u8,
    above: Option<&[P]>,
    left: Option<&[P]>,
    above_left: Option<P>,
    has_topright: bool,
    output: &mut [P; 64],
) {
    let (a, l) = (above.map(widen::<P, 16>), left.map(widen::<P, 8>));
    let mut out = [0i32; 64];
    predict_intra_8x8_i32(
        (P::MID, P::MAX),
        mode,
        a.as_ref().map(|v| &v[..]),
        l.as_ref().map(|v| &v[..]),
        above_left.map(P::to_i32),
        has_topright,
        &mut out,
    );
    narrow(&out, output);
}

/// Intra chroma prediction (spec 8.3.4) for any sample type; see
/// [`predict_chroma_i32`].
pub fn predict_chroma<P: Pixel>(
    mode: u8,
    above: Option<&[P]>,
    left: Option<&[P]>,
    left_avail: [bool; 2],
    above_left: Option<P>,
    h: usize,
    output: &mut [P],
) {
    let a = above.map(widen::<P, 8>);
    let l = left.map(widen::<P, 16>);
    let mut out = [0i32; 128];
    predict_chroma_i32(
        (P::MID, P::MAX),
        mode,
        a.as_ref().map(|v| &v[..]),
        l.as_ref().map(|v| &v[..h]),
        left_avail,
        above_left.map(P::to_i32),
        h,
        &mut out,
    );
    narrow(&out[..8 * h], output);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference implementation: compute I4x4 prediction using H.264 spec formulas
    /// with explicit p[x,-1]/p[-1,y]/p[-1,-1] indexing.
    fn spec_predict_4x4(mode: u8, above: &[u8; 8], left: &[u8; 4], above_left: u8) -> [u8; 16] {
        let pa = |x: i32| -> i32 {
            if x == -1 {
                above_left as i32
            } else {
                above[x as usize] as i32
            }
        };
        let pl = |y: i32| -> i32 {
            if y == -1 {
                above_left as i32
            } else {
                left[y as usize] as i32
            }
        };

        let mut out = [0u8; 16];
        for y in 0..4i32 {
            for x in 0..4i32 {
                let v: i32 = match mode {
                    0 => pa(x),
                    1 => pl(y),
                    3 => {
                        if x == 3 && y == 3 {
                            (pa(6) + 3 * pa(7) + 2) >> 2
                        } else {
                            (pa(x + y) + 2 * pa(x + y + 1) + pa(x + y + 2) + 2) >> 2
                        }
                    }
                    4 => {
                        if x > y {
                            (pa(x - y - 2) + 2 * pa(x - y - 1) + pa(x - y) + 2) >> 2
                        } else if x < y {
                            (pl(y - x - 2) + 2 * pl(y - x - 1) + pl(y - x) + 2) >> 2
                        } else {
                            (pa(0) + 2 * pa(-1) + pl(0) + 2) >> 2
                        }
                    }
                    5 => {
                        let zvr = 2 * x - y;
                        if zvr >= 0 && zvr % 2 == 0 {
                            let i = x - (y >> 1);
                            (pa(i - 1) + pa(i) + 1) >> 1
                        } else if zvr >= 0 {
                            let i = x - (y >> 1);
                            (pa(i - 2) + 2 * pa(i - 1) + pa(i) + 2) >> 2
                        } else if zvr == -1 {
                            (pa(0) + 2 * pa(-1) + pl(0) + 2) >> 2
                        } else {
                            // zVR < -1: use left neighbors
                            // For zVR=-3 (y=3,x=0): pl(0) + 2*pl(1) + pl(2)
                            let iy = (-1 - zvr) >> 1;
                            (pl(iy - 1) + 2 * pl(iy) + pl(iy + 1) + 2) >> 2
                        }
                    }
                    6 => {
                        let zhd = 2 * y - x;
                        if zhd >= 0 && zhd % 2 == 0 {
                            let i = y - (x >> 1);
                            (pl(i - 1) + pl(i) + 1) >> 1
                        } else if zhd >= 0 {
                            let i = y - (x >> 1);
                            (pl(i - 2) + 2 * pl(i - 1) + pl(i) + 2) >> 2
                        } else if zhd == -1 {
                            (pl(0) + 2 * pl(-1) + pa(0) + 2) >> 2
                        } else {
                            // zHD < -1: use above neighbors
                            let ix = (-1 - zhd) >> 1;
                            (pa(ix - 1) + 2 * pa(ix) + pa(ix + 1) + 2) >> 2
                        }
                    }
                    7 => {
                        let i = x + (y >> 1);
                        if y % 2 == 0 {
                            (pa(i) + pa(i + 1) + 1) >> 1
                        } else {
                            (pa(i) + 2 * pa(i + 1) + pa(i + 2) + 2) >> 2
                        }
                    }
                    8 => {
                        let zhu = x + 2 * y;
                        if zhu < 5 {
                            let i = y + (x >> 1);
                            if zhu % 2 == 0 {
                                (pl(i) + pl(i + 1) + 1) >> 1
                            } else {
                                (pl(i) + 2 * pl(i + 1) + pl(i + 2) + 2) >> 2
                            }
                        } else if zhu == 5 {
                            (pl(2) + 3 * pl(3) + 2) >> 2
                        } else {
                            pl(3)
                        }
                    }
                    _ => 128,
                };
                out[(y * 4 + x) as usize] = v.clamp(0, 255) as u8;
            }
        }
        out
    }

    #[test]
    fn test_all_directional_modes_against_spec() {
        let above: [u8; 8] = [10, 30, 50, 70, 90, 110, 130, 150];
        let left: [u8; 4] = [20, 60, 100, 140];
        let above_left: u8 = 40;

        for mode in [3u8, 4, 5, 6, 7, 8] {
            let expected = spec_predict_4x4(mode, &above, &left, above_left);
            let mut actual = [0u8; 16];
            predict_intra_4x4(
                mode,
                Some(&above[..]),
                Some(&left[..]),
                Some(above_left),
                &mut actual,
            );
            assert_eq!(
                actual, expected,
                "Mode {} mismatch.\n  actual:   {:?}\n  expected: {:?}",
                mode, actual, expected
            );
        }
    }

    #[test]
    fn test_directional_modes_uniform_input() {
        let above = [128u8; 8];
        let left = [128u8; 4];
        for mode in 0..9u8 {
            let mut output = [0u8; 16];
            predict_intra_4x4(
                mode,
                Some(&above[..]),
                Some(&left[..]),
                Some(128),
                &mut output,
            );
            assert!(
                output.iter().all(|&v| v == 128),
                "Mode {} should give all 128 for uniform input, got {:?}",
                mode,
                output
            );
        }
    }
}
