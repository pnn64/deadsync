use std::sync::Arc;

use crate::act;
use crate::assets::{FontRole, machine_font_key_for_text};
use crate::config::MachineFont;
use crate::screens::components::evaluation::eval_graphs::TimingHistogramScale;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadlib_render_core::{BlendMode, MeshVertex};
use deadsync_profile as profile_data;
use deadsync_rules::timing;
use std::cell::RefCell;

use super::utils::{eval_style_alpha, pane_origin_x};

#[derive(Clone, Copy)]
struct TimingBand {
    label: &'static str,
    start_ms: f32,
    end_ms: f32,
    color: [f32; 4],
}

const EMPTY_BAND: TimingBand = TimingBand {
    label: "",
    start_ms: 0.0,
    end_ms: 0.0,
    color: [0.0, 0.0, 0.0, 0.0],
};

#[derive(Clone, Copy, PartialEq)]
struct TimingPaneCacheKey {
    worst_window_bits: u32,
    timing_window_bits: [u32; 5],
    mesh_address: usize,
    mesh_len: usize,
    controller: profile_data::PlayerSide,
    scale: TimingHistogramScale,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
}

type CachedTimingPane = Option<(TimingPaneCacheKey, Arc<[Actor]>)>;

#[derive(Clone)]
pub(crate) struct TimingPaneText {
    values: [TextContent; 4],
    cached: RefCell<[CachedTimingPane; 3]>,
}

impl TimingPaneText {
    pub(crate) fn new(score: &ScoreInfo) -> Self {
        Self::from_timing(score.timing)
    }

    fn from_timing(stats: timing::TimingStats) -> Self {
        Self {
            values: [
                super::retained_text(format_args!("{:.2}ms", stats.mean_abs_ms)),
                super::retained_text(format_args!("{:.2}ms", stats.mean_ms)),
                super::retained_text(format_args!("{:.2}ms", stats.stddev_ms * 3.0)),
                super::retained_text(format_args!("{:.2}ms", stats.max_abs_ms)),
            ],
            cached: RefCell::new(std::array::from_fn(|_| None)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cached_actors(
        &self,
        worst_window_ms: f32,
        timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
        controller: profile_data::PlayerSide,
        scale: TimingHistogramScale,
        transparent: bool,
        machine_font: MachineFont,
        palette: JudgmentPalette,
    ) -> Arc<[Actor]> {
        let timing_windows = timing::effective_windows_ms();
        let key = TimingPaneCacheKey {
            worst_window_bits: worst_window_ms.to_bits(),
            timing_window_bits: timing_windows.map(f32::to_bits),
            mesh_address: timing_hist_mesh.map_or(0, |mesh| mesh.as_ptr() as usize),
            mesh_len: timing_hist_mesh.map_or(0, |mesh| mesh.len()),
            controller,
            scale,
            transparent,
            machine_font,
            palette,
        };
        let index = timing_scale_index(scale);
        if let Some((_, actors)) = self.cached.borrow()[index]
            .as_ref()
            .filter(|(cached, _)| *cached == key)
        {
            return Arc::clone(actors);
        }

        let actors: Arc<[Actor]> = Arc::from([timing_pane_actor(
            worst_window_ms,
            timing_windows,
            self,
            timing_hist_mesh,
            controller,
            scale,
            transparent,
            machine_font,
            palette,
            27,
        )]);
        self.cached.borrow_mut()[index] = Some((key, Arc::clone(&actors)));
        actors
    }
}

#[inline(always)]
const fn timing_scale_index(scale: TimingHistogramScale) -> usize {
    match scale {
        TimingHistogramScale::Itg => 0,
        TimingHistogramScale::Ex => 1,
        TimingHistogramScale::HardEx => 2,
    }
}

#[inline(always)]
const fn band(label: &'static str, start_ms: f32, end_ms: f32, color: [f32; 4]) -> TimingBand {
    TimingBand {
        label,
        start_ms,
        end_ms,
        color,
    }
}

#[inline(always)]
const fn timing_bands_itg(
    timing_windows: [f32; 5],
    palette: JudgmentPalette,
) -> ([TimingBand; 7], usize) {
    let blue = palette.color(Role::FantasticBlue);
    let excellent = palette.color(Role::Excellent);
    let great = palette.color(Role::Great);
    let decent = palette.color(Role::Decent);
    let wayoff = palette.color(Role::WayOff);
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w1, blue),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
            EMPTY_BAND,
            EMPTY_BAND,
        ],
        5,
    )
}

