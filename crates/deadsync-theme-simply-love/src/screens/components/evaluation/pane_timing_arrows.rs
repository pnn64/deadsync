use crate::act;
use crate::assets::{FontRole, machine_font_key_for_text};
use crate::config::MachineFont;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadsync_profile as profile_data;
use deadsync_rules::timing::{ArrowTimingBucket, ArrowTimingStats};
use std::cell::RefCell;
use std::sync::Arc;

#[cfg(any(test, feature = "bench-support"))]
use super::pane_column::build_pane3_arrow_preview;
use super::pane_column::{pane3_arrow_preview_capacity, push_pane3_arrow_preview};
use super::utils::pane_origin_x;

const LEFT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#FF3030");
const RIGHT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#3070FF");
const LABEL_RGBA: [f32; 4] = color::rgba_hex("#A0A0A0");
const VALUE_RGBA: [f32; 4] = color::rgba_hex("#FFFFFF");
const TIMING_ARROWS_TABLE_ACTORS: usize = 37;
const PANE_WIDTH: f32 = 300.0;
const PANE_HEIGHT: f32 = 180.0;
const HEADER_Y: f32 = 24.0;

fn timing_arrows_child_capacity(noteskin: Option<&deadsync_assets::noteskin::Noteskin>) -> usize {
    TIMING_ARROWS_TABLE_ACTORS
        + noteskin.map_or(0, |noteskin| {
            (0..4)
                .map(|col_idx| pane3_arrow_preview_capacity(noteskin, col_idx))
                .sum()
        })
}

#[derive(Clone)]
pub(crate) struct TimingArrowsText {
    cells: [[TextContent; 6]; 5],
    cached_table: RefCell<Option<(MachineFont, Arc<[Actor]>)>>,
}

impl TimingArrowsText {
    pub(crate) fn new(arrows: &ArrowTimingStats) -> Option<Self> {
        if arrows.per_column.len() != 4 {
            return None;
        }
        let buckets = [
            &arrows.per_column[0],
            &arrows.per_column[1],
            &arrows.per_column[2],
            &arrows.per_column[3],
            &arrows.left_foot,
            &arrows.right_foot,
        ];
        Some(Self {
            cells: std::array::from_fn(|row| {
                std::array::from_fn(|column| cell_text(buckets[column], row))
            }),
            cached_table: RefCell::new(None),
        })
    }

    fn cached_table(&self, machine_font: MachineFont) -> Arc<[Actor]> {
        if let Some((_, actors)) = self
            .cached_table
            .borrow()
            .as_ref()
            .filter(|(cached_font, _)| *cached_font == machine_font)
        {
            return Arc::clone(actors);
        }
        let actors = Arc::from(build_timing_arrows_table(self, machine_font));
        *self.cached_table.borrow_mut() = Some((machine_font, Arc::clone(&actors)));
        actors
    }
}

#[inline]
fn fmt_ms(value: f32) -> TextContent {
    super::retained_text(format_args!("{value:.2}"))
}

#[inline]
fn cell_text(bucket: &ArrowTimingBucket, row: usize) -> TextContent {
    if bucket.count == 0 {
        return TextContent::Static("-");
    }
    match row {
        0 => TextContent::inline_u32(bucket.count),
        1 => fmt_ms(bucket.stats.mean_abs_ms),
        2 => fmt_ms(bucket.stats.mean_ms),
        3 => fmt_ms(bucket.stats.stddev_ms * 3.0),
        4 => fmt_ms(bucket.stats.max_abs_ms),
        _ => TextContent::Static(""),
    }
}

