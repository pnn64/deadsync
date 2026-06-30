#[derive(Clone, Copy, Debug)]
pub struct ErrorBarTick {
    pub started_at: f32,
    pub offset_s: f32,
    pub window: TimingWindow,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarText {
    pub started_at: f32,
    pub early: bool,
    pub offset_ms: f32,
    pub scaled: bool,
    pub scale_start_ms: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetIndicatorText {
    pub started_at: f32,
    pub offset_ms: f32,
    pub window: TimingWindow,
}

pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 = 2000;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 = 100;
pub const ERROR_BAR_LONG_AVG_SAMPLE_FILTER_S: f32 = 0.060;
pub const ERROR_BAR_LONG_AVG_PRUNE_PER_TAP: usize = 4;

#[inline(always)]
pub const fn clamp_average_error_bar_interval_ms(ms: u32) -> u32 {
    let clamped = if ms < AVERAGE_ERROR_BAR_INTERVAL_MS_MIN {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
    } else if ms > AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
    } else {
        ms
    };
    let steps = (clamped - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        + AVERAGE_ERROR_BAR_INTERVAL_MS_STEP / 2)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    AVERAGE_ERROR_BAR_INTERVAL_MS_MIN + steps * AVERAGE_ERROR_BAR_INTERVAL_MS_STEP
}

#[inline(always)]
pub const fn error_bar_window_ix(window: TimingWindow) -> usize {
    match window {
        TimingWindow::W0 => 0,
        TimingWindow::W1 => 1,
        TimingWindow::W2 => 2,
        TimingWindow::W3 => 3,
        TimingWindow::W4 => 4,
        TimingWindow::W5 => 5,
    }
}

#[inline(always)]
pub fn error_bar_long_term_offset_s(
    samples: &mut VecDeque<(f32, f32)>,
    total: &mut f32,
    music_time_s: f32,
    offset_s: f32,
    average_window_ms: u32,
) -> (f32, usize) {
    let now_ms = (music_time_s * 1000.0).max(0.0);
    if offset_s.abs() <= ERROR_BAR_LONG_AVG_SAMPLE_FILTER_S {
        samples.push_back((now_ms, offset_s));
        *total += offset_s;
    }

    let long_window_ms = clamp_average_error_bar_interval_ms(average_window_ms) as f32 * 16.0;
    let mut popped = 0usize;
    while popped < ERROR_BAR_LONG_AVG_PRUNE_PER_TAP {
        let Some((time_ms, _)) = samples.front() else {
            break;
        };
        if now_ms - *time_ms <= long_window_ms {
            break;
        }
        if let Some((_, v)) = samples.pop_front() {
            *total -= v;
            popped += 1;
        } else {
            break;
        }
    }

    let len = samples.len();
    let mean = if len > 0 { *total / len as f32 } else { 0.0 };
    (mean, len)
}

#[inline(always)]
pub fn error_bar_push_tick<const N: usize>(
    ticks: &mut [Option<ErrorBarTick>; N],
    next: &mut usize,
    multi_tick: bool,
    tick: ErrorBarTick,
) {
    let ix = if multi_tick {
        let ix = (*next) % N;
        *next = (*next + 1) % N;
        ix
    } else {
        0
    };
    ticks[ix] = Some(tick);
    if !multi_tick {
        *next = 0;
    }
}

#[inline(always)]
pub fn error_bar_average_offset_s(
    samples: &mut VecDeque<(f32, f32)>,
    music_time_s: f32,
    offset_s: f32,
    window_ms: u32,
) -> (f32, usize) {
    let now_ms = ((music_time_s * 100.0).round() * 10.0).max(0.0);
    samples.push_back((now_ms, offset_s));

    let window_ms = clamp_average_error_bar_interval_ms(window_ms) as f32;
    while let Some((t, _)) = samples.front() {
        if now_ms - *t <= window_ms {
            break;
        }
        samples.pop_front();
    }

    let mut sum = 0.0_f32;
    let mut count: usize = 0;
    let mut oldest_in_window: Option<f32> = None;
    for &(t, v) in samples.iter().rev() {
        if now_ms - t > window_ms {
            break;
        }
        sum += v;
        count += 1;
        oldest_in_window = Some(v);
    }
    if count == 0 {
        return (offset_s, 1);
    }
    if count > 1
        && (count & 1) == 1
        && let Some(oldest) = oldest_in_window
    {
        sum -= oldest;
        count -= 1;
    }
    let avg = sum / (count.max(1) as f32);
    (avg, count)
}