#[inline(always)]
const fn timing_bands_ex(
    timing_windows: [f32; 5],
    palette: JudgmentPalette,
) -> ([TimingBand; 7], usize) {
    let blue = palette.color(Role::FantasticBlue);
    let excellent = palette.color(Role::Excellent);
    let great = palette.color(Role::Great);
    let decent = palette.color(Role::Decent);
    let wayoff = palette.color(Role::WayOff);
    let white = palette.color(Role::FantasticWhite);
    let w0 = timing::FA_PLUS_W0_MS;
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w0, blue),
            band("Fan", w0, w1, white),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
            EMPTY_BAND,
        ],
        6,
    )
}

#[inline(always)]
const fn timing_bands_hard_ex(
    timing_windows: [f32; 5],
    palette: JudgmentPalette,
) -> ([TimingBand; 7], usize) {
    let pink = color::HARD_EX_SCORE_RGBA;
    let blue = palette.color(Role::FantasticBlue);
    let excellent = palette.color(Role::Excellent);
    let great = palette.color(Role::Great);
    let decent = palette.color(Role::Decent);
    let wayoff = palette.color(Role::WayOff);
    let white = palette.color(Role::FantasticWhite);
    let w010 = timing::FA_PLUS_W010_MS;
    let w0 = timing::FA_PLUS_W0_MS;
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w010, pink),
            band("Fan", w010, w0, blue),
            band("Fan", w0, w1, white),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
        ],
        7,
    )
}

#[inline(always)]
const fn timing_bands_ms(
    scale: TimingHistogramScale,
    timing_windows: [f32; 5],
    palette: JudgmentPalette,
) -> ([TimingBand; 7], usize) {
    match scale {
        TimingHistogramScale::Itg => timing_bands_itg(timing_windows, palette),
        TimingHistogramScale::Ex => timing_bands_ex(timing_windows, palette),
        TimingHistogramScale::HardEx => timing_bands_hard_ex(timing_windows, palette),
    }
}

