// Frozen from 09ab0b740 (0.5.1213), readout keys and functions unchanged.
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadsync_theme::views::{FrameStatsSample, FrameStatsSummary, OverlayAnchor, OverlayStyle};
use std::cell::RefCell;
use std::sync::Arc;

const TEXT_CACHE_LIMIT: usize = 1024;

// Each readout owns a separate bounded, presentation-thread cache because
// its source fields change at different rates. Entries never evict, new keys
// stop being admitted at `TEXT_CACHE_LIMIT`, and all storage drops with the
// thread. Stable summaries clone `Arc` handles without formatting or heap
// churn; changing telemetry still produces exact fresh text.
thread_local! {
    static SUMMARY_TEXT_CACHE: RefCell<TextCache<SummaryTextKey>> = RefCell::new(text_cache_with_capacity(128));
    static LOAD_TEXT_CACHE: RefCell<TextCache<LoadTextKey>> = RefCell::new(text_cache_with_capacity(128));
    static STUTTER_TEXT_CACHE: RefCell<TextCache<StutterTextKey>> = RefCell::new(text_cache_with_capacity(128));
    static DISPLAY_TEXT_CACHE: RefCell<TextCache<DisplayTextKey>> = RefCell::new(text_cache_with_capacity(128));
    static AUDIO_TEXT_CACHE: RefCell<TextCache<AudioTextKey>> = RefCell::new(text_cache_with_capacity(128));
    static COMPACT_TEXT_CACHE: RefCell<TextCache<CompactTextKey>> = RefCell::new(text_cache_with_capacity(128));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SummaryTextKey {
    fps_bits: u32,
    avg_frame_us: u32,
    frame_jitter_us: u32,
    p99_frame_us: u32,
    spike_hold_us: u32,
    target_frame_us: u32,
    show_p99: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LoadTextKey {
    cpu_work_us: u32,
    gpu_wait_us: u32,
    avg_frame_us: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StutterTextKey {
    over_budget_count: u32,
    catch_up_count: u32,
    spike_hold_us: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DisplayTextKey {
    in_gameplay: bool,
    display_error_bits: u32,
    display_error_p99_bits: u32,
    display_error_jitter_us: u32,
    display_catching_up: bool,
    show_p99: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AudioTextKey {
    callback_gap_bits: u32,
    underruns: u64,
    output_delay_bits: u32,
    queued_frames: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CompactTextKey {
    fps_bits: u32,
    avg_frame_us: u32,
    frame_jitter_us: u32,
    p99_frame_us: u32,
    spike_hold_us: u32,
    in_gameplay: bool,
    display_error_bits: u32,
    display_catching_up: bool,
    audio_underruns: u64,
    audio_output_delay_bits: u32,
    audio_callback_gap_bits: u32,
    show_p99: bool,
}

#[inline(always)]
fn ms(us: u32) -> f32 {
    us as f32 / 1000.0
}

fn summary_text(summary: &FrameStatsSummary, show_p99: bool) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(96);
    let _ = write!(
        text,
        "FRAME STATS\n{:.0} FPS\navg {:.2} \u{00b1}{:.2}ms",
        summary.fps.max(0.0),
        ms(summary.avg_frame_us),
        ms(summary.frame_jitter_us),
    );
    if show_p99 {
        let _ = write!(text, "\np99 {:.2}ms", ms(summary.p99_frame_us));
    }
    let _ = write!(text, "\nmax {:.2}ms", ms(summary.spike_hold_us));
    if summary.target_frame_us > 0 {
        let _ = write!(text, "\ntgt {:.2}ms", ms(summary.target_frame_us));
    }
    text
}

fn load_text(summary: &FrameStatsSummary) -> String {
    use std::fmt::Write;
    let cpu = summary.cpu_work_us;
    let gpu = summary.gpu_wait_us;
    let frame = summary.avg_frame_us.max(1);
    // Idle = whatever of the frame isn't CPU work or GPU wait (the graph's dark-gray band).
    let idle = frame.saturating_sub(cpu).saturating_sub(gpu);
    let idle_pct = (idle as f32 / frame as f32 * 100.0).clamp(0.0, 100.0);
    // What's limiting the frame: the largest of the three slices. Idle largest → not bound
    // by either CPU or GPU (hitting the frame cap / vsync), so report "none".
    let lim = if idle >= cpu && idle >= gpu {
        "none"
    } else if gpu >= cpu {
        "GPU"
    } else {
        "CPU"
    };
    let mut text = String::with_capacity(64);
    let _ = write!(
        text,
        "LOAD\ncpu {:.2}ms\ngpu {:.2}ms\nidle {:.0}%\nlim {}",
        ms(cpu),
        ms(gpu),
        idle_pct,
        lim,
    );
    text
}

fn stutter_text(summary: &FrameStatsSummary) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(64);
    let _ = write!(
        text,
        "STUTTER\nover-budget {}\ncatch-ups {}\nworst {:.2}ms",
        summary.over_budget_count,
        summary.catch_up_count,
        ms(summary.spike_hold_us),
    );
    text
}

fn display_text(summary: &FrameStatsSummary, show_p99: bool) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(64);
    if summary.in_gameplay {
        let _ = write!(
            text,
            "DISPLAY CLOCK\nerr {:+.2}ms",
            summary.display_error_ms
        );
        if show_p99 {
            let _ = write!(text, "\np99 {:.2}ms", summary.display_error_p99_ms);
        }
        let _ = write!(
            text,
            "\njit {:.2}ms\ncatch-up {}",
            ms(summary.display_error_jitter_us),
            if summary.display_catching_up {
                "YES"
            } else {
                "no"
            },
        );
    } else {
        let _ = write!(text, "DISPLAY CLOCK\nn/a (menu)");
    }
    text
}

fn audio_text(summary: &FrameStatsSummary) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(64);
    let _ = write!(
        text,
        "AUDIO\ngap {:.2}ms\nunderruns {}\nout {:.2}ms\nq {}",
        summary.audio_callback_gap_ms,
        summary.audio_underruns,
        summary.audio_output_delay_ms,
        summary.audio_queued_frames,
    );
    text
}

fn compact_readout_text(summary: &FrameStatsSummary, show_p99: bool) -> String {
    use std::fmt::Write;

    let mut text = String::with_capacity(96);
    let _ = write!(
        text,
        "{:.0} FPS  avg {:.2}\u{00b1}{:.2}",
        summary.fps.max(0.0),
        ms(summary.avg_frame_us),
        ms(summary.frame_jitter_us),
    );
    if show_p99 {
        let _ = write!(text, "  p99 {:.2}", ms(summary.p99_frame_us));
    }
    let _ = write!(text, "  max {:.2} ms", ms(summary.spike_hold_us));
    if summary.in_gameplay {
        let _ = write!(
            text,
            "\nerr {:+.2} ms  catch-up {}  underruns {}  out {:.1} ms",
            summary.display_error_ms,
            if summary.display_catching_up {
                "YES"
            } else {
                "no"
            },
            summary.audio_underruns,
            summary.audio_output_delay_ms,
        );
    } else {
        let _ = write!(
            text,
            "\nunderruns {}  out {:.1} ms  gap {:.2} ms",
            summary.audio_underruns, summary.audio_output_delay_ms, summary.audio_callback_gap_ms,
        );
    }
    text
}

#[inline(always)]
pub(super) fn retained_summary_text(summary: &FrameStatsSummary, show_p99: bool) -> Arc<str> {
    let key = SummaryTextKey {
        fps_bits: summary.fps.to_bits(),
        avg_frame_us: summary.avg_frame_us,
        frame_jitter_us: summary.frame_jitter_us,
        p99_frame_us: summary.p99_frame_us,
        spike_hold_us: summary.spike_hold_us,
        target_frame_us: summary.target_frame_us,
        show_p99,
    };
    cached_text(&SUMMARY_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        summary_text(summary, show_p99)
    })
}

#[inline(always)]
pub(super) fn retained_load_text(summary: &FrameStatsSummary) -> Arc<str> {
    let key = LoadTextKey {
        cpu_work_us: summary.cpu_work_us,
        gpu_wait_us: summary.gpu_wait_us,
        avg_frame_us: summary.avg_frame_us,
    };
    cached_text(&LOAD_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        load_text(summary)
    })
}