/// Builds the per-arrow timing pane: a small table that breaks down
/// `# Steps`, `Mean Abs`, `Mean`, `Stddev*3`, and `Max` for each of the four
/// arrow directions plus the player's left and right foot.
fn timing_arrows_pane_actor(
    noteskin: Option<&deadsync_assets::noteskin::Noteskin>,
    text: &TimingArrowsText,
    controller: profile_data::PlayerSide,
    preview_elapsed: f32,
    machine_font: MachineFont,
    child_capacity: usize,
    mut append_preview: impl FnMut(
        &mut Vec<Actor>,
        &deadsync_assets::noteskin::Noteskin,
        usize,
        [f32; 2],
        f32,
    ),
) -> Actor {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;

    let pane_origin_x = pane_origin_x(controller);
    let frame_x = pane_width.mul_add(-0.5, pane_origin_x);
    let frame_y = deadlib_present::space::screen_center_y() - 56.0;

    let mut children = Vec::with_capacity(child_capacity);

    // Layout: 6 data columns + a row-label gutter.
    let label_col_width: f32 = 64.0;
    let data_area_left: f32 = label_col_width;
    let data_area_right: f32 = pane_width - 6.0;
    let data_area_width: f32 = data_area_right - data_area_left;
    let col_step: f32 = data_area_width / 6.0;
    let col_centers: [f32; 6] = [
        col_step.mul_add(0.5, data_area_left),
        col_step.mul_add(1.5, data_area_left),
        col_step.mul_add(2.5, data_area_left),
        col_step.mul_add(3.5, data_area_left),
        col_step.mul_add(4.5, data_area_left),
        col_step.mul_add(5.5, data_area_left),
    ];

    let header_y: f32 = 24.0;
    let row_start_y: f32 = 52.0;
    let row_step: f32 = 24.0;

    // Column headers: noteskin arrow previews for ←/↓/↑/→ (20% larger
    // than the column-judgments pane to give the table room to breathe).
    if let Some(ns) = noteskin {
        for col_idx in 0..4 {
            append_preview(
                &mut children,
                ns,
                col_idx,
                [col_centers[col_idx], header_y],
                preview_elapsed,
            );
        }
    }

    // L/R column headers.
    let foot_labels: [(&'static str, [f32; 4]); 2] =
        [("L", LEFT_FOOT_RGBA), ("R", RIGHT_FOOT_RGBA)];
    for (i, &(label, color_rgba)) in foot_labels.iter().enumerate() {
        let foot_header_font = machine_font_key_for_text(machine_font, FontRole::Header, label);
        children.push(act!(text: font(foot_header_font): settext(label):
            align(0.5, 0.5): xy(col_centers[4 + i], header_y):
            zoom(0.55):
            diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3])
        ));
    }

    let row_labels: [&'static str; 5] = ["# Steps", "Mean Abs", "Mean", "Stddev*3", "Max"];
    for (row_idx, &label) in row_labels.iter().enumerate() {
        let y = (row_idx as f32).mul_add(row_step, row_start_y);

        // Row label.
        children.push(act!(text: font("miso"): settext(label):
            align(1.0, 0.5): xy(label_col_width - 6.0, y):
            zoom(0.65):
            horizalign(right):
            diffuse(LABEL_RGBA[0], LABEL_RGBA[1], LABEL_RGBA[2], LABEL_RGBA[3])
        ));

        // `# Steps` is 50% larger than the timing-stat rows.
        let value_zoom = if row_idx == 0 { 1.05 } else { 0.7 };

        for col_idx in 0..6 {
            let color = match col_idx {
                4 => LEFT_FOOT_RGBA,
                5 => RIGHT_FOOT_RGBA,
                _ => VALUE_RGBA,
            };
            let value = text.cells[row_idx][col_idx].clone();
            children.push(act!(text: font("miso"): settext(value):
                align(0.5, 0.5): xy(col_centers[col_idx], y):
                zoom(value_zoom):
                diffuse(color[0], color[1], color[2], color[3])
            ));
        }
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

fn build_timing_arrows_table(text: &TimingArrowsText, machine_font: MachineFont) -> Vec<Actor> {
    let Actor::Frame { children, .. } = timing_arrows_pane_actor(
        None,
        text,
        profile_data::PlayerSide::P1,
        0.0,
        machine_font,
        TIMING_ARROWS_TABLE_ACTORS,
        |_, _, _, _, _| {},
    ) else {
        unreachable!("timing-arrow builder always returns one frame");
    };
    children
}

#[inline(always)]
fn timing_arrows_frame_offset(controller: profile_data::PlayerSide) -> [f32; 2] {
    [
        PANE_WIDTH.mul_add(-0.5, pane_origin_x(controller)),
        deadlib_present::space::screen_center_y() - 56.0,
    ]
}

#[inline(always)]
fn timing_arrows_column_centers() -> [f32; 4] {
    let data_area_left = 64.0_f32;
    let col_step = (PANE_WIDTH - 6.0 - data_area_left) / 6.0;
    [
        col_step.mul_add(0.5, data_area_left),
        col_step.mul_add(1.5, data_area_left),
        col_step.mul_add(2.5, data_area_left),
        col_step.mul_add(3.5, data_area_left),
    ]
}

fn promote_timing_arrow_previews(actors: &mut [Actor]) {
    for actor in actors {
        let z = match actor {
            Actor::Sprite { z, .. } | Actor::TexturedMesh { z, .. } => z,
            other => {
                debug_assert!(false, "unexpected timing-arrow preview actor: {other:?}");
                continue;
            }
        };
        *z = z.saturating_add(101);
    }
}

/// Appends animated previews directly and shares the immutable 37-actor table.
pub(crate) fn push_timing_arrows_pane(
    out: &mut Vec<Actor>,
    score_info: &ScoreInfo,
    text: &TimingArrowsText,
    controller: profile_data::PlayerSide,
    preview_elapsed: f32,
    machine_font: MachineFont,
) {
    push_retained_timing_arrows_pane(
        out,
        score_info.noteskin.as_deref(),
        text,
        controller,
        preview_elapsed,
        machine_font,
    );
}

fn push_retained_timing_arrows_pane(
    out: &mut Vec<Actor>,
    noteskin: Option<&deadsync_assets::noteskin::Noteskin>,
    text: &TimingArrowsText,
    controller: profile_data::PlayerSide,
    preview_elapsed: f32,
    machine_font: MachineFont,
) {
    let [frame_x, frame_y] = timing_arrows_frame_offset(controller);
    if let Some(noteskin) = noteskin {
        for (col_idx, center_x) in timing_arrows_column_centers().into_iter().enumerate() {
            let start = out.len();
            push_pane3_arrow_preview(
                out,
                noteskin,
                col_idx,
                [frame_x + center_x, frame_y + HEADER_Y],
                None,
                preview_elapsed,
                1.2,
            );
            promote_timing_arrow_previews(&mut out[start..]);
        }
    }
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(PANE_WIDTH), SizeSpec::Px(PANE_HEIGHT)],
        children: text.cached_table(machine_font),
        background: None,
        z: 101,
        tint: [1.0; 4],
        blend: None,
    });
}