#[allow(clippy::too_many_arguments)]
fn timing_pane_actor(
    worst_window_ms: f32,
    timing_windows: [f32; 5],
    text: &TimingPaneText,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile_data::PlayerSide,
    scale: TimingHistogramScale,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
    child_capacity: usize,
) -> Actor {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;
    let topbar_height: f32 = 26.0;
    let bottombar_height: f32 = 13.0;

    let pane_origin_x = pane_origin_x(controller);
    let frame_x = pane_width.mul_add(-0.5, pane_origin_x);
    let frame_y = deadlib_present::space::screen_center_y() - 56.0;

    let mut children = Vec::with_capacity(child_capacity);
    const BAR_BG_COLOR: [f32; 4] = color::rgba_hex("#101519");
    let topbar_alpha = eval_style_alpha(transparent, 1.0, 0.5);
    let early_alpha = eval_style_alpha(transparent, 1.0, 0.5);

    // Top and Bottom bars
    children.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        setsize(pane_width, topbar_height):
        diffuse(BAR_BG_COLOR[0], BAR_BG_COLOR[1], BAR_BG_COLOR[2], topbar_alpha)
    ));
    children.push(act!(quad:
        align(0.0, 1.0): xy(0.0, pane_height):
        setsize(pane_width, bottombar_height):
        diffuse(BAR_BG_COLOR[0], BAR_BG_COLOR[1], BAR_BG_COLOR[2], 1.0)
    ));

    // Center line of graph area
    children.push(act!(quad:
        align(0.5, 0.0): xy(pane_width / 2.0_f32, topbar_height):
        setsize(1.0, pane_height - topbar_height - bottombar_height):
        diffuse(1.0, 1.0, 1.0, 0.666)
    ));

    // Early/Late text
    let early_late_y = topbar_height + 11.0;
    children.push(act!(text: font(machine_font_key_for_text(machine_font, FontRole::Header, "Early")): settext("Early"):
        align(0.0, 0.0): xy(10.0, early_late_y):
        zoom(0.3):
        diffusealpha(early_alpha)
    ));
    children.push(act!(text: font(machine_font_key_for_text(machine_font, FontRole::Header, "Late")): settext("Late"):
        align(1.0, 0.0): xy(pane_width - 10.0, early_late_y):
        zoom(0.3): horizalign(right)
    ));

    // Bottom bar judgment labels
    let bottom_bar_center_y = pane_height - (bottombar_height / 2.0_f32);
    let (judgment_bands, band_count) = timing_bands_ms(scale, timing_windows, palette);
    let legend_span_ms = super::eval_graphs::timing_display_window_ms(worst_window_ms, scale);

    for (i, band) in judgment_bands.iter().take(band_count).enumerate() {
        if band.start_ms >= legend_span_ms {
            continue;
        }
        let clamped_end_ms = band.end_ms.min(legend_span_ms);
        if clamped_end_ms <= band.start_ms {
            continue;
        }
        let mid_point_ms = f32::midpoint(band.start_ms, clamped_end_ms);

        // Scale position from ms to pane coordinates
        let x_offset = (mid_point_ms / legend_span_ms) * (pane_width / 2.0_f32);

        if i == 0 {
            // "Fan" is centered
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
        } else {
            // Others are symmetric
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 - x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 + x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
        }
    }

    // Histogram (aggregate timing offsets) — Simply Love uses an ActorMultiVertex (QuadStrip).
    if let Some(mesh) = timing_hist_mesh
        && !mesh.is_empty()
    {
        let graph_area_height = (pane_height - topbar_height - bottombar_height).max(0.0);
        children.push(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, topbar_height],
            size: [SizeSpec::Px(pane_width), SizeSpec::Px(graph_area_height)],
            tint: [1.0; 4],
            vertices: mesh.clone(),
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        });
    }

    // Top bar stats
    let top_label_y = 2.0;
    let top_value_y = 13.0;
    let label_zoom = 0.575;
    let value_zoom = 0.8;

    let labels_and_values = [
        ("mean abs error", 40.0, &text.values[0]),
        (
            "mean",
            40.0 + (pane_width - 80.0_f32) / 3.0_f32,
            &text.values[1],
        ),
        (
            "std dev * 3",
            ((pane_width - 80.0_f32) / 3.0_f32).mul_add(2.0_f32, 40.0),
            &text.values[2],
        ),
        ("max error", pane_width - 40.0, &text.values[3]),
    ];

    for (label, x, value) in labels_and_values {
        children.push(act!(text: font("miso"): settext(label):
            align(0.5, 0.0): xy(x, top_label_y):
            zoom(label_zoom)
        ));
        children.push(act!(text: font("miso"): settext(value.clone()):
            align(0.5, 0.0): xy(x, top_value_y):
            zoom(value_zoom)
        ));
    }

    Actor::Frame {
        align: [0.0, 0.0],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(pane_width), SizeSpec::Px(pane_height)],
        children,
        background: None,
        z: 101,
    }
}

/// Appends the aggregate timing pane without staging its outer actor list.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_timing_pane_with_palette(
    out: &mut Vec<Actor>,
    score_info: &ScoreInfo,
    text: &TimingPaneText,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile_data::PlayerSide,
    scale: TimingHistogramScale,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) {
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: text.cached_actors(
            score_info.histogram.worst_window_ms,
            timing_hist_mesh,
            controller,
            scale,
            transparent,
            machine_font,
            palette,
        ),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::too_many_arguments)]
fn build_timing_pane_legacy(
    worst_window_ms: f32,
    text: &TimingPaneText,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile_data::PlayerSide,
    scale: TimingHistogramScale,
    transparent: bool,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) -> Vec<Actor> {
    vec![timing_pane_actor(
        worst_window_ms,
        timing::effective_windows_ms(),
        text,
        timing_hist_mesh,
        controller,
        scale,
        transparent,
        machine_font,
        palette,
        0,
    )]
}

/// Stable old/new fixture for aggregate timing-pane allocation churn.
#[cfg(any(test, feature = "bench-support"))]
pub struct TimingPaneAppendBenchmark {
    text: TimingPaneText,
}

