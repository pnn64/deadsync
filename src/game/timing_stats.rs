use crate::game::note::{Note, NoteType};
use crate::game::judgment::JudgeGrade;
use crate::game::timing_windows;

#[derive(Copy, Clone, Debug, Default)]
pub struct TimingStats {
    pub mean_abs_ms: f32,
    pub mean_ms: f32,
    pub stddev_ms: f32,
    pub max_abs_ms: f32,
    pub count: usize,
}

#[inline(always)]
pub fn compute_note_timing_stats(notes: &[Note]) -> TimingStats {
    // First pass: accumulate sums and maxima over non-miss judgments
    let mut sum_abs = 0.0_f32;
    let mut sum_signed = 0.0_f32;
    let mut max_abs = 0.0_f32;
    let mut count: usize = 0;

    for n in notes {
        if let Some(j) = &n.result {
            if j.grade != JudgeGrade::Miss {
                let e = j.time_error_ms;
                let a = e.abs();
                sum_abs += a;
                sum_signed += e;
                if a > max_abs { max_abs = a; }
                count += 1;
            }
        }
    }

    if count == 0 {
        return TimingStats::default();
    }

    let mean_ms = sum_signed / (count as f32);
    let mean_abs_ms = sum_abs / (count as f32);

    // Second pass: sample standard deviation of signed offsets
    let stddev_ms = if count > 1 {
        let mut sum_diff_sq = 0.0_f32;
        for n in notes {
            if let Some(j) = &n.result {
                if j.grade != JudgeGrade::Miss {
                    let d = j.time_error_ms - mean_ms;
                    sum_diff_sq += d * d;
                }
            }
        }
        (sum_diff_sq / ((count as f32) - 1.0)).sqrt()
    } else { 0.0 };

    TimingStats { mean_abs_ms, mean_ms, stddev_ms, max_abs_ms: max_abs, count }
}

#[derive(Copy, Clone, Debug)]
pub struct ScatterPoint {
    pub time_sec: f32,
    pub offset_ms: Option<f32>, // None for Miss
}

#[derive(Clone, Debug, Default)]
pub struct HistogramMs {
    pub bins: Vec<(i32, u32)>,        // raw counts (bin_ms, count), sorted by bin
    pub smoothed: Vec<(i32, f32)>,    // Gaussian-smoothed counts (bin_ms, value)
    pub max_count: u32,               // peak of raw counts
    pub worst_observed_ms: f32,       // max |offset| actually observed
    pub worst_window_ms: f32,         // for scaling (-worst..+worst)
}

const HIST_BIN_MS: f32 = 1.0; // 1ms bins, like Simply Love using 0.001s
// Gaussian-like kernel used by Simply Love to soften the histogram
const GAUSS7: [f32; 7] = [0.045, 0.090, 0.180, 0.370, 0.180, 0.090, 0.045];

#[inline(always)]
pub fn build_scatter_points(notes: &[Note], note_time_cache: &[f32]) -> Vec<ScatterPoint> {
    let mut out = Vec::with_capacity(notes.len());
    for (idx, n) in notes.iter().enumerate() {
        if matches!(n.note_type, NoteType::Mine) { continue; }
        let t = note_time_cache.get(idx).copied().unwrap_or(0.0);
        let offset_ms = match n.result.as_ref() {
            Some(j) => if j.grade == JudgeGrade::Miss { None } else { Some(j.time_error_ms) },
            None => continue, // do not include unjudged notes
        };
        out.push(ScatterPoint { time_sec: t, offset_ms });
    }
    out
}

#[inline(always)]
fn bin_index_ms(v_ms: f32) -> i32 {
    // Mirror Simply Love behavior: floor to 1ms steps, with negative going more negative
    (v_ms / HIST_BIN_MS).floor() as i32
}

#[inline(always)]
pub fn build_histogram_ms(notes: &[Note]) -> HistogramMs {
    use std::collections::HashMap;
    let mut counts: HashMap<i32, u32> = HashMap::new();
    let mut max_count: u32 = 0;
    let mut max_abs: f32 = 0.0;
    // Determine worst timing window seen (at least W3 per Simply Love histogram)
    let mut worst_window_index = 3; // 1=W1..5=W5
    let mut worst_observed_bin_abs: i32 = 0;

    for n in notes {
        let Some(j) = n.result.as_ref() else { continue; };
        if j.grade == JudgeGrade::Miss { continue; }
        if matches!(n.note_type, NoteType::Mine) { continue; }
        let e = j.time_error_ms;
        let b = bin_index_ms(e);
        let c = counts.entry(b).or_insert(0);
        *c = c.saturating_add(1);
        if *c > max_count { max_count = *c; }
        let a = e.abs();
        if a > max_abs { max_abs = a; }
        if b.abs() > worst_observed_bin_abs { worst_observed_bin_abs = b.abs(); }

        match j.grade {
            JudgeGrade::WayOff => worst_window_index = worst_window_index.max(5),
            JudgeGrade::Decent => worst_window_index = worst_window_index.max(4),
            JudgeGrade::Great => worst_window_index = worst_window_index.max(3),
            JudgeGrade::Excellent => worst_window_index = worst_window_index.max(2),
            JudgeGrade::Fantastic => worst_window_index = worst_window_index.max(1),
            JudgeGrade::Miss => {}
        }
    }

    let mut bins: Vec<(i32, u32)> = counts.into_iter().collect();
    bins.sort_unstable_by_key(|(bin, _)| *bin);

    let eff = timing_windows::effective_windows_ms();
    let worst_window_ms: f32 = match worst_window_index {
        1 => eff[0],
        2 => eff[1],
        3 => eff[2],
        4 => eff[3],
        _ => eff[4],
    };

    // Build smoothed distribution across the whole timing window range (1ms steps)
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round() as i32;
    let mut smoothed: Vec<(i32, f32)> = Vec::with_capacity((worst_window_bin * 2 + 1).max(1) as usize);

    // Rebuild a fast lookup for counts
    let mut count_map: HashMap<i32, u32> = HashMap::with_capacity(bins.len());
    for (bin, c) in &bins { count_map.insert(*bin, *c); }

    for i in -worst_window_bin..=worst_window_bin {
        let mut y = 0.0_f32;
        for (j, w) in GAUSS7.iter().enumerate() {
            let offset = j as i32 - 3; // -3..+3
            let k = (i + offset).clamp(-worst_window_bin, worst_window_bin);
            let c = *count_map.get(&k).unwrap_or(&0) as f32;
            y += c * *w;
        }
        smoothed.push((i, y));
    }

    HistogramMs {
        bins,
        smoothed,
        max_count,
        worst_observed_ms: (worst_observed_bin_abs as f32) * HIST_BIN_MS,
        worst_window_ms: worst_window_ms.max(max_abs),
    }
}
