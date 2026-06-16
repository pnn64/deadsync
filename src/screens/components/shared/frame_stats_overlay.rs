use crate::act;
use deadsync_present::actors::Actor;

const DEBUG_OVERLAY_Z: i16 = 32030;

/// Number of bins in the frame-interval (jitter) histogram.
pub const HISTOGRAM_BINS: usize = 32;

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

// Streaming-statistics tuning. The decaying histogram keeps a stable p99 over a long
// effective window without storing a long ring; the EWMA pair tracks a smoothed mean and
// jitter (standard deviation). All allocation-free and only touched while the overlay runs.
/// Bin count for the decaying histogram (covers up to `DHIST_BINS * DHIST_BUCKET_US`).
pub const DHIST_BINS: usize = 256;
/// Histogram bin resolution in microseconds.
pub const DHIST_BUCKET_US: u32 = 200;
/// Per-frame decay applied to every bin. `1/(1-gamma)` ≈ effective sample count (~1024).
pub const DHIST_GAMMA: f32 = 1023.0 / 1024.0;
/// EWMA smoothing factor for the displayed mean frame time (half-life ≈ 34 frames).
pub const EWMA_ALPHA_MEAN: f32 = 0.02;
/// EWMA smoothing factor for the jitter (variance) estimate (slower than the mean).
pub const EWMA_ALPHA_VAR: f32 = 0.01;
/// Recompute the cached percentiles every Nth sample (≈3–6 Hz) to keep the text steady.
pub const STATS_REFRESH_PERIOD: u8 = 20;

/// Which screen corner (or center seam) the overlay anchors to. The engine recomputes a
/// sensible default the first time the overlay is shown; once the user moves it, the chosen
/// position is remembered (persisted to config) and restored on later toggles and restarts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
}

impl OverlayAnchor {
    /// Stable string key for persistence (config ini). Keep these values fixed.
    #[inline(always)]
    pub fn to_key(self) -> &'static str {
        match self {
            OverlayAnchor::TopLeft => "top-left",
            OverlayAnchor::TopRight => "top-right",
            OverlayAnchor::BottomLeft => "bottom-left",
            OverlayAnchor::BottomRight => "bottom-right",
            OverlayAnchor::TopCenter => "top-center",
            OverlayAnchor::BottomCenter => "bottom-center",
        }
    }

    /// Inverse of `to_key`; `None` for "auto"/empty/unknown (engine picks a play-context
    /// default until the user positions it). Case/whitespace tolerant.
    #[inline(always)]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "top-left" => Some(OverlayAnchor::TopLeft),
            "top-right" => Some(OverlayAnchor::TopRight),
            "bottom-left" => Some(OverlayAnchor::BottomLeft),
            "bottom-right" => Some(OverlayAnchor::BottomRight),
            "top-center" => Some(OverlayAnchor::TopCenter),
            "bottom-center" => Some(OverlayAnchor::BottomCenter),
            _ => None,
        }
    }
}

/// Presentation style for the overlay, toggleable at runtime so the two can be compared
/// side by side. The names describe how much is shown: `Detailed` is DeadSync's richer
/// readout (stable decaying-histogram p99 + the jitter histogram); `Minimal` strips both —
/// mirroring osu!framework, where the graph *is* the jitter display — and shows only the
/// smoothed averages, jitter, and a held worst spike.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayStyle {
    Detailed,
    Minimal,
}

impl OverlayStyle {
    /// Flip to the other style.
    #[inline(always)]
    pub fn toggle(self) -> Self {
        match self {
            OverlayStyle::Detailed => OverlayStyle::Minimal,
            OverlayStyle::Minimal => OverlayStyle::Detailed,
        }
    }

    /// Short label, also used as the persistence key in the config ini. Keep fixed.
    #[inline(always)]
    pub fn label(self) -> &'static str {
        match self {
            OverlayStyle::Detailed => "detailed",
            OverlayStyle::Minimal => "minimal",
        }
    }

    /// Parse a persisted style key; unknown/empty falls back to `Detailed`. Case tolerant.
    #[inline(always)]
    pub fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "minimal" => OverlayStyle::Minimal,
            _ => OverlayStyle::Detailed,
        }
    }

    /// Whether percentile (p99) readouts are shown in this style.
    #[inline(always)]
    fn show_p99(self) -> bool {
        matches!(self, OverlayStyle::Detailed)
    }

    /// Whether the jitter histogram panel is shown in this style (full mode only).
    #[inline(always)]
    fn show_histogram(self) -> bool {
        matches!(self, OverlayStyle::Detailed)
    }
}