#[cfg(any(test, feature = "bench-support"))]
impl TimingPaneAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let fixture = Self {
            text: TimingPaneText::from_timing(timing::TimingStats {
                mean_abs_ms: 12.345,
                mean_ms: -3.5,
                stddev_ms: 2.25,
                max_abs_ms: 180.0,
            }),
        };
        let _ = fixture.text.cached_actors(
            180.0,
            None,
            profile_data::PlayerSide::P1,
            TimingHistogramScale::HardEx,
            false,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        fixture
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_timing_pane_legacy(
            180.0,
            &self.text,
            None,
            profile_data::PlayerSide::P1,
            TimingHistogramScale::HardEx,
            false,
            MachineFont::Mega,
            JudgmentPalette::default(),
        ));
        std::hint::black_box(&*out);
        actor_tree_count(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.push(timing_pane_actor(
            180.0,
            timing::effective_windows_ms(),
            &self.text,
            None,
            profile_data::PlayerSide::P1,
            TimingHistogramScale::HardEx,
            false,
            MachineFont::Mega,
            JudgmentPalette::default(),
            27,
        ));
        std::hint::black_box(&*out);
        semantic_actor_tree_count(out)
    }

    #[must_use]
    pub fn retained_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: self.text.cached_actors(
                180.0,
                None,
                profile_data::PlayerSide::P1,
                TimingHistogramScale::HardEx,
                false,
                MachineFont::Mega,
                JudgmentPalette::default(),
            ),
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        });
        std::hint::black_box(&*out);
        semantic_actor_tree_count(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for TimingPaneAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn actor_tree_count(actors: &[Actor]) -> u64 {
    actors.iter().fold(actors.len() as u64, |count, actor| {
        count
            + match actor {
                Actor::Frame { children, .. } => actor_tree_count(children),
                _ => 1,
            }
    })
}

#[cfg(any(test, feature = "bench-support"))]
fn semantic_actor_tree_count(actors: &[Actor]) -> u64 {
    match actors {
        [Actor::SharedFrame { children, .. }] => actor_tree_count(children),
        _ => actor_tree_count(actors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_pane_text_compiles_normal_values_inline() {
        let text = TimingPaneText::from_timing(timing::TimingStats {
            mean_abs_ms: 12.345,
            mean_ms: -3.5,
            stddev_ms: 2.25,
            max_abs_ms: 20.0,
        });

        assert_eq!(text.values[0].as_str(), "12.35ms");
        assert_eq!(text.values[1].as_str(), "-3.50ms");
        assert_eq!(text.values[2].as_str(), "6.75ms");
        assert_eq!(text.values[3].as_str(), "20.00ms");
        assert!(
            text.values
                .iter()
                .all(|value| matches!(value, TextContent::Inline(_)))
        );
    }

    #[test]
    fn timing_pane_text_shares_oversized_fallback() {
        let text = TimingPaneText::from_timing(timing::TimingStats {
            mean_abs_ms: f32::MAX,
            mean_ms: f32::MIN,
            stddev_ms: f32::MAX,
            max_abs_ms: f32::MAX,
        });
        let clone = text.values[0].clone();

        assert_eq!(text.values[0].as_str(), format!("{:.2}ms", f32::MAX));
        let (TextContent::Shared(text), TextContent::Shared(clone)) = (&text.values[0], &clone)
        else {
            panic!("oversized timing values should use shared text");
        };
        assert!(Arc::ptr_eq(text, clone));
    }

    #[test]
    fn direct_timing_append_matches_legacy_batch() {
        let fixture = TimingPaneAppendBenchmark::new();
        let mut legacy = Vec::with_capacity(1);
        let mut direct = Vec::with_capacity(1);

        assert_eq!(
            fixture.legacy_frame(&mut legacy),
            fixture.direct_frame(&mut direct)
        );
        assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));
    }

    #[test]
    fn retained_timing_matches_direct_and_reuses_the_shared_slice() {
        let fixture = TimingPaneAppendBenchmark::new();
        let mut direct = Vec::new();
        let mut retained = Vec::new();
        let _ = fixture.direct_frame(&mut direct);
        let _ = fixture.retained_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("expected retained timing actors in one shared frame");
        };
        assert_eq!(format!("{direct:#?}"), format!("{children:#?}"));

        let children = Arc::clone(children);
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected retained timing actors in one shared frame");
        };
        assert!(Arc::ptr_eq(&children, repeated));
    }
}
