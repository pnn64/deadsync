use crate::act;
use deadlib_present::actors::Actor;
use deadsync_screens::diagnostics::{
    FrameStatsSample, FrameStatsSummary, HISTOGRAM_BINS, OverlayAnchor, OverlayStyle, histogram,
};

const DEBUG_OVERLAY_Z: i16 = 32030;

/// Gap between the panel and the nearest screen edge (logical px).
const MARGIN: f32 = 16.0;
/// Top margin for top-anchored panels. Sized so the panel clears the gameplay song-meter/
/// progress bar (centered at y≈20, height 22 → bottom ≈31) plus a small gap. Applied on
/// every screen so the overlay keeps a steady position between menus and gameplay.
const TOP_PROGRESS_OFFSET: f32 = 38.0;

// Full (1-player) panel geometry: graph (and optional histogram) stacked on top, with the
// five data cells in a single row below. Each cell is sized to its own text (not a shared
// fixed width) so columns hug their content — tight gaps between panes and no wasted space
// after the last one — while the whole panel stays within half the screen.
const DATA_COLS: usize = 5;
const GRAPH_H: f32 = 96.0;
const HIST_GAP: f32 = 10.0;
const HIST_H: f32 = 56.0;
const PANEL_PAD: f32 = 6.0;
const GRAPH_TEXT_GAP: f32 = 12.0;
/// Gap between the histogram and the data row below it.
const DATA_GAP: f32 = 12.0;
/// miso line pitch at zoom 0.5 (font LineSpacing 24 × 0.5). The data block height is sized
/// from the tallest cell's line count so there's no fixed over-budget margin below the text.
const DATA_LINE_PITCH: f32 = 12.0;
/// Visible height of the final text line (≈ miso Baseline 19 × 0.5), added once on top of
/// the inter-line pitches so the block hugs the last line without clipping it.
const DATA_LINE_CAP: f32 = 10.0;
/// Estimated horizontal advance per character of the readout text at zoom 0.5 (miso is a
/// narrow proportional font). Used to size each data cell to its own longest line.
const DATA_CELL_CHAR_W: f32 = 4.5;
/// Padding added to a data cell's estimated text width.
const DATA_CELL_PAD: f32 = 4.0;
/// Horizontal gap between adjacent data cells.
const DATA_CELL_GAP: f32 = 10.0;

/// Height of a data block with `lines` text rows (0 → 0). One cap height plus one pitch per
/// additional line.
#[inline(always)]
fn data_block_h(lines: usize) -> f32 {
    if lines == 0 {
        0.0
    } else {
        DATA_LINE_CAP + DATA_LINE_PITCH * (lines as f32 - 1.0)
    }
}

/// Number of text lines in a block (newlines + 1; empty → 0).
#[inline(always)]
fn line_count(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.bytes().filter(|&b| b == b'\n').count() + 1
    }
}

/// Estimated rendered width (px) of a data cell: its longest line's character count times the
/// per-character advance, plus a little padding. Empty text → 0.
#[inline(always)]
fn data_cell_width(text: &str) -> f32 {
    let max_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    if max_chars == 0 {
        0.0
    } else {
        max_chars as f32 * DATA_CELL_CHAR_W + DATA_CELL_PAD
    }
}

/// Total width of the data row given each cell's width: the cells plus the inter-cell gaps.
#[inline(always)]
fn data_row_width(cell_widths: &[f32; DATA_COLS]) -> f32 {
    cell_widths.iter().sum::<f32>() + DATA_CELL_GAP * (DATA_COLS as f32 - 1.0)
}

// Compact (2-player) panel geometry: small graph + one inline readout line.
const COMPACT_GRAPH_W: f32 = 160.0;
const COMPACT_GRAPH_H: f32 = 40.0;
const COMPACT_TEXT_W: f32 = 230.0;
const COMPACT_W: f32 = COMPACT_GRAPH_W + GRAPH_TEXT_GAP + COMPACT_TEXT_W;
const COMPACT_H: f32 = COMPACT_GRAPH_H;

