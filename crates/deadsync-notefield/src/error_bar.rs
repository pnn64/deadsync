use crate::style::*;
use deadsync_rules::judgment::TimingWindow;

pub fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if age < 0.0 || dur <= 0.0 || age >= dur {
        return 0.0;
    }
    if !multi_tick {
        return 1.0;
    }
    let fade_start = 0.03;
    if age <= fade_start {
        1.0
    } else {
        1.0 - (age - fade_start) / (dur - fade_start)
    }
}

pub fn error_bar_flash_alpha(now: f32, started_at: Option<f32>, dur: f32) -> f32 {
    let Some(started) = started_at else {
        return ERROR_BAR_SEG_ALPHA_BASE;
    };
    if !now.is_finite() || now < started || dur <= 0.0 {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    let age = now - started;
    if age >= dur {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    1.0 + (ERROR_BAR_SEG_ALPHA_BASE - 1.0) * (age / dur)
}

pub fn error_bar_boundaries_s(
    windows: [f32; 5],
    fa_plus_s: Option<f32>,
    show_fa_plus: bool,
    max_window_ix: usize,
) -> ([f32; 6], usize) {
    let mut out = [0.0; 6];
    let mut len = 0;
    if show_fa_plus {
        if let Some(v) = fa_plus_s.filter(|v| v.is_finite() && *v > 0.0) {
            out[len] = v;
            len += 1;
        }
    }
    let max = max_window_ix.min(4);
    for w in windows.iter().take(max + 1).copied() {
        out[len] = w;
        len += 1;
    }
    (out, len)
}

pub const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

pub const fn error_bar_color_for_window(window: TimingWindow, white_w0: bool) -> [f32; 4] {
    match window {
        TimingWindow::W0 => FANTASTIC_BLUE_RGBA,
        TimingWindow::W1 => {
            if white_w0 {
                FA_PLUS_WHITE_RGBA
            } else {
                FANTASTIC_BLUE_RGBA
            }
        }
        TimingWindow::W2 => EXCELLENT_RGBA,
        TimingWindow::W3 => GREAT_RGBA,
        TimingWindow::W4 => DECENT_RGBA,
        TimingWindow::W5 => WAY_OFF_RGBA,
    }
}

pub fn error_bar_text_scalable_zoom(abs_ms: f32, scale_start_ms: f32, w2_ms: f32) -> f32 {
    let ms = if abs_ms.is_finite() {
        abs_ms
    } else {
        deadsync_rules::timing::FA_PLUS_W010_MS
    };
    let scale_start_ms = if scale_start_ms.is_finite() && scale_start_ms > 0.0 {
        scale_start_ms
    } else {
        deadsync_rules::timing::FA_PLUS_W010_MS
    };
    let w1_ms = scale_start_ms
        + (deadsync_rules::timing::FA_PLUS_W0_MS - deadsync_rules::timing::FA_PLUS_W010_MS)
            .max(0.001);
    let w2_ms = if w2_ms.is_finite() && w2_ms > w1_ms {
        w2_ms
    } else {
        w1_ms
    };
    let mut scale1 = 1.0;
    let mut scale2 = 1.0;
    if scale_start_ms < ms && ms <= w1_ms {
        scale1 = (ms - scale_start_ms) / (w1_ms - scale_start_ms);
    } else if w1_ms < ms && ms <= w2_ms && w2_ms > w1_ms {
        scale2 = (ms - w1_ms) / (w2_ms - w1_ms);
    }
    0.15 + scale1 * 0.2 + scale2 * 0.1
}
