use crate::game::timing::TimingData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphImageData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

fn build_row_gradient(height: u32, bottom: [u8; 3], top: [u8; 3]) -> Vec<[u8; 4]> {
    if height == 0 {
        return Vec::new();
    }
    if height == 1 {
        return vec![[bottom[0], bottom[1], bottom[2], 255]];
    }

    let denom = (height - 1) as f32;
    let mut out = Vec::with_capacity(height as usize);
    for y in 0..height {
        let frac = (height - 1 - y) as f32 / denom;
        let lerp = |a: u8, b: u8| -> u8 {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * frac)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        out.push([lerp(bottom[0], top[0]), lerp(bottom[1], top[1]), lerp(bottom[2], top[2]), 255]);
    }
    out
}

fn fill_bg_rgba(pixels: &mut [u8], bg: [u8; 3]) {
    let [r, g, b] = bg;
    for px in pixels.chunks_exact_mut(4) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        px[3] = 255;
    }
}

fn build_points(
    measure_nps: &[f64],
    peak_nps: f64,
    timing: &TimingData,
    first_second: f32,
    last_second: f32,
    width: u32,
    height: u32,
) -> Vec<(f32, f32)> {
    let Some(start_ix) = measure_nps.iter().position(|&nps| nps > 0.0) else {
        return Vec::new();
    };
    let denom = (last_second - first_second).max(0.001_f32);
    let peak = (peak_nps as f32).max(0.000_001_f32);
    let w = width as f32;
    let h = height as f32;

    let mut out: Vec<(f32, f32)> = Vec::with_capacity(measure_nps.len() - start_ix + 1);
    for (i, &nps_f64) in measure_nps.iter().enumerate().skip(start_ix) {
        let t = timing.get_time_for_beat(i as f32 * 4.0);
        let x = ((t - first_second) / denom * w).clamp(0.0, w);
        let frac = ((nps_f64 as f32) / peak).clamp(0.0, 1.0);
        let bar_h = (frac * h).round();

        if let Some(last) = out.last_mut()
            && x <= last.0
        {
            last.1 = bar_h;
            continue;
        }
        out.push((x, bar_h));
    }

    if !out.is_empty() && measure_nps.last().is_some_and(|&n| n != 0.0) {
        if out.last().is_some_and(|&(x, _)| x < w) {
            out.push((w, 0.0));
        } else if let Some(last) = out.last_mut() {
            last.0 = w;
            last.1 = 0.0;
        }
    }

    out
}

fn build_col_heights(points: &[(f32, f32)], width: u32, height: u32) -> Vec<u16> {
    let w = width as usize;
    let h_max = height as f32;
    let mut out = vec![0u16; w];
    if points.is_empty() || width == 0 || height == 0 {
        return out;
    }

    if points.len() == 1 {
        let (x0, h0) = points[0];
        let xi = x0.round() as i32;
        if (0..width as i32).contains(&xi) {
            out[xi as usize] = h0.round().clamp(0.0, h_max) as u16;
        }
        return out;
    }

    let mut seg = 0usize;
    let first_x = points[0].0;
    for x in 0..w {
        let x_f = x as f32;
        if x_f < first_x {
            continue;
        }
        while seg + 1 < points.len() && points[seg + 1].0 <= x_f {
            seg += 1;
        }
        if seg + 1 >= points.len() {
            break;
        }

        let (x0, h0) = points[seg];
        let (x1, h1) = points[seg + 1];
        let dx = x1 - x0;
        if dx <= 0.000_001 {
            continue;
        }

        let t = (x_f - x0) / dx;
        let h_x = (h0 + (h1 - h0) * t).round().clamp(0.0, h_max);
        out[x] = h_x as u16;
    }

    out
}

pub fn render_density_graph_rgba(
    measure_nps: &[f64],
    peak_nps: f64,
    timing: &TimingData,
    first_second: f32,
    last_second: f32,
    width: u32,
    height: u32,
    bottom_color: [u8; 3],
    top_color: [u8; 3],
    bg_color: [u8; 3],
) -> GraphImageData {
    if width == 0 || height == 0 {
        return GraphImageData {
            width,
            height,
            data: Vec::new(),
        };
    }

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    fill_bg_rgba(&mut pixels, bg_color);

    let denom = last_second - first_second;
    if measure_nps.is_empty() || peak_nps <= 0.0 || !denom.is_finite() || denom <= 0.0 {
        return GraphImageData {
            width,
            height,
            data: pixels,
        };
    }

    let points = build_points(
        measure_nps,
        peak_nps,
        timing,
        first_second,
        last_second,
        width,
        height,
    );
    if points.is_empty() {
        return GraphImageData {
            width,
            height,
            data: pixels,
        };
    }

    let grad = build_row_gradient(height, bottom_color, top_color);
    let col_heights = build_col_heights(&points, width, height);

    for x in 0..width as usize {
        let bar_h = col_heights[x] as u32;
        if bar_h == 0 {
            continue;
        }
        let y_top = height.saturating_sub(bar_h);
        for y in y_top..height {
            let idx = (y * width) as usize * 4 + x * 4;
            pixels[idx..idx + 4].copy_from_slice(&grad[y as usize]);
        }
    }

    GraphImageData {
        width,
        height,
        data: pixels,
    }
}