// Per-phase segment colors (stacked from the bottom of each column).
const COLOR_INPUT: [f32; 3] = [0.35, 0.70, 1.00];
const COLOR_UPDATE: [f32; 3] = [0.40, 0.90, 0.45];
const COLOR_COMPOSE: [f32; 3] = [0.95, 0.85, 0.30];
const COLOR_UPLOAD: [f32; 3] = [0.95, 0.60, 0.25];
const COLOR_DRAW: [f32; 3] = [0.95, 0.35, 0.35];
const COLOR_GPU_WAIT: [f32; 3] = [0.75, 0.45, 0.95];
const COLOR_IDLE: [f32; 3] = [0.30, 0.30, 0.34];

const COLOR_HIST: [f32; 4] = [0.55, 0.80, 1.00, 0.95];
const COLOR_MARKER_CATCHUP: [f32; 4] = [1.00, 0.85, 0.20, 0.85];
const COLOR_MARKER_SPIKE: [f32; 4] = [1.00, 0.30, 0.30, 0.85];

// osu!-style horizontal reference lines drawn over the graph: the monitor's target frame
// time and twice that (the "stutter" threshold). They give the eye a fixed yardstick so
// jitter is read against a stable baseline instead of an auto-scaled one.
const COLOR_REF_TARGET: [f32; 4] = [0.55, 0.95, 0.55, 0.55];
const COLOR_REF_DOUBLE: [f32; 4] = [0.95, 0.55, 0.30, 0.45];

#[inline(always)]
fn anchor_is_right(a: OverlayAnchor) -> bool {
    matches!(a, OverlayAnchor::TopRight | OverlayAnchor::BottomRight)
}

#[inline(always)]
fn anchor_is_bottom(a: OverlayAnchor) -> bool {
    matches!(
        a,
        OverlayAnchor::BottomLeft | OverlayAnchor::BottomRight | OverlayAnchor::BottomCenter
    )
}

#[inline(always)]
fn anchor_is_center(a: OverlayAnchor) -> bool {
    matches!(a, OverlayAnchor::TopCenter | OverlayAnchor::BottomCenter)
}

/// Resolved pixel rectangles for one overlay layout. The graph and (optional) histogram
/// are stacked; the readout text sits to one side of the graph (mirrored to the left for
/// right-anchored panels so it never runs off-screen).
struct Layout {
    graph_x: f32,
    graph_y: f32,
    graph_w: f32,
    graph_h: f32,
    hist_x: f32,
    hist_y: f32,
    hist_w: f32,
    hist_h: f32,
    text_x: f32,
    text_y: f32,
    text_right: bool,
    // Full-mode data row: four cell x-origins at a shared y. Unused (0) in compact mode.
    data_cols: [f32; DATA_COLS],
    data_y: f32,
}

/// Top-left origin of the panel bounding box for `anchor`, given the box size and screen.
/// Top-anchored panels start below the song-meter/progress-bar band (≈y 31) so they don't
/// overlap it during gameplay; this offset is applied on every screen so the overlay keeps a
/// consistent position when moving between menus and gameplay. Bottom anchors hug the margin.
fn panel_origin(
    anchor: OverlayAnchor,
    box_w: f32,
    box_h: f32,
    screen_w: f32,
    screen_h: f32,
) -> (f32, f32) {
    let x = if anchor_is_center(anchor) {
        (screen_w - box_w) * 0.5
    } else if anchor_is_right(anchor) {
        screen_w - MARGIN - box_w
    } else {
        MARGIN
    };
    let y = if anchor_is_bottom(anchor) {
        screen_h - MARGIN - box_h
    } else {
        TOP_PROGRESS_OFFSET
    };
    (x.max(0.0), y.max(0.0))
}