#[inline(always)]
pub(super) fn retained_stutter_text(summary: &FrameStatsSummary) -> Arc<str> {
    let key = StutterTextKey {
        over_budget_count: summary.over_budget_count,
        catch_up_count: summary.catch_up_count,
        spike_hold_us: summary.spike_hold_us,
    };
    cached_text(&STUTTER_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        stutter_text(summary)
    })
}

#[inline(always)]
pub(super) fn retained_display_text(summary: &FrameStatsSummary, show_p99: bool) -> Arc<str> {
    let key = DisplayTextKey {
        in_gameplay: summary.in_gameplay,
        display_error_bits: summary.display_error_ms.to_bits(),
        display_error_p99_bits: summary.display_error_p99_ms.to_bits(),
        display_error_jitter_us: summary.display_error_jitter_us,
        display_catching_up: summary.display_catching_up,
        show_p99,
    };
    cached_text(&DISPLAY_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        display_text(summary, show_p99)
    })
}

#[inline(always)]
pub(super) fn retained_audio_text(summary: &FrameStatsSummary) -> Arc<str> {
    let key = AudioTextKey {
        callback_gap_bits: summary.audio_callback_gap_ms.to_bits(),
        underruns: summary.audio_underruns,
        output_delay_bits: summary.audio_output_delay_ms.to_bits(),
        queued_frames: summary.audio_queued_frames,
    };
    cached_text(&AUDIO_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        audio_text(summary)
    })
}

#[inline(always)]
pub(super) fn retained_compact_text(summary: &FrameStatsSummary, show_p99: bool) -> Arc<str> {
    let key = CompactTextKey {
        fps_bits: summary.fps.to_bits(),
        avg_frame_us: summary.avg_frame_us,
        frame_jitter_us: summary.frame_jitter_us,
        p99_frame_us: summary.p99_frame_us,
        spike_hold_us: summary.spike_hold_us,
        in_gameplay: summary.in_gameplay,
        display_error_bits: summary.display_error_ms.to_bits(),
        display_catching_up: summary.display_catching_up,
        audio_underruns: summary.audio_underruns,
        audio_output_delay_bits: summary.audio_output_delay_ms.to_bits(),
        audio_callback_gap_bits: summary.audio_callback_gap_ms.to_bits(),
        show_p99,
    };
    cached_text(&COMPACT_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        compact_readout_text(summary, show_p99)
    })
}

// Test harness reset; the frozen cache and formatting bodies above are unchanged.
pub(super) fn reset() {
    SUMMARY_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
    LOAD_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
    STUTTER_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
    DISPLAY_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
    AUDIO_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
    COMPACT_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(128));
}

// Logical retained payload bytes, excluding Arc headers and hash-table storage.
pub(super) fn storage() -> (usize, usize) {
    let mut entries = 0;
    let mut bytes = 0;
    SUMMARY_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    LOAD_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    STUTTER_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    DISPLAY_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    AUDIO_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    COMPACT_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        entries += c.len();
        bytes += c.values().map(|s| s.len()).sum::<usize>();
    });
    (entries, bytes)
}
