use std::sync::Arc;

use deadlib_present::actors::TextContent;

pub mod eval_grades;
pub mod eval_graphs;
pub mod event_progress;
mod footer_clock;
pub mod pane_column;
pub mod pane_gs_records;
pub mod pane_machine_records;
pub mod pane_modifiers;
pub mod pane_percentage;
pub mod pane_qr;
pub mod pane_stats;
pub mod pane_timing;
pub mod pane_timing_arrows;
mod utils;

#[inline]
fn retained_text(args: std::fmt::Arguments<'_>) -> TextContent {
    TextContent::inline_format(args)
        .unwrap_or_else(|| TextContent::Shared(Arc::from(args.to_string())))
}

#[inline]
fn retained_str(value: &str) -> TextContent {
    TextContent::inline_str(value).unwrap_or_else(|| TextContent::Shared(Arc::from(value)))
}

pub(crate) use footer_clock::FooterClock;
pub(crate) use utils::{eval_style_alpha, pane_origin_x as test_input_pane_origin_x};

pub use event_progress::build_event_overlay;
pub use event_progress::build_event_progress_boxes;
pub use pane_column::{build_column_judgments_pane, build_column_judgments_pane_with_palette};
pub(crate) use pane_gs_records::{
    OnlineRecordsPresentation, build_arrowcloud_records_pane, build_gs_ex_records_pane,
    build_gs_records_pane, build_itl_records_pane, build_srpg_records_pane,
};
pub(crate) use pane_machine_records::{MachineRecordsPaneText, build_machine_records_pane};
pub use pane_modifiers::{build_modifiers_pane, push_modifiers_pane};
pub(crate) use pane_percentage::{PercentageText, build_pane_percentage_display_with_palette};
pub(crate) use pane_qr::{QrPanePresentation, build_gs_qr_pane};
pub(crate) use pane_stats::build_stats_pane_with_palette;
pub(crate) use pane_timing::{TimingPaneText, build_timing_pane_with_palette};
pub(crate) use pane_timing_arrows::{TimingArrowsText, build_timing_arrows_pane};