/// Default anchor when the overlay is first switched on, before the user has positioned it:
/// the top-right corner. Once moved (Ctrl+Shift+F3) the chosen corner is remembered instead.
pub fn default_anchor() -> OverlayAnchor {
    OverlayAnchor::TopRight
}

/// Advance to the next anchor in a mode-specific cycle. Compact (2-player) mode includes
/// the top/bottom-center seams; full mode walks the four corners.
pub fn next_anchor(current: OverlayAnchor, compact: bool) -> OverlayAnchor {
    use OverlayAnchor::*;
    let order: &[OverlayAnchor] = if compact {
        &[BottomCenter, TopCenter, BottomLeft, BottomRight, TopLeft, TopRight]
    } else {
        &[TopLeft, TopRight, BottomRight, BottomLeft]
    };
    let idx = order.iter().position(|a| *a == current).unwrap_or(usize::MAX);
    order[(idx.wrapping_add(1)) % order.len()]
}

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
        let stack_after_graph = if show_histogram { HIST_GAP + HIST_H } else { 0.0 };
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

/// One captured frame's per-phase timing plus sync state. `Copy`, no heap.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSample {
    pub host_nanos: u64,
    pub frame_us: u32,
    pub input_us: u32,
    pub update_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub draw_us: u32,
    pub gpu_wait_us: u32,
    pub display_error_us: i32,
    pub catching_up: bool,
}

impl FrameStatsSample {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            host_nanos: 0,
            frame_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            gpu_wait_us: 0,
            display_error_us: 0,
            catching_up: false,
        }
    }

    #[inline(always)]
    const fn is_empty(&self) -> bool {
        self.host_nanos == 0
    }

    /// Sum of the explicitly measured phases (everything that isn't idle headroom).
    #[inline(always)]
    const fn measured_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
            .saturating_add(self.gpu_wait_us)
    }

    /// Idle / "sleep" headroom: the slice of the frame interval not spent in a measured
    /// phase. Mirrors osu!'s Sleep segment so each column fills the full frame interval.
    #[inline(always)]
    const fn idle_us(&self) -> u32 {
        self.frame_us.saturating_sub(self.measured_us())
    }

    /// CPU work this frame: every measured phase except the GPU/swapchain wait. This is the
    /// real CPU cost (input → draw); `gpu_wait_us` is tracked separately as the GPU side.
    #[inline(always)]
    const fn cpu_work_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
    }
}

/// Precomputed sync-health readouts supplied by the caller (engine-side snapshots).
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSummary {
    pub avg_frame_us: u32,
    pub p99_frame_us: u32,
    pub max_frame_us: u32,
    pub fps: f32,
    pub display_error_ms: f32,
    pub display_error_p99_ms: f32,
    pub display_catching_up: bool,
    pub in_gameplay: bool,
    pub audio_callback_gap_ms: f32,
    pub audio_underruns: u64,
    pub audio_output_delay_ms: f32,
    pub audio_queued_frames: u32,
    /// Smoothed jitter (EWMA standard deviation) of the frame interval, in microseconds.
    pub frame_jitter_us: u32,
    /// Smoothed jitter of the display-clock error, in microseconds.
    pub display_error_jitter_us: u32,
    /// Slow-decay "worst recent frame" hold, in microseconds (osu! spike marker analog).
    pub spike_hold_us: u32,
    /// Monitor target frame time for the graph reference lines, in microseconds (0 = none).
    pub target_frame_us: u32,
    /// Smoothed CPU work per frame (input → draw, excluding GPU wait), in microseconds.
    pub cpu_work_us: u32,
    /// Smoothed GPU/swapchain wait per frame, in microseconds.
    pub gpu_wait_us: u32,
    /// Frames in the recent ring window that exceeded the stutter threshold (the 2× target
    /// reference line) — a count of visible hitches.
    pub over_budget_count: u32,
    /// Distinct display-clock catch-up events (rising edges) in the recent ring window.
    pub catch_up_count: u32,
}

