// Frozen starting implementations; unchanged row/run helpers come from the parent module.
use super::*;

pub fn measure_densities(data: &[u8], lanes: usize) -> Vec<usize> {
    match lanes {
        5 => measure_densities_impl::<5>(data),
        8 => measure_densities_impl::<8>(data),
        10 => measure_densities_impl::<10>(data),
        _ => measure_densities_impl::<4>(data),
    }
}

pub fn stream_measure_densities(data: &[u8], lanes: usize) -> Vec<u8> {
    match lanes {
        5 => stream_measure_densities_impl::<5>(data),
        8 => stream_measure_densities_impl::<8>(data),
        10 => stream_measure_densities_impl::<10>(data),
        _ => stream_measure_densities_impl::<4>(data),
    }
}

pub fn stream_run_progress(
    data: &[u8],
    lanes: usize,
    threshold: usize,
    current_measure: usize,
) -> Option<(usize, usize)> {
    match lanes {
        5 => stream_run_progress_impl::<5>(data, threshold, current_measure),
        8 => stream_run_progress_impl::<8>(data, threshold, current_measure),
        10 => stream_run_progress_impl::<10>(data, threshold, current_measure),
        _ => stream_run_progress_impl::<4>(data, threshold, current_measure),
    }
}

pub fn stream_sequences_threshold(measures: &[u8], threshold: usize) -> Vec<StreamSegment> {
    let mut segs = Vec::with_capacity(measures.len().min(64));
    for_each_stream_segment(measures, threshold, |segment| segs.push(segment));
    segs
}

fn measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
    const ROWS_PER_MEASURE_HINT: usize = 16;
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * ROWS_PER_MEASURE_HINT) + 1);
    for_each_measure_density::<LANES, 0>(data, |density| {
        densities.push(density);
        true
    });
    densities
}

fn stream_measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<u8> {
    const ROWS_PER_MEASURE_HINT: usize = 16;
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * ROWS_PER_MEASURE_HINT) + 1);
    for_each_measure_density::<LANES, 32>(data, |density| {
        densities.push(density as u8);
        true
    });
    densities
}

fn stream_run_progress_impl<const LANES: usize>(
    data: &[u8],
    threshold: usize,
    current_measure: usize,
) -> Option<(usize, usize)> {
    let mut progress = StreamProgress::new(threshold, current_measure);
    for_each_measure_density::<LANES, 0>(data, |density| progress.record(density));
    progress.finish()
}

fn for_each_measure_density<const LANES: usize, const CAP: usize>(
    data: &[u8],
    mut visit: impl FnMut(usize) -> bool,
) {
    // Empty-subdivision reduction cannot remove a step: a nonzero off-grid row
    // prevents that reduction level. The reduced density is this direct count.
    let mut measure_steps = 0usize;
    let mut done = false;

    for raw in data.split(|&byte| byte == b'\n') {
        let line = skip_ws(trim_cr(raw));
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                if !visit(std::mem::take(&mut measure_steps)) {
                    return;
                }
            }
            b';' => {
                visit(std::mem::take(&mut measure_steps));
                done = true;
                break;
            }
            _ if line.len() >= LANES && (CAP == 0 || measure_steps < CAP) => {
                measure_steps += usize::from(density_row_has_step::<LANES>(line));
            }
            _ => {}
        }
    }

    if !done {
        visit(measure_steps);
    }
}