fn compute_layout(
    anchor: OverlayAnchor,
    compact: bool,
    show_histogram: bool,
    data_h: f32,
    cell_widths: [f32; DATA_COLS],
    screen_w: f32,
    screen_h: f32,
) -> Layout {
    let right = anchor_is_right(anchor);
    if compact {
        let (ox, oy) = panel_origin(anchor, COMPACT_W, COMPACT_H, screen_w, screen_h);
        // Right-anchored: text on the left, graph on the right. Otherwise graph then text.
        let (graph_x, text_x) = if right {
            (ox + COMPACT_TEXT_W + GRAPH_TEXT_GAP, ox + COMPACT_TEXT_W)
        } else {
            (ox, ox + COMPACT_GRAPH_W + GRAPH_TEXT_GAP)
        };
        Layout {
            graph_x,
            graph_y: oy,
            graph_w: COMPACT_GRAPH_W,
            graph_h: COMPACT_GRAPH_H,
            hist_x: 0.0,
            hist_y: 0.0,
            hist_w: 0.0,
            hist_h: 0.0,
            text_x,
            text_y: oy,
            text_right: right,
            data_cols: [0.0; DATA_COLS],
            data_y: 0.0,
        }
    } else {
        // Vertical stack: graph, optional histogram, then a single row of content-sized data
        // cells laid left to right with a fixed gap. The graph and histogram span the same
        // total width, so the panel hugs its content with no wasted space on the right. In
        // minimal style the histogram is dropped, so the panel is shorter and the data row
        // slides up under the graph.
        let content_w = data_row_width(&cell_widths);
        let hist_h = if show_histogram { HIST_H } else { 0.0 };
        let stack_after_graph = if show_histogram {
            HIST_GAP + HIST_H
        } else {
            0.0
        };
        let full_h = GRAPH_H + stack_after_graph + DATA_GAP + data_h;
        let (ox, oy) = panel_origin(anchor, content_w, full_h, screen_w, screen_h);
        let hist_y = oy + GRAPH_H + HIST_GAP;
        let data_y = oy + GRAPH_H + stack_after_graph + DATA_GAP;
        let mut data_cols = [0.0; DATA_COLS];
        let mut cx = ox;
        for (i, col) in data_cols.iter_mut().enumerate() {
            *col = cx;
            cx += cell_widths[i] + DATA_CELL_GAP;
        }
        Layout {
            graph_x: ox,
            graph_y: oy,
            graph_w: content_w,
            graph_h: GRAPH_H,
            hist_x: ox,
            hist_y,
            hist_w: content_w,
            hist_h,
            text_x: ox,
            text_y: oy,
            text_right: false,
            data_cols,
            data_y,
        }
    }
}

#[inline(always)]
fn quad(x: f32, y: f32, w: f32, h: f32, c: [f32; 4], z: i16) -> Actor {
    act!(quad:
        align(0.0, 0.0):
        xy(x, y):
        zoomto(w.max(0.0), h.max(0.0)):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z)
    )
}

#[inline(always)]
fn segment(
    actors: &mut Vec<Actor>,
    x: f32,
    baseline: f32,
    drawn: &mut f32,
    seg_us: u32,
    px_per_us: f32,
    col_w: f32,
    rgb: [f32; 3],
) {
    if seg_us == 0 {
        return;
    }
    let h = seg_us as f32 * px_per_us;
    if h < 0.25 {
        return;
    }
    let top = baseline - *drawn - h;
    actors.push(quad(
        x,
        top,
        col_w,
        h,
        [rgb[0], rgb[1], rgb[2], 0.95],
        DEBUG_OVERLAY_Z + 1,
    ));
    *drawn += h;
}