/// Exponentially-decaying bucketed histogram. Each `update` decays every bin by `gamma`
/// and adds 1.0 to the sample's bin, so old frames fade and the effective window is
/// `~1/(1-gamma)` samples. Percentiles are read by walking the weighted bins — no sort,
/// no heap, `Copy`. This is what gives a *stable* p99 (unlike a short sliding window,
/// which sawtooths as single outliers enter and leave it).
#[derive(Clone, Copy)]
pub struct DecayingHist {
    bins: [f32; DHIST_BINS],
    total: f32,
}

impl DecayingHist {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            bins: [0.0; DHIST_BINS],
            total: 0.0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.bins = [0.0; DHIST_BINS];
        self.total = 0.0;
    }

    /// Decay all bins and add the new `value_us` to its bucket.
    #[inline]
    pub fn update(&mut self, value_us: u32, gamma: f32, bucket_us: u32) {
        let bucket_us = bucket_us.max(1);
        let idx = (value_us / bucket_us).min(DHIST_BINS as u32 - 1) as usize;
        for b in self.bins.iter_mut() {
            *b *= gamma;
        }
        self.bins[idx] += 1.0;
        self.total = self.total * gamma + 1.0;
    }

    /// Weighted percentile in microseconds (bucket-quantized). Returns 0 until warmed up.
    #[inline]
    pub fn percentile_us(&self, pct: f32, bucket_us: u32) -> u32 {
        if self.total <= 0.0 {
            return 0;
        }
        let bucket_us = bucket_us.max(1);
        let target = (self.total * pct.clamp(0.0, 1.0)).max(f32::MIN_POSITIVE);
        let mut cumulative = 0.0f32;
        for (idx, &b) in self.bins.iter().enumerate() {
            cumulative += b;
            if cumulative >= target {
                return (idx as u32 + 1) * bucket_us;
            }
        }
        (DHIST_BINS as u32) * bucket_us
    }

    /// Effective sample count currently represented (≈ `1/(1-gamma)` once warmed up).
    #[inline(always)]
    pub fn effective_n(&self) -> u32 {
        self.total.round().max(0.0) as u32
    }
}

