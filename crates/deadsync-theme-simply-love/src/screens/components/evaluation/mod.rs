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

#[cfg(any(test, feature = "bench-support"))]
fn benchmark_asset_manager() -> crate::assets::AssetManager {
    use deadlib_present::font::{Font, Glyph, GlyphMap};
    use std::collections::HashMap;

    let texture_key = Arc::<str>::from("evaluation/benchmark-font.png");
    let glyph = Glyph {
        texture_key,
        stroke_texture_key: None,
        tex_rect: [0.0, 0.0, 8.0, 16.0],
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        size: [8.0, 16.0],
        offset: [0.0, 0.0],
        advance: 8.0,
        advance_i32: 8,
    };
    let mut glyph_map = GlyphMap::default();
    let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
    for byte in 32_u8..=126 {
        let ch = char::from(byte);
        glyph_map.insert(ch, glyph.clone());
        ascii_glyphs[byte as usize] = Some(glyph.clone());
    }
    let font = Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: Some(glyph),
        line_spacing: 20,
        height: 16,
        fallback_font_name: None,
        cache_tag: 0,
        chain_key: 0,
        default_stroke_color: [0.0, 0.0, 0.0, 1.0],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    };

    let mut assets = crate::assets::AssetManager::new();
    let font_names = [
        "miso",
        "wendy",
        crate::assets::machine_font_key(
            crate::config::MachineFont::Wendy,
            crate::assets::FontRole::ScreenEval,
        ),
        crate::assets::machine_font_key(
            crate::config::MachineFont::Mega,
            crate::assets::FontRole::ScreenEval,
        ),
        crate::assets::machine_font_key(
            crate::config::MachineFont::Mega,
            crate::assets::FontRole::Header,
        ),
    ];
    for name in font_names {
        assets.register_font(name, font.clone());
    }
    assets
}

pub(crate) use footer_clock::FooterClock;
pub(crate) use utils::{eval_style_alpha, pane_origin_x as test_input_pane_origin_x};

pub use event_progress::build_event_overlay;
pub use event_progress::build_event_progress_boxes;
pub(crate) use event_progress::{
    EventActorCache, push_cached_event_overlay, push_cached_event_progress_boxes,
};
pub(crate) use pane_column::{
    ColumnPanePresentation, push_cached_column_judgments_pane_with_palette,
};
pub use pane_column::{build_column_judgments_pane, build_column_judgments_pane_with_palette};
pub(crate) use pane_gs_records::{
    OnlineRecordsPresentation, push_arrowcloud_records_pane, push_gs_ex_records_pane,
    push_gs_records_pane, push_itl_records_pane, push_srpg_records_pane,
};
pub(crate) use pane_machine_records::{MachineRecordsPaneText, push_machine_records_pane};
pub(crate) use pane_modifiers::{ModifiersPanePresentation, push_cached_modifiers_pane};
pub use pane_modifiers::{build_modifiers_pane, push_modifiers_pane};
pub(crate) use pane_percentage::{PercentageText, push_pane_percentage_display_with_palette};
pub(crate) use pane_qr::{QrPanePresentation, push_gs_qr_pane};
pub(crate) use pane_stats::{StatsPanePresentation, push_stats_pane_with_palette};
pub(crate) use pane_timing::{TimingPaneText, push_timing_pane_with_palette};
pub(crate) use pane_timing_arrows::{TimingArrowsText, push_timing_arrows_pane};