fn build_graph(
    actors: &mut Vec<Actor>,
    samples: &[FrameStatsSample],
    scale_us: u32,
    target_frame_us: u32,
    gx: f32,
    gy: f32,
    gw: f32,
    gh: f32,
) {
    let baseline = gy + gh;
    let n = samples.len().max(1);
    let col_w = gw / n as f32;
    let px_per_us = gh / scale_us.max(1) as f32;
    let spike_us = scale_us.max(1) * 2 / 3;

    for (i, s) in samples.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let x = gx + i as f32 * col_w;
        let mut drawn = 0.0f32;
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.idle_us(),
            px_per_us,
            col_w,
            COLOR_IDLE,
        );
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.input_us,
            px_per_us,
            col_w,
            COLOR_INPUT,
        );
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.update_us,
            px_per_us,
            col_w,
            COLOR_UPDATE,
        );
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.compose_us,
            px_per_us,
            col_w,
            COLOR_COMPOSE,
        );
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.upload_us,
            px_per_us,
            col_w,
            COLOR_UPLOAD,
        );
        segment(
            actors, x, baseline, &mut drawn, s.draw_us, px_per_us, col_w, COLOR_DRAW,
        );
        segment(
            actors,
            x,
            baseline,
            &mut drawn,
            s.gpu_wait_us,
            px_per_us,
            col_w,
            COLOR_GPU_WAIT,
        );

        // Event markers (osu! GC-marker analog): a full-height vertical line on frames
        // where the display clock is actively catching up, or that spiked well past scale.
        if s.catching_up {
            actors.push(quad(
                x,
                gy,
                col_w.max(1.0),
                gh,
                COLOR_MARKER_CATCHUP,
                DEBUG_OVERLAY_Z + 2,
            ));
        } else if s.frame_us >= spike_us {
            actors.push(quad(
                x,
                gy,
                col_w.max(1.0),
                gh,
                COLOR_MARKER_SPIKE,
                DEBUG_OVERLAY_Z + 2,
            ));
        }
    }

    // osu!-style fixed reference lines: the monitor's target frame time and twice it.
    // Drawn last so they sit above the bars and markers; skipped if off the graph.
    ref_line(
        actors,
        target_frame_us,
        scale_us,
        gx,
        baseline,
        gw,
        gh,
        COLOR_REF_TARGET,
    );
    ref_line(
        actors,
        target_frame_us.saturating_mul(2),
        scale_us,
        gx,
        baseline,
        gw,
        gh,
        COLOR_REF_DOUBLE,
    );
}

/// Draw a horizontal reference line at `value_us` over the graph, if it falls on-scale.
#[inline]
fn ref_line(
    actors: &mut Vec<Actor>,
    value_us: u32,
    scale_us: u32,
    gx: f32,
    baseline: f32,
    gw: f32,
    gh: f32,
    color: [f32; 4],
) {
    if value_us == 0 || value_us >= scale_us {
        return;
    }
    let px_per_us = gh / scale_us.max(1) as f32;
    let y = baseline - value_us as f32 * px_per_us;
    actors.push(quad(gx, y - 0.5, gw, 1.0, color, DEBUG_OVERLAY_Z + 3));
}

fn build_histogram(
    actors: &mut Vec<Actor>,
    samples: &[FrameStatsSample],
    bin_width_us: u32,
    hx: f32,
    hy: f32,
    hw: f32,
    hh: f32,
) {
    let mut bins = [0u32; HISTOGRAM_BINS];
    histogram(samples, &mut bins, bin_width_us);
    let peak = bins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let baseline = hy + hh;
    let col_w = hw / HISTOGRAM_BINS as f32;
    for (i, &count) in bins.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let h = (count as f32 / peak) * hh;
        let x = hx + i as f32 * col_w;
        actors.push(quad(
            x,
            baseline - h,
            col_w - 0.5,
            h,
            COLOR_HIST,
            DEBUG_OVERLAY_Z + 1,
        ));
    }
}

#[inline(always)]
fn ms(us: u32) -> f32 {
    us as f32 / 1000.0
}

/// Top-left data cell: rolling frame-time summary. Numbers are smoothed (EWMA mean/jitter,
/// decaying-histogram p99, slow-decay max-hold) so they read steadily instead of flickering.
/// In minimal style the p99 line is omitted (osu! reports no percentile).
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

/// Second data cell: live CPU/GPU load breakdown. Shows the smoothed CPU work and GPU/swap
/// wait per frame, the idle headroom as a percentage, and which slice currently dominates
/// the frame ("lim CPU/GPU", or "none" when idle headroom dominates = frame-rate capped).
/// Replaces the old static color legend with numbers that actually move.
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

/// Third data cell: stutter tally — how often and how badly the frame loop hitched recently.
/// `over-budget` counts frames past the 2× stutter threshold in the rolling ring window,
/// `catch-ups` counts distinct display-clock resync events, and `worst` is the slow-decay
/// worst-frame hold. All a steady "did I hitch?" readout that matches the graph's markers.
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

/// Bottom-left data cell: display-clock sync health. The p99 line is omitted in minimal style.
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

/// Bottom-right data cell: audio output health.
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