impl Default for DecayingHist {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponentially-weighted mean and variance (West's incremental EWMA). Tracks a smoothed
/// average and a jitter (standard deviation) estimate without storing samples. `Copy`.
#[derive(Clone, Copy)]
pub struct EwmaStats {
    mean: f32,
    var: f32,
    count: u32,
}

impl EwmaStats {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            mean: 0.0,
            var: 0.0,
            count: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.mean = 0.0;
        self.var = 0.0;
        self.count = 0;
    }

    #[inline]
    pub fn update(&mut self, value: f32, alpha_mean: f32, alpha_var: f32) {
        if self.count == 0 {
            self.mean = value;
            self.var = 0.0;
        } else {
            let delta = value - self.mean;
            self.mean += alpha_mean * delta;
            self.var = (1.0 - alpha_var) * (self.var + alpha_var * delta * delta);
        }
        self.count = self.count.saturating_add(1);
    }

    #[inline(always)]
    pub fn mean(&self) -> f32 {
        self.mean
    }

    #[inline(always)]
    pub fn std_dev(&self) -> f32 {
        self.var.max(0.0).sqrt()
    }
}

impl Default for EwmaStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Long-window streaming statistics for the overlay: decaying histograms (frame time and
/// display-clock error) for stable percentiles, plus EWMA mean/jitter for both signals.
/// Percentiles are cached and refreshed at a low cadence so the readout text stays steady.
/// All inline arrays — `Copy`, no heap — and only fed while the overlay is enabled.
#[derive(Clone, Copy)]
pub struct FrameStatsLong {
    frame_hist: DecayingHist,
    error_hist: DecayingHist,
    frame_ewma: EwmaStats,
    error_ewma: EwmaStats,
    cpu_ewma: EwmaStats,
    gpu_ewma: EwmaStats,
    cached_p99_frame_us: u32,
    cached_p99_error_us: u32,
    refresh_counter: u8,
}

impl FrameStatsLong {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            frame_hist: DecayingHist::new(),
            error_hist: DecayingHist::new(),
            frame_ewma: EwmaStats::new(),
            error_ewma: EwmaStats::new(),
            cpu_ewma: EwmaStats::new(),
            gpu_ewma: EwmaStats::new(),
            cached_p99_frame_us: 0,
            cached_p99_error_us: 0,
            refresh_counter: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.frame_hist.reset();
        self.error_hist.reset();
        self.frame_ewma.reset();
        self.error_ewma.reset();
        self.cpu_ewma.reset();
        self.gpu_ewma.reset();
        self.cached_p99_frame_us = 0;
        self.cached_p99_error_us = 0;
        self.refresh_counter = 0;
    }

    /// Feed one captured frame (hot path while the overlay is on). Refreshes the cached
    /// percentiles every `STATS_REFRESH_PERIOD` calls.
    #[inline]
    pub fn push(&mut self, sample: &FrameStatsSample) {
        let abs_err = sample.display_error_us.unsigned_abs();
        self.frame_hist
            .update(sample.frame_us, DHIST_GAMMA, DHIST_BUCKET_US);
        self.error_hist
            .update(abs_err, DHIST_GAMMA, DHIST_BUCKET_US);
        self.frame_ewma
            .update(sample.frame_us as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.error_ewma
            .update(abs_err as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.cpu_ewma
            .update(sample.cpu_work_us() as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.gpu_ewma
            .update(sample.gpu_wait_us as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);

        self.refresh_counter = self.refresh_counter.wrapping_add(1);
        if self.refresh_counter >= STATS_REFRESH_PERIOD {
            self.refresh_counter = 0;
            self.cached_p99_frame_us = self.frame_hist.percentile_us(0.99, DHIST_BUCKET_US);
            self.cached_p99_error_us = self.error_hist.percentile_us(0.99, DHIST_BUCKET_US);
        }
    }

    #[inline(always)]
    pub fn p99_frame_us(&self) -> u32 {
        self.cached_p99_frame_us
    }

    #[inline(always)]
    pub fn p99_error_us(&self) -> u32 {
        self.cached_p99_error_us
    }

    #[inline(always)]
    pub fn avg_frame_us(&self) -> u32 {
        self.frame_ewma.mean().max(0.0).round() as u32
    }

    /// Smoothed CPU work per frame (input → draw, excluding GPU wait), in microseconds.
    #[inline(always)]
    pub fn avg_cpu_us(&self) -> u32 {
        self.cpu_ewma.mean().max(0.0).round() as u32
    }

    /// Smoothed GPU/swapchain wait per frame, in microseconds.
    #[inline(always)]
    pub fn avg_gpu_us(&self) -> u32 {
        self.gpu_ewma.mean().max(0.0).round() as u32
    }

    #[inline(always)]
    pub fn frame_jitter_us(&self) -> u32 {
        self.frame_ewma.std_dev().round() as u32
    }

    #[inline(always)]
    pub fn error_jitter_us(&self) -> u32 {
        self.error_ewma.std_dev().round() as u32
    }

    #[inline(always)]
    pub fn effective_n(&self) -> u32 {
        self.frame_hist.effective_n()
    }
}

impl Default for FrameStatsLong {
    fn default() -> Self {
        Self::new()
    }
}

/// Single-pass bucketed percentile of `frame_us` across `samples`, in microseconds.
/// Avoids sorting/allocation; resolution is `bucket_us`.
pub fn percentile_us(samples: &[FrameStatsSample], pct: f32, bucket_us: u32) -> u32 {
    const BUCKETS: usize = 256;
    let bucket_us = bucket_us.max(1);
    let mut hist = [0u32; BUCKETS];
    let mut count: u32 = 0;
    for s in samples {
        if s.is_empty() {
            continue;
        }
        let idx = (s.frame_us / bucket_us).min(BUCKETS as u32 - 1) as usize;
        hist[idx] = hist[idx].saturating_add(1);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return 0;
    }
    let target = (f64::from(count) * f64::from(pct.clamp(0.0, 1.0))).ceil() as u32;
    let target = target.max(1);
    let mut cumulative: u32 = 0;
    for (idx, bin) in hist.iter().enumerate() {
        cumulative = cumulative.saturating_add(*bin);
        if cumulative >= target {
            return (idx as u32 + 1) * bucket_us;
        }
    }
    (BUCKETS as u32) * bucket_us
}

/// Fill `out` with a frame-interval histogram. Bin `i` counts frames whose interval falls
/// in `[i*bin_width_us, (i+1)*bin_width_us)`; the final bin absorbs the overflow.
pub fn histogram(samples: &[FrameStatsSample], out: &mut [u32; HISTOGRAM_BINS], bin_width_us: u32) {
    *out = [0; HISTOGRAM_BINS];
    let bin_width_us = bin_width_us.max(1);
    for s in samples {
        if s.is_empty() {
            continue;
        }
        let idx = (s.frame_us / bin_width_us).min(HISTOGRAM_BINS as u32 - 1) as usize;
        out[idx] = out[idx].saturating_add(1);
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
    actors.push(quad(x, top, col_w, h, [rgb[0], rgb[1], rgb[2], 0.95], DEBUG_OVERLAY_Z + 1));
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
        segment(actors, x, baseline, &mut drawn, s.idle_us(), px_per_us, col_w, COLOR_IDLE);
        segment(actors, x, baseline, &mut drawn, s.input_us, px_per_us, col_w, COLOR_INPUT);
        segment(actors, x, baseline, &mut drawn, s.update_us, px_per_us, col_w, COLOR_UPDATE);
        segment(actors, x, baseline, &mut drawn, s.compose_us, px_per_us, col_w, COLOR_COMPOSE);
        segment(actors, x, baseline, &mut drawn, s.upload_us, px_per_us, col_w, COLOR_UPLOAD);
        segment(actors, x, baseline, &mut drawn, s.draw_us, px_per_us, col_w, COLOR_DRAW);
        segment(actors, x, baseline, &mut drawn, s.gpu_wait_us, px_per_us, col_w, COLOR_GPU_WAIT);

        // Event markers (osu! GC-marker analog): a full-height vertical line on frames
        // where the display clock is actively catching up, or that spiked well past scale.
        if s.catching_up {
            actors.push(quad(x, gy, col_w.max(1.0), gh, COLOR_MARKER_CATCHUP, DEBUG_OVERLAY_Z + 2));
        } else if s.frame_us >= spike_us {
            actors.push(quad(x, gy, col_w.max(1.0), gh, COLOR_MARKER_SPIKE, DEBUG_OVERLAY_Z + 2));
        }
    }

    // osu!-style fixed reference lines: the monitor's target frame time and twice it.
    // Drawn last so they sit above the bars and markers; skipped if off the graph.
    ref_line(actors, target_frame_us, scale_us, gx, baseline, gw, gh, COLOR_REF_TARGET);
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
        actors.push(quad(x, baseline - h, col_w - 0.5, h, COLOR_HIST, DEBUG_OVERLAY_Z + 1));
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
        let _ = write!(text, "DISPLAY CLOCK\nerr {:+.2}ms", summary.display_error_ms);
        if show_p99 {
            let _ = write!(text, "\np99 {:.2}ms", summary.display_error_p99_ms);
        }
        let _ = write!(
            text,
            "\njit {:.2}ms\ncatch-up {}",
            ms(summary.display_error_jitter_us),
            if summary.display_catching_up { "YES" } else { "no" },
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
            if summary.display_catching_up { "YES" } else { "no" },
            summary.audio_underruns,
            summary.audio_output_delay_ms,
        );
    } else {
        let _ = write!(
            text,
            "\nunderruns {}  out {:.1} ms  gap {:.2} ms",
            summary.audio_underruns,
            summary.audio_output_delay_ms,
            summary.audio_callback_gap_ms,
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
    let scale_us = summary
        .max_frame_us
        .max(summary.spike_hold_us)
        .max(20_000);
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

    fn sample(frame_us: u32) -> FrameStatsSample {
        FrameStatsSample {
            host_nanos: 1,
            frame_us,
            ..FrameStatsSample::empty()
        }
    }

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
    fn percentile_ignores_empty_and_picks_high_bucket() {
        let mut samples = vec![FrameStatsSample::empty(); 4];
        samples.push(sample(16_000));
        samples.push(sample(16_000));
        samples.push(sample(16_000));
        samples.push(sample(50_000));
        // p99 should land in the bucket covering the 50ms spike.
        let p99 = percentile_us(&samples, 0.99, 1_000);
        assert!(p99 >= 50_000, "p99 was {p99}");
        // p50 should sit at the common 16ms frames.
        let p50 = percentile_us(&samples, 0.5, 1_000);
        assert!((16_000..=17_000).contains(&p50), "p50 was {p50}");
    }

    #[test]
    fn percentile_empty_is_zero() {
        let samples = [FrameStatsSample::empty(); 3];
        assert_eq!(percentile_us(&samples, 0.99, 1_000), 0);
    }

    #[test]
    fn histogram_buckets_and_overflow() {
        let mut bins = [0u32; HISTOGRAM_BINS];
        let samples = [sample(0), sample(1_000), sample(1_500), sample(10_000_000)];
        histogram(&samples, &mut bins, 1_000);
        assert_eq!(bins[0], 1); // 0us
        assert_eq!(bins[1], 2); // 1000us and 1500us both land in bin 1
        assert_eq!(bins[HISTOGRAM_BINS - 1], 1); // huge value clamps to last bin
    }

    #[test]
    fn idle_is_frame_minus_measured() {
        let s = FrameStatsSample {
            host_nanos: 1,
            frame_us: 16_000,
            input_us: 1_000,
            update_us: 2_000,
            compose_us: 1_000,
            upload_us: 500,
            draw_us: 3_000,
            gpu_wait_us: 1_500,
            ..FrameStatsSample::empty()
        };
        assert_eq!(s.measured_us(), 9_000);
        assert_eq!(s.idle_us(), 7_000);
    }

    #[test]
    fn default_anchor_is_top_right() {
        // The overlay opens in the top-right corner until the user moves it.
        assert_eq!(default_anchor(), OverlayAnchor::TopRight);
    }

    #[test]
    fn next_anchor_cycles_and_wraps() {
        // Full-mode cycle walks the four corners and wraps.
        assert_eq!(next_anchor(OverlayAnchor::TopLeft, false), OverlayAnchor::TopRight);
        assert_eq!(next_anchor(OverlayAnchor::TopRight, false), OverlayAnchor::BottomRight);
        assert_eq!(next_anchor(OverlayAnchor::BottomRight, false), OverlayAnchor::BottomLeft);
        assert_eq!(next_anchor(OverlayAnchor::BottomLeft, false), OverlayAnchor::TopLeft);
        // Compact-mode cycle starts at the seams.
        assert_eq!(next_anchor(OverlayAnchor::BottomCenter, true), OverlayAnchor::TopCenter);
        // An anchor outside the active list falls back to the first entry.
        assert_eq!(next_anchor(OverlayAnchor::TopCenter, false), OverlayAnchor::TopLeft);
    }

    #[test]
    fn decaying_hist_percentile_tracks_distribution() {
        let mut h = DecayingHist::new();
        // Feed a steady 16ms stream with an occasional 50ms spike (~2%).
        for i in 0..2000 {
            let v = if i % 50 == 0 { 50_000 } else { 16_000 };
            h.update(v, DHIST_GAMMA, DHIST_BUCKET_US);
        }
        let p50 = h.percentile_us(0.50, DHIST_BUCKET_US);
        let p99 = h.percentile_us(0.99, DHIST_BUCKET_US);
        // Median sits on the common 16ms frames; p99 reaches the 50ms spike bucket.
        assert!((16_000..=16_200).contains(&p50), "p50 was {p50}");
        assert!(p99 >= 49_800, "p99 was {p99}");
        // Effective window is bounded near 1/(1-gamma) (~1024), not the 2000 fed.
        assert!(h.effective_n() <= 1100, "effective_n was {}", h.effective_n());
    }

    #[test]
    fn decaying_hist_is_stable_against_single_outlier() {
        // A short sliding-window p99 sawtooths when one outlier enters/leaves; the decaying
        // histogram should barely move when a lone spike ages out of relevance.
        let mut h = DecayingHist::new();
        for _ in 0..2000 {
            h.update(16_000, DHIST_GAMMA, DHIST_BUCKET_US);
        }
        let before = h.percentile_us(0.99, DHIST_BUCKET_US);
        h.update(80_000, DHIST_GAMMA, DHIST_BUCKET_US);
        for _ in 0..200 {
            h.update(16_000, DHIST_GAMMA, DHIST_BUCKET_US);
        }
        let after = h.percentile_us(0.99, DHIST_BUCKET_US);
        // One spike in ~1000 effective samples stays under the 99th percentile → no jump.
        assert_eq!(before, after, "p99 moved {before}->{after} on a single outlier");
    }

    #[test]
    fn ewma_converges_to_mean_and_reports_jitter() {
        let mut e = EwmaStats::new();
        for _ in 0..1000 {
            e.update(16_000.0, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        }
        assert!((e.mean() - 16_000.0).abs() < 1.0, "mean was {}", e.mean());
        assert!(e.std_dev() < 50.0, "constant input should have ~0 jitter");

        let mut j = EwmaStats::new();
        for i in 0..4000 {
            let v = if i % 2 == 0 { 14_000.0 } else { 18_000.0 };
            j.update(v, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        }
        // Alternating ±2ms around 16ms → mean ~16ms, non-trivial jitter.
        assert!((j.mean() - 16_000.0).abs() < 500.0, "mean was {}", j.mean());
        assert!(j.std_dev() > 500.0, "jitter was {}", j.std_dev());
    }

    #[test]
    fn frame_stats_long_caches_percentiles_at_cadence() {
        let mut long = FrameStatsLong::new();
        let mut s = sample(16_000);
        // Below the refresh period the cache stays cold (0); it populates once it ticks over.
        for _ in 0..(STATS_REFRESH_PERIOD - 1) {
            long.push(&s);
        }
        assert_eq!(long.p99_frame_us(), 0, "cache should be cold before first refresh");
        long.push(&s);
        assert!(long.p99_frame_us() > 0, "cache should populate at the refresh cadence");

        s.display_error_us = 1_200;
        for _ in 0..STATS_REFRESH_PERIOD {
            long.push(&s);
        }
        assert!(long.p99_error_us() > 0, "error percentile should populate too");
        long.reset();
        assert_eq!(long.p99_frame_us(), 0);
        assert_eq!(long.effective_n(), 0);
    }

    #[test]
    fn ref_line_clips_off_scale_values() {
        let mut actors = Vec::new();
        // Target above the graph scale draws nothing; on-scale draws one line quad.
        ref_line(&mut actors, 40_000, 20_000, 0.0, 100.0, 200.0, 100.0, COLOR_REF_TARGET);
        assert!(actors.is_empty(), "off-scale reference line should be skipped");
        ref_line(&mut actors, 16_700, 33_400, 0.0, 100.0, 200.0, 100.0, COLOR_REF_TARGET);
        assert_eq!(actors.len(), 1, "on-scale reference line should draw one quad");
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
        assert!(l.graph_w <= sw * 0.5 + 0.01, "graph_w {} should be <= half screen", l.graph_w);
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
        let b = compute_layout(OverlayAnchor::BottomLeft, false, true, data_h, widths, sw, sh);
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
    fn anchor_and_style_keys_round_trip() {
        use OverlayAnchor::*;
        for a in [TopLeft, TopRight, BottomLeft, BottomRight, TopCenter, BottomCenter] {
            assert_eq!(OverlayAnchor::from_key(a.to_key()), Some(a));
        }
        // "auto"/empty/unknown map to None so the engine falls back to the default.
        assert_eq!(OverlayAnchor::from_key("auto"), None);
        assert_eq!(OverlayAnchor::from_key(""), None);
        assert_eq!(OverlayAnchor::from_key("nonsense"), None);
        // Keys are case/whitespace tolerant.
        assert_eq!(OverlayAnchor::from_key("  TOP-LEFT "), Some(TopLeft));
        // Styles round-trip; unknown/empty falls back to detailed.
        assert_eq!(OverlayStyle::from_key(OverlayStyle::Detailed.label()), OverlayStyle::Detailed);
        assert_eq!(OverlayStyle::from_key(OverlayStyle::Minimal.label()), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("MINIMAL"), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("???"), OverlayStyle::Detailed);
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
        let detailed_b = compute_layout(OverlayAnchor::BottomLeft, false, true, data_h, widths, sw, sh);
        let minimal_b = compute_layout(OverlayAnchor::BottomLeft, false, false, data_h, widths, sw, sh);
        assert!(minimal_b.graph_y > detailed_b.graph_y);
    }

    #[test]
    fn overlay_style_toggle_round_trips() {
        assert_eq!(OverlayStyle::Detailed.toggle(), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::Minimal.toggle(), OverlayStyle::Detailed);
        assert_eq!(OverlayStyle::Detailed.toggle().toggle(), OverlayStyle::Detailed);
        assert!(OverlayStyle::Detailed.show_p99());
        assert!(!OverlayStyle::Minimal.show_p99());
        assert!(OverlayStyle::Detailed.show_histogram());
        assert!(!OverlayStyle::Minimal.show_histogram());
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