#[cfg(any(test, feature = "bench-support"))]
fn build_timing_arrows_pane_legacy(
    noteskin: Option<&deadsync_assets::noteskin::Noteskin>,
    text: &TimingArrowsText,
    controller: profile_data::PlayerSide,
    preview_elapsed: f32,
    machine_font: MachineFont,
) -> Vec<Actor> {
    vec![timing_arrows_pane_actor(
        noteskin,
        text,
        controller,
        preview_elapsed,
        machine_font,
        0,
        |children, noteskin, col_idx, center, elapsed| {
            children.extend(build_pane3_arrow_preview(
                noteskin, col_idx, center, None, elapsed, 1.2,
            ));
        },
    )]
}

/// Stable old/new fixture for the populated per-arrow timing pane.
#[cfg(any(test, feature = "bench-support"))]
pub struct TimingArrowsPaneAppendBenchmark {
    noteskin: deadsync_assets::noteskin::Noteskin,
    text: TimingArrowsText,
}

#[cfg(any(test, feature = "bench-support"))]
impl TimingArrowsPaneAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let bucket = ArrowTimingBucket {
            count: 123,
            stats: deadsync_rules::timing::TimingStats {
                mean_abs_ms: 12.345,
                mean_ms: -3.5,
                stddev_ms: 2.25,
                max_abs_ms: 20.0,
            },
        };
        let text = TimingArrowsText::new(&ArrowTimingStats {
            per_column: vec![bucket; 4],
            left_foot: bucket,
            right_foot: bucket,
        })
        .expect("four columns have timing-arrow presentation");
        let noteskin = deadsync_assets::noteskin::load_itg_default(&deadsync_noteskin::Style {
            num_cols: 4,
            num_players: 1,
        })
        .expect("bundled dance noteskin should load");
        let _ = text.cached_table(MachineFont::Mega);
        Self { noteskin, text }
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_timing_arrows_pane_legacy(
            Some(&self.noteskin),
            &self.text,
            profile_data::PlayerSide::P1,
            1.25,
            MachineFont::Mega,
        ));
        std::hint::black_box(&*out);
        actor_tree_count(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let noteskin = Some(&self.noteskin);
        out.push(timing_arrows_pane_actor(
            noteskin,
            &self.text,
            profile_data::PlayerSide::P1,
            1.25,
            MachineFont::Mega,
            timing_arrows_child_capacity(noteskin),
            |children, noteskin, col_idx, center, elapsed| {
                push_pane3_arrow_preview(children, noteskin, col_idx, center, None, elapsed, 1.2);
            },
        ));
        std::hint::black_box(&*out);
        actor_tree_count(out)
    }

    #[must_use]
    pub fn retained_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_retained_timing_arrows_pane(
            out,
            Some(&self.noteskin),
            &self.text,
            profile_data::PlayerSide::P1,
            1.25,
            MachineFont::Mega,
        );
        std::hint::black_box(&*out);
        actor_tree_count(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for TimingArrowsPaneAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn actor_tree_count(actors: &[Actor]) -> u64 {
    actors
        .iter()
        .map(|actor| match actor {
            Actor::Frame { children, .. } => actor_tree_count(children),
            Actor::SharedFrame { children, .. } => actor_tree_count(children),
            _ => 1,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::TimingStats;

    fn arrows(bucket: ArrowTimingBucket) -> ArrowTimingStats {
        ArrowTimingStats {
            per_column: vec![bucket; 4],
            left_foot: bucket,
            right_foot: bucket,
        }
    }

    #[test]
    fn timing_arrow_text_compiles_normal_cells_inline() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket {
            count: 123,
            stats: TimingStats {
                mean_abs_ms: 12.345,
                mean_ms: -3.5,
                stddev_ms: 2.25,
                max_abs_ms: 20.0,
            },
        }))
        .expect("four columns have timing-arrow presentation");

        assert!(matches!(text.cells[0][0], TextContent::InlineU32(_)));
        assert_eq!(text.cells[0][0].as_str(), "123");
        assert_eq!(text.cells[1][0].as_str(), "12.35");
        assert_eq!(text.cells[2][0].as_str(), "-3.50");
        assert_eq!(text.cells[3][0].as_str(), "6.75");
        assert_eq!(text.cells[4][0].as_str(), "20.00");
        assert!(
            text.cells[1..]
                .iter()
                .flatten()
                .all(|cell| matches!(cell, TextContent::Inline(_)))
        );
    }

    #[test]
    fn timing_arrow_text_keeps_empty_buckets_static() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket::default()))
            .expect("four columns have timing-arrow presentation");

        assert!(
            text.cells
                .iter()
                .flatten()
                .all(|cell| matches!(cell, TextContent::Static("-")))
        );
    }

    #[test]
    fn timing_arrow_text_shares_oversized_float_fallback() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket {
            count: u32::MAX,
            stats: TimingStats {
                mean_abs_ms: f32::MAX,
                mean_ms: f32::MIN,
                stddev_ms: f32::MAX,
                max_abs_ms: f32::MAX,
            },
        }))
        .expect("four columns have timing-arrow presentation");

        assert_eq!(text.cells[0][0].as_str(), u32::MAX.to_string());
        assert_eq!(text.cells[1][0].as_str(), format!("{:.2}", f32::MAX));
        assert!(matches!(text.cells[1][0], TextContent::Shared(_)));
        assert!(TimingArrowsText::new(&ArrowTimingStats::default()).is_none());
    }

    #[test]
    fn direct_timing_arrows_append_matches_legacy_batch() {
        let fixture = TimingArrowsPaneAppendBenchmark::new();
        let mut legacy = Vec::with_capacity(1);
        let mut direct = Vec::with_capacity(1);

        assert_eq!(
            fixture.legacy_frame(&mut legacy),
            fixture.direct_frame(&mut direct)
        );
        assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));
    }

    #[test]
    fn retained_timing_arrows_match_direct_and_reuse_the_table() {
        let fixture = TimingArrowsPaneAppendBenchmark::new();
        let mut direct = Vec::with_capacity(1);
        let mut retained = Vec::with_capacity(8);
        assert_eq!(
            fixture.direct_frame(&mut direct),
            fixture.retained_frame(&mut retained),
        );

        let [
            Actor::Frame {
                offset,
                size,
                children: direct_children,
                ..
            },
        ] = direct.as_slice()
        else {
            panic!("expected direct timing arrows in one frame");
        };
        let Some(Actor::SharedFrame {
            offset: retained_offset,
            size: retained_size,
            children: table,
            ..
        }) = retained.last()
        else {
            panic!("expected retained timing-arrow table");
        };
        assert_eq!(offset, retained_offset);
        assert_eq!(format!("{size:?}"), format!("{retained_size:?}"));
        assert_eq!(table.len(), TIMING_ARROWS_TABLE_ACTORS);
        let preview_count = direct_children.len() - table.len();
        assert_eq!(retained.len(), preview_count + 1);
        assert_eq!(
            format!("{:#?}", &direct_children[preview_count..]),
            format!("{:#?}", table.as_ref()),
        );
        let preview_layout = |actor: &Actor| match actor {
            Actor::Sprite { offset, z, .. } | Actor::TexturedMesh { offset, z, .. } => {
                (*offset, *z)
            }
            other => panic!("unexpected timing-arrow preview actor: {other:?}"),
        };
        for (direct_actor, retained_actor) in direct_children[..preview_count]
            .iter()
            .zip(&retained[..preview_count])
        {
            let (direct_offset, direct_z) = preview_layout(direct_actor);
            let (retained_offset, retained_z) = preview_layout(retained_actor);
            assert!((retained_offset[0] - (direct_offset[0] + offset[0])).abs() < 0.001);
            assert!((retained_offset[1] - (direct_offset[1] + offset[1])).abs() < 0.001);
            assert_eq!(retained_z, direct_z.saturating_add(101));
        }

        let table_ptr = Arc::as_ptr(table).cast::<()>() as usize;
        let _ = fixture.retained_frame(&mut retained);
        let Some(Actor::SharedFrame {
            children: repeated, ..
        }) = retained.last()
        else {
            panic!("expected repeated retained timing-arrow table");
        };
        assert_eq!(table_ptr, Arc::as_ptr(repeated).cast::<()>() as usize);
    }
}
