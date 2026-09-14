// Frozen histogram implementation from cb48d6353 (0.5.1208).
// Only count visibility differs; unchanged row selection and constants are shared.
use super::*;

#[inline(always)]
fn bin_index_ms(v_ms: f32) -> i32 {
    // Mirror Simply Love behavior: floor to 1ms steps, with negative going more negative
    (v_ms / HIST_BIN_MS).floor() as i32
}

#[derive(Copy, Clone, Debug)]
struct HistMeta {
    max_abs: f32,
    worst_window_ix: usize,
    worst_observed_bin_abs: i32,
}

#[derive(Copy, Clone, Debug)]
struct HistScan {
    meta: HistMeta,
    min_bin: i32,
    max_bin: i32,
    count: usize,
    all_fast: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct HistCounts {
    pub(super) bins: Vec<(i32, u32)>,
    pub(super) dense: Vec<u32>,
    pub(super) min_bin: i32,
    pub(super) max_count: u32,
}

const MAX_DENSE_HIST_SPAN: i32 = 4096;
const FAST_HIST_MIN_BIN: i32 = -512;
const FAST_HIST_BINS: usize = 1024;

#[inline(always)]
const fn hist_window_ix(grade: JudgeGrade) -> usize {
    match grade {
        JudgeGrade::Fantastic => 0,
        JudgeGrade::Excellent => 1,
        JudgeGrade::Great => 2,
        JudgeGrade::Decent => 3,
        JudgeGrade::WayOff => 4,
        JudgeGrade::Miss => 2,
    }
}

fn scan_hist_bins(notes: &[Note], fast_counts: &mut [u32; FAST_HIST_BINS]) -> HistScan {
    let mut scan = HistScan {
        meta: HistMeta {
            max_abs: 0.0,
            worst_window_ix: hist_window_ix(JudgeGrade::Great),
            worst_observed_bin_abs: 0,
        },
        min_bin: i32::MAX,
        max_bin: i32::MIN,
        count: 0,
        all_fast: true,
    };

    for_each_row_final_judgment(notes, |judgment| {
        if judgment.grade != JudgeGrade::Miss {
            let bin = bin_index_ms(judgment.time_error_ms);
            scan.min_bin = scan.min_bin.min(bin);
            scan.max_bin = scan.max_bin.max(bin);
            scan.count += 1;
            let fast_index = i64::from(bin) - i64::from(FAST_HIST_MIN_BIN);
            if fast_index >= 0 && fast_index < FAST_HIST_BINS as i64 {
                fast_counts[fast_index as usize] += 1;
            } else {
                scan.all_fast = false;
            }
            scan.meta.max_abs = scan.meta.max_abs.max(judgment.time_error_ms.abs());
            scan.meta.worst_window_ix = scan
                .meta
                .worst_window_ix
                .max(hist_window_ix(judgment.grade));
            scan.meta.worst_observed_bin_abs =
                scan.meta.worst_observed_bin_abs.max(bin.saturating_abs());
        }
    });

    scan
}

fn pack_hist_counts(mut seen_bins: Vec<i32>) -> HistCounts {
    if seen_bins.is_empty() {
        return HistCounts::default();
    }

    seen_bins.sort_unstable();
    let min_bin = seen_bins[0];
    let max_bin = *seen_bins.last().unwrap_or(&min_bin);
    let span = max_bin - min_bin + 1;
    let mut counts = HistCounts {
        bins: Vec::with_capacity(seen_bins.len().min(span as usize)),
        dense: if span <= MAX_DENSE_HIST_SPAN {
            vec![0; span as usize]
        } else {
            Vec::new()
        },
        min_bin,
        max_count: 0,
    };

    let mut prev = min_bin;
    let mut run_count = 0u32;
    for bin in seen_bins {
        if !counts.dense.is_empty() {
            counts.dense[(bin - min_bin) as usize] += 1;
        }
        if bin == prev {
            run_count += 1;
            continue;
        }
        counts.max_count = counts.max_count.max(run_count);
        counts.bins.push((prev, run_count));
        prev = bin;
        run_count = 1;
    }

    counts.max_count = counts.max_count.max(run_count);
    counts.bins.push((prev, run_count));
    counts
}

fn pack_dense_hist_counts(dense: Vec<u32>, min_bin: i32, bin_capacity: usize) -> HistCounts {
    let mut bins = Vec::with_capacity(bin_capacity.min(dense.len()));
    let mut max_count = 0;
    for (offset, &count) in dense.iter().enumerate() {
        if count == 0 {
            continue;
        }
        max_count = max_count.max(count);
        bins.push((min_bin.saturating_add(offset as i32), count));
    }
    HistCounts {
        bins,
        dense,
        min_bin,
        max_count,
    }
}

fn count_hist_bins(
    notes: &[Note],
    scan: HistScan,
    fast_counts: &[u32; FAST_HIST_BINS],
) -> HistCounts {
    if scan.count == 0 {
        return HistCounts::default();
    }
    if scan.all_fast {
        let start = (scan.min_bin - FAST_HIST_MIN_BIN) as usize;
        let end = (scan.max_bin - FAST_HIST_MIN_BIN) as usize + 1;
        return pack_dense_hist_counts(fast_counts[start..end].to_vec(), scan.min_bin, scan.count);
    }
    let span = i64::from(scan.max_bin) - i64::from(scan.min_bin) + 1;
    if span > i64::from(MAX_DENSE_HIST_SPAN) {
        let mut bins = Vec::with_capacity(scan.count);
        for_each_row_final_judgment(notes, |judgment| {
            if judgment.grade != JudgeGrade::Miss {
                bins.push(bin_index_ms(judgment.time_error_ms));
            }
        });
        return pack_hist_counts(bins);
    }

    let mut dense = vec![0u32; span as usize];
    for_each_row_final_judgment(notes, |judgment| {
        if judgment.grade != JudgeGrade::Miss {
            let bin = bin_index_ms(judgment.time_error_ms);
            dense[(i64::from(bin) - i64::from(scan.min_bin)) as usize] += 1;
        }
    });
    pack_dense_hist_counts(dense, scan.min_bin, scan.count)
}

fn merge_hist_counts<'a, I>(histograms: I) -> Option<HistCounts>
where
    I: Clone + Iterator<Item = &'a HistogramMs>,
{
    let bin_capacity = histograms.clone().map(|hist| hist.bins.len()).sum();
    if bin_capacity == 0 {
        return None;
    }
    let min_bin = histograms
        .clone()
        .flat_map(|hist| hist.bins.iter().map(|&(bin, _)| bin))
        .min()
        .unwrap_or(0);
    let max_bin = histograms
        .clone()
        .flat_map(|hist| hist.bins.iter().map(|&(bin, _)| bin))
        .max()
        .unwrap_or(min_bin);
    let span = i64::from(max_bin) - i64::from(min_bin) + 1;
    if span <= i64::from(MAX_DENSE_HIST_SPAN) {
        let mut dense = vec![0u32; span as usize];
        for histogram in histograms {
            for &(bin, count) in &histogram.bins {
                dense[(i64::from(bin) - i64::from(min_bin)) as usize] += count;
            }
        }
        return Some(pack_dense_hist_counts(dense, min_bin, bin_capacity));
    }

    let mut pairs = Vec::with_capacity(bin_capacity);
    for histogram in histograms {
        pairs.extend_from_slice(&histogram.bins);
    }
    pairs.sort_unstable_by_key(|&(bin, _)| bin);
    let mut bins: Vec<(i32, u32)> = Vec::with_capacity(bin_capacity);
    for (bin, count) in pairs {
        if let Some((last_bin, last_count)) = bins.last_mut()
            && *last_bin == bin
        {
            *last_count += count;
        } else {
            bins.push((bin, count));
        }
    }
    let max_count = bins.iter().map(|&(_, count)| count).max().unwrap_or(0);
    Some(HistCounts {
        bins,
        dense: Vec::new(),
        min_bin,
        max_count,
    })
}

#[inline(always)]
fn hist_count_at(counts: &HistCounts, bin: i32) -> u32 {
    if !counts.dense.is_empty() {
        let idx = bin - counts.min_bin;
        if idx >= 0 && (idx as usize) < counts.dense.len() {
            return counts.dense[idx as usize];
        }
        return 0;
    }

    counts
        .bins
        .binary_search_by_key(&bin, |(key, _)| *key)
        .map_or(0, |idx| counts.bins[idx].1)
}

pub(super) fn smooth_hist_counts(counts: &HistCounts, worst_window_bin: i32) -> Vec<(i32, f32)> {
    let mut smoothed = Vec::with_capacity((worst_window_bin * 2 + 1).max(1) as usize);
    if worst_window_bin < 0 {
        return smoothed;
    }
    let mut samples: [f32; 7] = std::array::from_fn(|index| {
        let sample =
            (-worst_window_bin + (index as i32 - 3)).clamp(-worst_window_bin, worst_window_bin);
        hist_count_at(counts, sample) as f32
    });
    for bin in -worst_window_bin..=worst_window_bin {
        let mut y = 0.0_f32;
        for (sample, weight) in samples.into_iter().zip(GAUSS7) {
            y = sample.mul_add(weight, y);
        }
        smoothed.push((bin, y));
        if bin < worst_window_bin {
            // Adjacent points share six counts. Retain the original FMA order
            // and edge clamping, but look up and convert only the entering bin.
            samples.rotate_left(1);
            let sample = (bin + 4).clamp(-worst_window_bin, worst_window_bin);
            samples[6] = hist_count_at(counts, sample) as f32;
        }
    }
    smoothed
}

#[inline(always)]
#[must_use]
pub fn build_histogram_ms(notes: &[Note]) -> HistogramMs {
    let mut fast_counts = [0u32; FAST_HIST_BINS];
    let scan = scan_hist_bins(notes, &mut fast_counts);
    let counts = count_hist_bins(notes, scan, &fast_counts);
    let worst_window_ms = effective_windows_ms()[scan.meta.worst_window_ix];
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round() as i32;
    let smoothed = smooth_hist_counts(&counts, worst_window_bin);

    HistogramMs {
        bins: counts.bins,
        smoothed,
        max_count: counts.max_count,
        worst_observed_ms: (scan.meta.worst_observed_bin_abs as f32) * HIST_BIN_MS,
        worst_window_ms: worst_window_ms.max(scan.meta.max_abs),
    }
}

#[must_use]
pub fn merge_histograms_ms(histograms: &[HistogramMs]) -> HistogramMs {
    merge_histograms_ms_iter(histograms.iter())
}

pub fn merge_histograms_ms_iter<'a, I>(histograms: I) -> HistogramMs
where
    I: Clone + Iterator<Item = &'a HistogramMs>,
{
    let Some(counts) = merge_hist_counts(histograms.clone()) else {
        return HistogramMs::default();
    };

    let mut worst_observed_ms = 0.0_f32;
    let mut worst_window_ms = 0.0_f32;
    for hist in histograms {
        worst_observed_ms = worst_observed_ms.max(hist.worst_observed_ms);
        worst_window_ms = worst_window_ms.max(hist.worst_window_ms);
    }

    let worst_window_ms = worst_window_ms
        .max(worst_observed_ms)
        .max(effective_windows_ms()[1]);
    let worst_window_bin = (worst_window_ms / HIST_BIN_MS).round().max(1.0) as i32;
    let smoothed = smooth_hist_counts(&counts, worst_window_bin);

    HistogramMs {
        bins: counts.bins,
        smoothed,
        max_count: counts.max_count,
        worst_observed_ms,
        worst_window_ms,
    }
}