/// Push a left-aligned miso text block at `(x, y)`.
fn push_text_block(actors: &mut Vec<Actor>, x: f32, y: f32, zoom: f32, text: String) {
    actors.push(act!(text:
        align(0.0, 0.0):
        xy(x, y):
        zoom(zoom):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        settext(text):
        horizalign(left):
        z(DEBUG_OVERLAY_Z + 1)
    ));
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

/// Build the frame-statistics overlay actors anchored at `anchor`. Full mode draws a
/// rolling per-phase stacked column graph (with idle, await-GPU and event-marker overlays),
/// a jitter histogram, and a multi-line sync-health readout. Compact mode (2 players) drops
/// the histogram for a small graph plus a two-line inline readout so it covers less of
/// either notefield. `style` selects the presentation: `Detailed` shows the histogram and
/// p99 readouts; `Minimal` drops both (the graph is the jitter display). Allocation-light:
/// one pre-sized `Vec`, no per-sample heap.
pub fn build(
    samples: &[FrameStatsSample],
    summary: FrameStatsSummary,
    anchor: OverlayAnchor,
    compact: bool,
    style: OverlayStyle,
    screen_w: f32,
    screen_h: f32,
) -> Vec<Actor> {
    let show_p99 = style.show_p99();
    // The histogram is a full-mode-only panel; minimal style drops it entirely.
    let show_histogram = !compact && style.show_histogram();

    // Scale the graph to the worst recent frame (held briefly via spike-hold so the scale
    // doesn't snap back the instant a spike leaves the ring), with a sane floor.
    let scale_us = summary.max_frame_us.max(summary.spike_hold_us).max(20_000);
    let bin_width_us = (scale_us / HISTOGRAM_BINS as u32).max(250);

    // Build the full-mode data-cell strings up front so the data block is sized to the
    // tallest cell that's actually rendered (varies by style + gameplay/menu), leaving no
    // fixed over-budget margin below the text. Skipped entirely in compact mode.
    let data_cells: Option<[String; DATA_COLS]> = if compact {
        None
    } else {
        Some([
            summary_text(&summary, show_p99),
            load_text(&summary),
            stutter_text(&summary),
            display_text(&summary, show_p99),
            audio_text(&summary),
        ])
    };
    let data_h = match &data_cells {
        None => 0.0,
        Some(cells) => {
            let max_lines = cells.iter().map(|s| line_count(s)).max().unwrap_or(0);
            data_block_h(max_lines)
        }
    };
    // Per-cell widths so each data column hugs its own text (no shared fixed width).
    let cell_widths: [f32; DATA_COLS] = match &data_cells {
        None => [0.0; DATA_COLS],
        Some(cells) => {
            let mut w = [0.0; DATA_COLS];
            for (i, c) in cells.iter().enumerate() {
                w[i] = data_cell_width(c);
            }
            w
        }
    };

    let layout = compute_layout(
        anchor,
        compact,
        show_histogram,
        data_h,
        cell_widths,
        screen_w,
        screen_h,
    );
    let mut actors = Vec::with_capacity(samples.len() * 7 + HISTOGRAM_BINS + 16);

    // Panel background behind the graph.
    actors.push(quad(
        layout.graph_x - PANEL_PAD,
        layout.graph_y - PANEL_PAD,
        layout.graph_w + PANEL_PAD * 2.0,
        layout.graph_h + PANEL_PAD * 2.0,
        [0.0, 0.0, 0.0, 0.55],
        DEBUG_OVERLAY_Z,
    ));
    if layout.hist_h > 0.0 {
        actors.push(quad(
            layout.hist_x - PANEL_PAD,
            layout.hist_y - PANEL_PAD,
            layout.hist_w + PANEL_PAD * 2.0,
            layout.hist_h + PANEL_PAD * 2.0,
            [0.0, 0.0, 0.0, 0.55],
            DEBUG_OVERLAY_Z,
        ));
    }

    build_graph(
        &mut actors,
        samples,
        scale_us,
        summary.target_frame_us,
        layout.graph_x,
        layout.graph_y,
        layout.graph_w,
        layout.graph_h,
    );
    if layout.hist_h > 0.0 {
        build_histogram(
            &mut actors,
            samples,
            bin_width_us,
            layout.hist_x,
            layout.hist_y,
            layout.hist_w,
            layout.hist_h,
        );
    }

    if compact {
        // Compact: a single inline readout beside the small graph (mirrored when right-anchored).
        let text = compact_readout_text(&summary, show_p99);
        let text_y = layout.text_y - PANEL_PAD;
        if layout.text_right {
            actors.push(act!(text:
                align(1.0, 0.0):
                xy(layout.text_x, text_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                font("miso"):
                settext(text):
                horizalign(right):
                z(DEBUG_OVERLAY_Z + 1)
            ));
        } else {
            push_text_block(&mut actors, layout.text_x, text_y, 0.5, text);
        }
    } else {
        // Full: a single row of content-sized data cells stacked under the graph (and optional
        // histogram). The background spans the same width as the graph (the total content
        // width) and is sized to the tallest cell (data_h), so it hugs the text on every side.
        actors.push(quad(
            layout.graph_x - PANEL_PAD,
            layout.data_y - PANEL_PAD,
            layout.graph_w + PANEL_PAD * 2.0,
            data_h + PANEL_PAD * 2.0,
            [0.0, 0.0, 0.0, 0.55],
            DEBUG_OVERLAY_Z,
        ));
        let [c0, c1, c2, c3, c4] = data_cells.expect("full mode builds data_cells");
        push_text_block(&mut actors, layout.data_cols[0], layout.data_y, 0.5, c0);
        push_text_block(&mut actors, layout.data_cols[1], layout.data_y, 0.5, c1);
        push_text_block(&mut actors, layout.data_cols[2], layout.data_y, 0.5, c2);
        push_text_block(&mut actors, layout.data_cols[3], layout.data_y, 0.5, c3);
        push_text_block(&mut actors, layout.data_cols[4], layout.data_y, 0.5, c4);
    }

    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary() -> FrameStatsSummary {
        FrameStatsSummary {
            avg_frame_us: 16_600,
            p99_frame_us: 22_000,
            max_frame_us: 30_000,
            fps: 60.0,
            display_error_ms: 0.4,
            display_error_p99_ms: 1.2,
            display_catching_up: false,
            in_gameplay: true,
            audio_callback_gap_ms: 0.1,
            audio_underruns: 0,
            audio_output_delay_ms: 12.0,
            audio_queued_frames: 1024,
            frame_jitter_us: 800,
            display_error_jitter_us: 300,
            spike_hold_us: 30_000,
            target_frame_us: 16_667,
            cpu_work_us: 1_800,
            gpu_wait_us: 2_300,
            over_budget_count: 2,
            catch_up_count: 1,
        }
    }

    #[test]
    fn ref_line_clips_off_scale_values() {
        let mut actors = Vec::new();
        // Target above the graph scale draws nothing; on-scale draws one line quad.
        ref_line(
            &mut actors,
            40_000,
            20_000,
            0.0,
            100.0,
            200.0,
            100.0,
            COLOR_REF_TARGET,
        );
        assert!(
            actors.is_empty(),
            "off-scale reference line should be skipped"
        );
        ref_line(
            &mut actors,
            16_700,
            33_400,
            0.0,
            100.0,
            200.0,
            100.0,
            COLOR_REF_TARGET,
        );
        assert_eq!(
            actors.len(),
            1,
            "on-scale reference line should draw one quad"
        );
    }

    #[test]
    fn layout_stacks_graph_over_content_sized_data_row() {
        let sw = 854.0;
        let sh = 480.0;
        let data_h = data_block_h(6);
        // Representative per-cell widths (FRAME STATS widest, the rest narrower).
        let widths: [f32; DATA_COLS] = [80.0, 48.0, 62.0, 62.0, 52.0];
        let content_w = data_row_width(&widths);
        // Detailed mode: graph, histogram and a single content-sized data row stack vertically.
        let l = compute_layout(OverlayAnchor::TopLeft, false, true, data_h, widths, sw, sh);
        assert!((l.graph_x - MARGIN).abs() < 0.01);
        // Top anchors start at the fixed progress-bar offset (applied on every screen so the
        // overlay keeps a steady position between menus and gameplay).
        assert!((l.graph_y - TOP_PROGRESS_OFFSET).abs() < 0.01);
        assert!(!l.text_right);
        // Histogram sits directly below the graph; data row below the histogram.
        assert!(l.hist_y > l.graph_y);
        assert!((l.hist_x - l.graph_x).abs() < 0.01);
        assert!(l.data_y > l.hist_y);
        // The graph spans the total content width and stays under half the screen.
        assert!((l.graph_w - content_w).abs() < 0.01);
        assert!(
            l.graph_w <= sw * 0.5 + 0.01,
            "graph_w {} should be <= half screen",
            l.graph_w
        );
        // Cells are laid left to right: each starts after the previous cell's width + the gap.
        assert!((l.data_cols[0] - l.graph_x).abs() < 0.01);
        for i in 1..DATA_COLS {
            let expected = l.data_cols[i - 1] + widths[i - 1] + DATA_CELL_GAP;
            assert!((l.data_cols[i] - expected).abs() < 0.01);
        }
        // The last cell ends right at the panel's right edge (no wasted trailing space).
        let last_end = l.data_cols[DATA_COLS - 1] + widths[DATA_COLS - 1];
        assert!((last_end - (l.graph_x + l.graph_w)).abs() < 0.01);
        // Right anchor: whole panel shifts right but layout stays left-aligned (no mirror).
        let r = compute_layout(OverlayAnchor::TopRight, false, true, data_h, widths, sw, sh);
        assert!(r.graph_x > l.graph_x);
        assert!(!r.text_right);
        assert!((r.hist_x - r.graph_x).abs() < 0.01);
        assert!(r.graph_x + r.graph_w <= sw - MARGIN + 0.01);
        // Bottom anchor grows upward from the bottom margin.
        let b = compute_layout(
            OverlayAnchor::BottomLeft,
            false,
            true,
            data_h,
            widths,
            sw,
            sh,
        );
        let full_h = GRAPH_H + HIST_GAP + HIST_H + DATA_GAP + data_h;
        assert!((b.graph_y + full_h - (sh - MARGIN)).abs() < 0.01);
        // Compact mode drops the histogram and the data row.
        let c = compute_layout(OverlayAnchor::BottomCenter, true, true, 0.0, widths, sw, sh);
        assert_eq!(c.hist_h, 0.0);
        assert_eq!(c.data_y, 0.0);
    }

    #[test]
    fn data_cell_width_hugs_longest_line() {
        assert_eq!(data_cell_width(""), 0.0);
        // Width tracks the longest line's character count.
        let one = data_cell_width("abc");
        let two = data_cell_width("abc\nlonger line");
        assert!(two > one);
        assert!((data_cell_width("abc") - (3.0 * DATA_CELL_CHAR_W + DATA_CELL_PAD)).abs() < 0.01);
        // The row width is the cells plus the inter-cell gaps.
        let widths = [10.0, 20.0, 30.0, 40.0, 50.0];
        assert!((data_row_width(&widths) - (150.0 + 4.0 * DATA_CELL_GAP)).abs() < 0.01);
    }

    #[test]
    fn data_block_height_hugs_line_count() {
        // Empty block has no height; otherwise one cap plus one pitch per extra line.
        assert_eq!(data_block_h(0), 0.0);
        assert_eq!(data_block_h(1), DATA_LINE_CAP);
        assert!((data_block_h(6) - (DATA_LINE_CAP + DATA_LINE_PITCH * 5.0)).abs() < 0.01);
        // Fewer lines → a shorter block (e.g. minimal style's 5-line cell vs detailed's 6).
        assert!(data_block_h(5) < data_block_h(6));
        // Line counting: newlines + 1, empty string → 0.
        assert_eq!(line_count(""), 0);
        assert_eq!(line_count("one"), 1);
        assert_eq!(line_count("a\nb\nc"), 3);
    }

    #[test]
    fn minimal_data_block_is_shorter_than_detailed() {
        // The minimal-style FRAME STATS / DISPLAY cells drop their p99 line, so the tallest
        // cell is shorter and the data block hugs it more tightly than detailed.
        let s = sample_summary();
        let detailed_lines = [
            summary_text(&s, true),
            load_text(&s),
            stutter_text(&s),
            display_text(&s, true),
            audio_text(&s),
        ]
        .iter()
        .map(|t| line_count(t))
        .max()
        .unwrap();
        let minimal_lines = [
            summary_text(&s, false),
            load_text(&s),
            stutter_text(&s),
            display_text(&s, false),
            audio_text(&s),
        ]
        .iter()
        .map(|t| line_count(t))
        .max()
        .unwrap();
        assert!(
            minimal_lines < detailed_lines,
            "minimal {minimal_lines} should be < detailed {detailed_lines}"
        );
        assert!(data_block_h(minimal_lines) < data_block_h(detailed_lines));
    }

    #[test]
    fn stutter_text_reports_counts_and_worst() {
        let mut s = sample_summary();
        s.over_budget_count = 4;
        s.catch_up_count = 2;
        s.spike_hold_us = 33_200;
        let t = stutter_text(&s);
        assert!(t.contains("over-budget 4"), "{t}");
        assert!(t.contains("catch-ups 2"), "{t}");
        assert!(t.contains("worst 33.20ms"), "{t}");
    }

    #[test]
    fn load_text_reports_dominant_limiter() {
        // GPU wait dominates → gpu-bound, low idle.
        let mut s = sample_summary();
        s.avg_frame_us = 5_000;
        s.cpu_work_us = 1_000;
        s.gpu_wait_us = 3_500;
        let t = load_text(&s);
        assert!(t.contains("lim GPU"), "{t}");
        assert!(t.contains("cpu 1.00ms") && t.contains("gpu 3.50ms"), "{t}");
        // CPU work dominates → cpu-bound.
        s.cpu_work_us = 3_500;
        s.gpu_wait_us = 1_000;
        assert!(load_text(&s).contains("lim CPU"));
        // Lots of idle headroom → frame-capped, not bound by either.
        s.avg_frame_us = 16_667;
        s.cpu_work_us = 1_000;
        s.gpu_wait_us = 1_000;
        let t = load_text(&s);
        assert!(t.contains("lim none"), "{t}");
        assert!(t.contains("idle 88%"), "{t}");
    }

    #[test]
    fn minimal_layout_drops_histogram_and_shortens_panel() {
        let sw = 854.0;
        let sh = 480.0;
        let data_h = data_block_h(6);
        let widths: [f32; DATA_COLS] = [80.0, 48.0, 62.0, 62.0, 52.0];
        let detailed = compute_layout(OverlayAnchor::TopLeft, false, true, data_h, widths, sw, sh);
        let minimal = compute_layout(OverlayAnchor::TopLeft, false, false, data_h, widths, sw, sh);
        // Minimal style has no histogram band...
        assert_eq!(minimal.hist_h, 0.0);
        assert!(detailed.hist_h > 0.0);
        // ...so the data row slides up directly under the graph (shorter overall panel).
        assert!(minimal.data_y < detailed.data_y);
        assert!((minimal.data_y - (minimal.graph_y + GRAPH_H + DATA_GAP)).abs() < 0.01);
        // Bottom-anchored minimal panel is shorter, so its graph starts lower than detailed.
        let detailed_b = compute_layout(
            OverlayAnchor::BottomLeft,
            false,
            true,
            data_h,
            widths,
            sw,
            sh,
        );
        let minimal_b = compute_layout(
            OverlayAnchor::BottomLeft,
            false,
            false,
            data_h,
            widths,
            sw,
            sh,
        );
        assert!(minimal_b.graph_y > detailed_b.graph_y);
    }

    #[test]
    fn style_controls_p99_in_readout_text() {
        let s = sample_summary();
        // Detailed shows p99 in every readout; minimal omits it everywhere.
        assert!(summary_text(&s, true).contains("p99"));
        assert!(!summary_text(&s, false).contains("p99"));
        assert!(display_text(&s, true).contains("p99"));
        assert!(!display_text(&s, false).contains("p99"));
        assert!(compact_readout_text(&s, true).contains("p99"));
        assert!(!compact_readout_text(&s, false).contains("p99"));
        // Averages and spike-hold max survive in both styles.
        assert!(summary_text(&s, false).contains("avg"));
        assert!(summary_text(&s, false).contains("max"));
    }
}
