use crate::act;
use crate::assets::AssetManager;
use crate::assets::{FontRole, machine_font_key_for_text};
use crate::config::MachineFont;
use crate::screens::components::shared::banner as shared_banner;
use deadlib_present::actors::{Actor, SizeSpec, TextAttribute};
use deadlib_present::color::{self, JUDGMENT_RGBA};
use deadlib_present::font;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height};
use deadsync_chart::SongData;
use deadsync_profile as profile_data;
use deadsync_score as score_data;
use std::sync::Arc;

const ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const SRPG_YELLOW: [f32; 4] = [1.0, 0.972, 0.792, 1.0];
const POSITIVE_GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const NEGATIVE_RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const BODY_FALLBACK_HEIGHT: f32 = 15.0;
const BODY_FALLBACK_SPACING: f32 = 24.0;
const UPPER_ROW_HEIGHT: f32 = 25.0;
const UPPER_HEADER_FONT: &str = "wendy";
const OVERLAY_ROW_HEIGHT: f32 = 24.0;
const POPUP_DISMISS_TEXT: &str = "Press &START; to dismiss.";
const MORE_INFO_TEXT: &str = "More Information";
const OVERLAY_PANE_NAV_WIDTH: f32 = 230.0;
const OVERLAY_LB_ROWS: usize = 13;
const OVERLAY_LB_GRID_W: f32 = 230.0;
const OVERLAY_LB_RIVAL: [f32; 4] = color::rgba_hex("#BD94FF");
const OVERLAY_LB_SELF: [f32; 4] = color::rgba_hex("#A1FF94");
const OVERLAY_LB_TEXT_ZOOM: f32 = 1.0;
const TIER_BRONZE: [f32; 4] = color::rgba_hex("#966832");
const TIER_SILVER: [f32; 4] = color::rgba_hex("#A1AEC1");
const TIER_GOLD: [f32; 4] = color::rgba_hex("#F6AB2D");
const TIER_PRISMATIC: [f32; 4] = color::rgba_hex("#8731D2");

#[derive(Clone, Copy, PartialEq, Eq)]
struct EventFontKey {
    font_count: usize,
    miso: usize,
    wendy: usize,
    machine_header: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct UpperEventCacheKey {
    revision: u64,
    side: profile_data::PlayerSide,
    single_player: bool,
    fonts: EventFontKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OverlayEventCacheKey {
    revisions: [u64; 2],
    pages: [usize; 2],
    sides: [Option<profile_data::PlayerSide>; 2],
    panel_count: u8,
    single_player: bool,
    translated_titles: bool,
    machine_font: MachineFont,
    song_address: usize,
    fonts: EventFontKey,
}

/// Screen-lifetime, bounded actor caches for event progress and its overlay.
pub(crate) struct EventActorCache {
    upper: [Option<(UpperEventCacheKey, Arc<[Actor]>)>; 2],
    overlay: Option<(OverlayEventCacheKey, Arc<[Actor]>)>,
}

impl Default for EventActorCache {
    fn default() -> Self {
        Self {
            upper: std::array::from_fn(|_| None),
            overlay: None,
        }
    }
}

fn font_address(asset_manager: &AssetManager, name: &str) -> usize {
    asset_manager
        .with_font(name, |font| std::ptr::from_ref(font) as usize)
        .unwrap_or(0)
}

fn event_font_key(asset_manager: &AssetManager, machine_font: MachineFont) -> EventFontKey {
    EventFontKey {
        font_count: asset_manager.fonts().len(),
        miso: font_address(asset_manager, "miso"),
        wendy: font_address(asset_manager, UPPER_HEADER_FONT),
        machine_header: font_address(
            asset_manager,
            crate::assets::machine_font_key(machine_font, FontRole::Header),
        ),
    }
}

fn push_shared_root(out: &mut Vec<Actor>, children: Arc<[Actor]>) {
    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

#[inline(always)]
const fn event_color(kind: score_data::EventProgressKind) -> [f32; 4] {
    match kind {
        score_data::EventProgressKind::Itl => ITL_PINK,
        score_data::EventProgressKind::Srpg => SRPG_YELLOW,
    }
}

#[inline(always)]
const fn event_badge(kind: score_data::EventProgressKind) -> Option<&'static str> {
    match kind {
        score_data::EventProgressKind::Itl => Some("EX"),
        score_data::EventProgressKind::Srpg => None,
    }
}

#[inline(always)]
fn header_name(progress: &score_data::EventProgress) -> String {
    let mut text = match progress.kind {
        score_data::EventProgressKind::Itl => progress.name.replacen("ITL Online", "ITL", 1),
        score_data::EventProgressKind::Srpg => {
            if progress.name.trim().eq_ignore_ascii_case("rpg") {
                "SRPG".to_string()
            } else {
                progress.name.clone()
            }
        }
    };
    if progress.is_doubles && !text.contains("Doubles") {
        text.push_str(" Doubles");
    }
    text
}

#[inline(always)]
fn format_pct_hundredths(value: u32) -> String {
    format!("{}.{:02}%", value / 100, value % 100)
}

#[inline(always)]
fn format_signed_pct_hundredths(value: i32) -> String {
    let sign = if value < 0 { '-' } else { '+' };
    let abs = value.unsigned_abs();
    format!("({sign}{}.{:02}%)", abs / 100, abs % 100)
}

#[inline(always)]
fn format_signed_points(value: i32) -> String {
    format!("({value:+})")
}

#[inline(always)]
fn format_rate_hundredths(value: u32) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

#[inline(always)]
fn format_signed_rate_hundredths(value: i32) -> String {
    let sign = if value < 0 { '-' } else { '+' };
    let abs = value.unsigned_abs();
    format!("({sign}{}.{:02})", abs / 100, abs % 100)
}

#[inline(always)]
const fn clear_type_name(clear_type: u8) -> &'static str {
    match clear_type {
        0 => "No Play",
        1 => "Clear",
        2 => "FC",
        3 => "FEC",
        4 => "FFC",
        5 => "FBFC",
        _ => "Clear",
    }
}

#[inline(always)]
fn build_itl_box_body(progress: &score_data::EventProgress) -> String {
    format!(
        "EX Score: {} {}\n\
         Points: {} {}\n\n\
         Ranking Points: {} {}\n\
         Song Points: {} {}\n\
         EX Points: {} {}\n\
         Total Points: {} {}",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        progress.current_points,
        format_signed_points(progress.point_delta),
        progress.current_ranking_points,
        format_signed_points(progress.ranking_delta),
        progress.current_song_points,
        format_signed_points(progress.song_delta),
        progress.current_ex_points,
        format_signed_points(progress.ex_delta),
        progress.current_total_points,
        format_signed_points(progress.total_delta),
    )
}

#[inline(always)]
fn srpg_stat_lines(progress: &score_data::EventProgress) -> (Vec<String>, Vec<String>) {
    let srpg_stats = ["tp", "lp", "bb", "gold", "jp"];
    let show_qualifier_pair = progress.stat_improvements.len() >= 5;
    let mut qualifier = Vec::with_capacity(2);
    let mut stats = Vec::with_capacity(progress.stat_improvements.len());
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0
            || !srpg_stats
                .iter()
                .any(|stat| improvement.name.eq_ignore_ascii_case(stat))
        {
            continue;
        }
        let line = format!(
            "+{} {}",
            improvement.gained,
            improvement.name.to_uppercase()
        );
        if show_qualifier_pair
            && (improvement.name.eq_ignore_ascii_case("tp")
                || improvement.name.eq_ignore_ascii_case("lp"))
        {
            qualifier.push(line);
        } else {
            stats.push(line);
        }
    }
    (qualifier, stats)
}

#[inline(always)]
fn srpg_overlay_stat_lines(progress: &score_data::EventProgress) -> Vec<String> {
    progress
        .stat_improvements
        .iter()
        .filter(|improvement| improvement.gained > 0)
        .map(|improvement| {
            format!(
                "+{} {}",
                improvement.gained,
                improvement.name.to_uppercase()
            )
        })
        .collect()
}

#[inline(always)]
fn build_srpg_box_body(progress: &score_data::EventProgress) -> String {
    let mut body = format!(
        "Score: {} {}\n\
         Rate: {} {}\n",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        format_rate_hundredths(progress.rate_hundredths.unwrap_or(100)),
        format_signed_rate_hundredths(progress.rate_delta_hundredths.unwrap_or(0)),
    );
    let (qualifier, stats) = srpg_stat_lines(progress);
    if !qualifier.is_empty() || !stats.is_empty() {
        body.push('\n');
    }
    if !qualifier.is_empty() {
        body.push_str(qualifier.join(" ").as_str());
        body.push('\n');
    }
    for line in stats {
        body.push_str(line.as_str());
        body.push('\n');
    }
    body.trim_end().to_string()
}

#[inline(always)]
fn build_box_body(progress: &score_data::EventProgress) -> String {
    match progress.kind {
        score_data::EventProgressKind::Itl => build_itl_box_body(progress),
        score_data::EventProgressKind::Srpg => build_srpg_box_body(progress),
    }
}

#[inline(always)]
fn build_itl_stat_improvements(progress: &score_data::EventProgress) -> Option<String> {
    let (Some(before), Some(after)) = (progress.clear_type_before, progress.clear_type_after)
    else {
        return None;
    };
    (after > before).then(|| {
        format!(
            "Clear Type: {} >>> {}",
            clear_type_name(before),
            clear_type_name(after)
        )
    })
}

#[inline(always)]
fn build_itl_overlay_body(progress: &score_data::EventProgress) -> String {
    let mut text = format!(
        "EX Score: {} {}\n\
         Points: {} {}\n\n\
         Ranking Points: {} {}\n\
         Song Points: {} {}\n\
         EX Points: {} {}\n\
         Total Points: {} {}\n\n\
         You've passed the chart {} times",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        progress.current_points,
        format_signed_points(progress.point_delta),
        progress.current_ranking_points,
        format_signed_points(progress.ranking_delta),
        progress.current_song_points,
        format_signed_points(progress.song_delta),
        progress.current_ex_points,
        format_signed_points(progress.ex_delta),
        progress.current_total_points,
        format_signed_points(progress.total_delta),
        progress.total_passes,
    );
    if let Some(improvement) = build_itl_stat_improvements(progress) {
        text.push_str("\n\n");
        text.push_str(improvement.as_str());
    }
    text
}

#[inline(always)]
fn build_srpg_overlay_body(progress: &score_data::EventProgress) -> String {
    let mut text = format!(
        "Skill Improvements\n\n\
         {} {} at\n\
         {}x {} rate",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        format_rate_hundredths(progress.rate_hundredths.unwrap_or(100)),
        format_signed_rate_hundredths(progress.rate_delta_hundredths.unwrap_or(0)),
    );
    let stats = srpg_overlay_stat_lines(progress);
    if !stats.is_empty() {
        text.push_str("\n\n");
        text.push_str(stats.join("\n").as_str());
    }
    if !progress.skill_improvements.is_empty() {
        text.push_str("\n\n");
        text.push_str(progress.skill_improvements.join("\n").as_str());
    }
    text.trim_end().to_string()
}

#[inline(always)]
fn build_overlay_body(progress: &score_data::EventProgress) -> String {
    match progress.kind {
        score_data::EventProgressKind::Itl => build_itl_overlay_body(progress),
        score_data::EventProgressKind::Srpg => build_srpg_overlay_body(progress),
    }
}

#[inline(always)]
fn active_overlay_page(
    progress: &score_data::EventProgress,
    page_idx: usize,
) -> Option<&score_data::EventOverlayPage> {
    progress
        .overlay_pages
        .get(page_idx)
        .or_else(|| progress.overlay_pages.first())
}

#[inline(always)]
fn leaderboard_name(entry: &score_data::LeaderboardEntry) -> String {
    let name = entry.name.trim();
    if name.is_empty() {
        "----".to_string()
    } else {
        name.to_string()
    }
}

struct BodyLayout {
    text: String,
    zoom: f32,
}

#[derive(Clone, Copy)]
struct BodyBounds {
    top_y: f32,
    max_height: f32,
}

struct HeaderLayout {
    text: String,
    zoom: f32,
}

#[inline(always)]
fn wrap_text_with_measure<F>(
    raw_text: &str,
    max_width_px: f32,
    zoom: f32,
    measure: &mut F,
) -> String
where
    F: FnMut(&str) -> f32,
{
    let mut out = String::new();
    let mut is_first_output_line = true;
    for segment in raw_text.split('\n') {
        let trimmed = segment.trim_end();
        if trimmed.is_empty() {
            if !is_first_output_line {
                out.push('\n');
            }
            is_first_output_line = false;
            continue;
        }

        let mut current_line = String::new();
        for word in trimmed.split_whitespace() {
            let candidate = if current_line.is_empty() {
                word.to_owned()
            } else {
                let mut tmp = current_line.clone();
                tmp.push(' ');
                tmp.push_str(word);
                tmp
            };

            if !current_line.is_empty() && measure(candidate.as_str()) * zoom > max_width_px {
                if !is_first_output_line {
                    out.push('\n');
                }
                out.push_str(&current_line);
                is_first_output_line = false;
                current_line.clear();
                current_line.push_str(word);
            } else {
                current_line = candidate;
            }
        }

        if !is_first_output_line {
            out.push('\n');
        }
        out.push_str(&current_line);
        is_first_output_line = false;
    }

    out
}

#[inline(always)]
fn body_layout_with_measure<F>(
    text: &str,
    pane_width: f32,
    max_height: f32,
    font_height: f32,
    line_spacing: f32,
    mut measure: F,
) -> BodyLayout
where
    F: FnMut(&str) -> f32,
{
    let mut best = BodyLayout {
        text: text.to_string(),
        zoom: 0.1,
    };

    for zoom_step in (2..=20).rev() {
        let zoom = zoom_step as f32 / 20.0;
        let wrapped = wrap_text_with_measure(text, pane_width, zoom, &mut measure);
        let line_count = wrapped.split('\n').count().max(1) as f32;
        let block_height = (line_count - 1.0)
            .max(0.0)
            .mul_add(line_spacing, font_height);
        let layout = BodyLayout {
            text: wrapped,
            zoom,
        };
        if block_height * zoom <= max_height {
            return layout;
        }
        best = layout;
    }

    best
}

#[inline(always)]
fn header_layout_with_measure<F>(
    text: &str,
    pane_width: f32,
    row_height: f32,
    font_height: f32,
    line_spacing: f32,
    mut measure: F,
) -> HeaderLayout
where
    F: FnMut(&str) -> f32,
{
    let max_width = pane_width - 6.0;
    let max_height = row_height * 2.0;
    let mut best = HeaderLayout {
        text: text.to_string(),
        zoom: 0.1,
    };

    for zoom_step in (2..=10).rev() {
        let zoom = zoom_step as f32 / 20.0;
        let wrapped = wrap_text_with_measure(text, max_width, zoom, &mut measure);
        let line_count = wrapped.split('\n').count().max(1) as f32;
        let block_height = (line_count - 1.0)
            .max(0.0)
            .mul_add(line_spacing, font_height);
        let layout = HeaderLayout {
            text: wrapped,
            zoom,
        };
        if block_height * zoom <= max_height {
            return layout;
        }
        best = layout;
    }

    best
}

#[inline(always)]
fn body_bounds(pane_height: f32, row_height: f32, bottom_reserved: f32) -> BodyBounds {
    BodyBounds {
        top_y: row_height.mul_add(1.5, -pane_height * 0.5),
        max_height: row_height.mul_add(-1.5, pane_height) - bottom_reserved,
    }
}

#[inline(always)]
fn body_layout(
    asset_manager: &AssetManager,
    text: &str,
    pane_width: f32,
    max_height: f32,
) -> BodyLayout {
    asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |miso_font| {
                body_layout_with_measure(
                    text,
                    pane_width,
                    max_height,
                    miso_font.height.max(1) as f32,
                    miso_font.line_spacing.max(1) as f32,
                    |candidate| {
                        font::measure_line_width_logical(miso_font, candidate, all_fonts) as f32
                    },
                )
            })
        })
        .unwrap_or_else(|| {
            body_layout_with_measure(
                text,
                pane_width,
                max_height,
                BODY_FALLBACK_HEIGHT,
                BODY_FALLBACK_SPACING,
                |candidate| candidate.chars().count() as f32 * 8.0,
            )
        })
}

#[inline(always)]
fn upper_header_layout(asset_manager: &AssetManager, text: &str, pane_width: f32) -> HeaderLayout {
    asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font(UPPER_HEADER_FONT, |header_font| {
                header_layout_with_measure(
                    text,
                    pane_width,
                    UPPER_ROW_HEIGHT,
                    header_font.height as f32,
                    header_font.line_spacing.max(header_font.height) as f32,
                    |candidate| {
                        font::measure_line_width_logical(header_font, candidate, all_fonts) as f32
                    },
                )
            })
        })
        .unwrap_or_else(|| {
            header_layout_with_measure(
                text,
                pane_width,
                UPPER_ROW_HEIGHT,
                36.0,
                36.0,
                |candidate| candidate.chars().count() as f32 * 16.0,
            )
        })
}

#[inline(always)]
fn push_attr(
    attrs: &mut Vec<TextAttribute>,
    text: &str,
    byte_start: usize,
    byte_len: usize,
    color: [f32; 4],
) {
    let char_start = text[..byte_start].chars().count();
    let char_len = text[byte_start..byte_start + byte_len].chars().count();
    if char_len > 0 {
        attrs.push(TextAttribute {
            start: char_start,
            length: char_len,
            color,
            vertex_colors: None,
            glow: None,
        });
    }
}

fn build_body_attributes(text: &str, default_number_color: [f32; 4]) -> Vec<TextAttribute> {
    let mut attrs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let start = i;
        let mut j = i;
        if matches!(bytes[j], b'+' | b'-') {
            j += 1;
        }
        let mut has_digit = false;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            has_digit = true;
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'.' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                has_digit = true;
                j += 1;
            }
        }
        if has_digit {
            if j < bytes.len() && matches!(bytes[j], b'%' | b'x') {
                j += 1;
            }
            let color = match bytes[start] {
                b'+' => POSITIVE_GREEN,
                b'-' => NEGATIVE_RED,
                _ => default_number_color,
            };
            push_attr(&mut attrs, text, start, j - start, color);
            i = j;
            continue;
        }
        i += 1;
    }

    let mut offset = 0usize;
    while let Some(rel_start) = text[offset..].find('"') {
        let start = offset + rel_start;
        let Some(rel_end) = text[start + 1..].find('"') else {
            break;
        };
        let end = start + 1 + rel_end + 1;
        let quoted = &text[start + 1..end - 1];
        let quoted_color = match quoted {
            "Bronze" => TIER_BRONZE,
            "Silver" => TIER_SILVER,
            "Gold" => TIER_GOLD,
            "Prismatic" => TIER_PRISMATIC,
            _ => POSITIVE_GREEN,
        };
        push_attr(&mut attrs, text, start, end - start, quoted_color);
        offset = end;
    }

    if let Some(start) = text.find("Clear Type: ") {
        for (clear, color) in [
            ("FC", JUDGMENT_RGBA[2]),
            ("FEC", JUDGMENT_RGBA[1]),
            ("FFC", JUDGMENT_RGBA[0]),
            ("FBFC", ITL_PINK),
        ] {
            let mut search_from = start;
            while let Some(found) = text[search_from..].find(clear) {
                let byte_start = search_from + found;
                push_attr(&mut attrs, text, byte_start, clear.len(), color);
                search_from = byte_start + clear.len();
            }
        }
    }

    if let Some(start) = text.find("New ") {
        for (grade, color) in [("Quad", JUDGMENT_RGBA[0]), ("Quint", ITL_PINK)] {
            let mut search_from = start;
            while let Some(found) = text[search_from..].find(grade) {
                let byte_start = search_from + found;
                push_attr(&mut attrs, text, byte_start, grade.len(), color);
                search_from = byte_start + grade.len();
            }
        }
    }

    attrs
}

#[inline(always)]
fn build_upper_header_text(
    asset_manager: &AssetManager,
    text: String,
    pane_width: f32,
    y: f32,
    z: i16,
) -> Actor {
    let layout = upper_header_layout(asset_manager, text.as_str(), pane_width);
    act!(text:
        font(UPPER_HEADER_FONT):
        settext(layout.text):
        align(0.5, 0.5):
        xy(0.0, y):
        zoom(layout.zoom):
        wrapwidthpixels((pane_width - 6.0) / layout.zoom):
        horizalign(center):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z)
    )
}

#[inline(always)]
fn build_header_text(
    text: String,
    pane_width: f32,
    y: f32,
    z: i16,
    machine_font: MachineFont,
) -> Actor {
    let font_key = machine_font_key_for_text(machine_font, FontRole::Header, &text);
    act!(text:
        font(font_key):
        settext(text):
        align(0.5, 0.5):
        xy(0.0, y):
        zoom(0.5):
        maxwidth((pane_width - 6.0) / 0.5):
        horizalign(center):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z)
    )
}

#[inline(always)]
fn build_body_text(
    asset_manager: &AssetManager,
    text: String,
    wrap_width: f32,
    bounds: BodyBounds,
    default_number_color: [f32; 4],
    z: i16,
) -> Actor {
    let layout = body_layout(asset_manager, text.as_str(), wrap_width, bounds.max_height);
    let mut actor = act!(text:
        font("miso"):
        settext(layout.text):
        align(0.5, 0.0):
        xy(0.0, bounds.top_y):
        zoom(layout.zoom):
        wrapwidthpixels(wrap_width / layout.zoom):
        horizalign(center):
        valign(top):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z)
    );
    if let Actor::Text {
        content,
        attributes,
        ..
    } = &mut actor
    {
        *attributes = build_body_attributes(content.as_str(), default_number_color).into();
    }
    actor
}

fn build_overlay_leaderboard(
    entries: &[score_data::LeaderboardEntry],
    pane_width: f32,
    single_player: bool,
    z: i16,
) -> Vec<Actor> {
    let rank_x = OVERLAY_LB_GRID_W.mul_add(-0.5, -(pane_width - OVERLAY_LB_GRID_W) * 0.5) + 32.0;
    let name_x = OVERLAY_LB_GRID_W.mul_add(-0.5, -(pane_width - OVERLAY_LB_GRID_W) * 0.5) + 100.0;
    let score_x = OVERLAY_LB_GRID_W.mul_add(0.5, -(pane_width - OVERLAY_LB_GRID_W) * 0.5) - 2.0;
    let date_x = score_x + 100.0;
    let first_row_y = -OVERLAY_ROW_HEIGHT * ((OVERLAY_LB_ROWS - 1) as f32 * 0.5);
    let mut rows: Vec<(
        String,
        String,
        String,
        String,
        [f32; 4],
        [f32; 4],
        Option<[f32; 4]>,
    )> = Vec::with_capacity(OVERLAY_LB_ROWS);

    if entries.is_empty() {
        rows.push((
            String::new(),
            "No Scores".to_string(),
            String::new(),
            String::new(),
            WHITE,
            WHITE,
            None,
        ));
    } else {
        for entry in entries.iter().take(OVERLAY_LB_ROWS) {
            let bg = if entry.is_rival {
                Some(OVERLAY_LB_RIVAL)
            } else if entry.is_self {
                Some(OVERLAY_LB_SELF)
            } else {
                None
            };
            let row_color = if bg.is_some() { BLACK } else { WHITE };
            let score_color = if entry.is_fail {
                NEGATIVE_RED
            } else {
                row_color
            };
            rows.push((
                format!("{}.", entry.rank),
                leaderboard_name(entry),
                format!("{:.2}%", entry.score / 100.0),
                score_data::format_leaderboard_date_or_placeholder(&entry.date),
                row_color,
                score_color,
                bg,
            ));
        }
    }

    while rows.len() < OVERLAY_LB_ROWS {
        rows.push((
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            WHITE,
            WHITE,
            None,
        ));
    }

    let mut children = Vec::with_capacity(OVERLAY_LB_ROWS * 5);
    for (idx, (rank, name, score, date, row_color, score_color, bg)) in rows.into_iter().enumerate()
    {
        let y = OVERLAY_ROW_HEIGHT.mul_add(idx as f32, first_row_y);
        if let Some(bg) = bg {
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(0.0, y):
                setsize(pane_width, OVERLAY_ROW_HEIGHT):
                diffuse(bg[0], bg[1], bg[2], bg[3]):
                z(z)
            ));
        }
        children.push(act!(text:
            font("miso"):
            settext(rank):
            align(1.0, 0.5):
            xy(rank_x, y):
            zoom(OVERLAY_LB_TEXT_ZOOM):
            maxwidth(30.0):
            horizalign(right):
            diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
            z(z + 1)
        ));
        children.push(act!(text:
            font("miso"):
            settext(name):
            align(0.5, 0.5):
            xy(name_x, y):
            zoom(OVERLAY_LB_TEXT_ZOOM):
            maxwidth(130.0):
            horizalign(center):
            diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
            z(z + 1)
        ));
        children.push(act!(text:
            font("miso"):
            settext(score):
            align(1.0, 0.5):
            xy(score_x, y):
            zoom(OVERLAY_LB_TEXT_ZOOM):
            horizalign(right):
            diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
            z(z + 1)
        ));
        if single_player {
            children.push(act!(text:
                font("miso"):
                settext(date):
                align(1.0, 0.5):
                xy(date_x, y):
                zoom(OVERLAY_LB_TEXT_ZOOM):
                horizalign(right):
                diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                z(z + 1)
            ));
        }
    }

    children
}

fn build_overlay_banner_and_song(song: &SongData, translated_titles: bool, z: i16) -> Vec<Actor> {
    let mut children = Vec::with_capacity(2);
    if let Some(banner_path) = song.banner_path.as_ref() {
        let banner_key = banner_path.to_string_lossy().into_owned();
        children.push(shared_banner::sprite(
            banner_key, 0.0, 112.0, 418.0, 164.0, 0.34, z,
        ));
    }
    children.push(act!(text:
        font("miso"):
        settext(song.display_full_title(translated_titles)):
        align(0.5, 0.0):
        xy(0.0, 142.6):
        zoom(0.68):
        maxwidth(500.0 / 0.68):
        horizalign(center):
        valign(top):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z + 1)
    ));
    children
}

fn build_upper_panel(
    asset_manager: &AssetManager,
    center_x: f32,
    center_y: f32,
    pane_width: f32,
    pane_height: f32,
    progress: &score_data::EventProgress,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let event_color = event_color(progress.kind);
    let mut children = Vec::with_capacity(4);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width, pane_height):
        diffuse(1.0, 1.0, 1.0, 0.1):
        z(0)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width - border_width, pane_height - border_width):
        diffuse(0.0, 0.0, 0.0, 0.85):
        z(1)
    ));
    children.push(build_upper_header_text(
        asset_manager,
        header_name(progress),
        pane_width,
        (-pane_height).mul_add(0.5, 15.0),
        2,
    ));
    children.push(build_body_text(
        asset_manager,
        build_box_body(progress),
        pane_width - border_width,
        body_bounds(pane_height, UPPER_ROW_HEIGHT, 0.0),
        event_color,
        2,
    ));

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [center_x, center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z,
    }
}

fn build_overlay_panel(
    asset_manager: &AssetManager,
    center_x: f32,
    center_y: f32,
    pane_width: f32,
    pane_height: f32,
    song: Option<&SongData>,
    translated_titles: bool,
    progress: &score_data::EventProgress,
    page_idx: usize,
    machine_font: MachineFont,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let event_color = event_color(progress.kind);
    let badge = event_badge(progress.kind);
    let header_y = (-pane_height).mul_add(0.5, 12.0);
    let header_bar_y = OVERLAY_ROW_HEIGHT.mul_add(0.5, -pane_height * 0.5);
    let has_more_info = progress.overlay_pages.len() > 1;
    let bottom_reserved = if has_more_info {
        OVERLAY_ROW_HEIGHT
    } else {
        0.0
    };
    let single_player = pane_width > OVERLAY_LB_GRID_W;
    let mut children = Vec::with_capacity(11 + OVERLAY_LB_ROWS * 5);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width + border_width, pane_height + border_width + 1.0):
        diffuse(event_color[0], event_color[1], event_color[2], event_color[3]):
        z(0)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width, pane_height):
        diffuse(BLACK[0], BLACK[1], BLACK[2], BLACK[3]):
        z(1)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width + border_width, OVERLAY_ROW_HEIGHT + border_width + 1.0):
        diffuse(event_color[0], event_color[1], event_color[2], event_color[3]):
        z(2)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width, OVERLAY_ROW_HEIGHT):
        diffuse(0.157, 0.157, 0.165, 1.0):
        z(3)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width, OVERLAY_ROW_HEIGHT):
        diffuse(0.3, 0.3, 0.3, 0.55):
        fadebottom(1.0):
        z(3)
    ));
    children.push(build_header_text(
        header_name(progress),
        pane_width,
        header_y,
        4,
        machine_font,
    ));
    if let Some(badge) = badge {
        children.push(act!(text:
            font(machine_font_key_for_text(machine_font, FontRole::Header, badge)):
            settext(badge):
            align(0.5, 0.5):
            xy(pane_width.mul_add(0.5, -18.0), header_y):
            zoom(0.5):
            diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
            z(4)
        ));
    }
    match active_overlay_page(progress, page_idx) {
        Some(score_data::EventOverlayPage::Leaderboard(entries)) => {
            children.extend(build_overlay_leaderboard(
                entries.as_slice(),
                pane_width,
                single_player,
                4,
            ));
            if let Some(song) = song {
                children.extend(build_overlay_banner_and_song(song, translated_titles, 4));
            }
        }
        Some(score_data::EventOverlayPage::Text(text)) => children.push(build_body_text(
            asset_manager,
            text.clone(),
            pane_width,
            body_bounds(pane_height, OVERLAY_ROW_HEIGHT, bottom_reserved),
            event_color,
            4,
        )),
        None => children.push(build_body_text(
            asset_manager,
            build_overlay_body(progress),
            pane_width,
            body_bounds(pane_height, OVERLAY_ROW_HEIGHT, bottom_reserved),
            event_color,
            4,
        )),
    }
    if has_more_info {
        let nav_y = OVERLAY_ROW_HEIGHT.mul_add(-0.5, pane_height * 0.5);
        let icon_x = OVERLAY_PANE_NAV_WIDTH.mul_add(0.5, -10.0);
        children.push(act!(text:
            font("miso"):
            settext("&MENULEFT;"):
            align(0.5, 0.5):
            xy(-icon_x, nav_y):
            zoom(1.0):
            diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
            z(4)
        ));
        children.push(act!(text:
            font("miso"):
            settext(MORE_INFO_TEXT):
            align(0.5, 0.5):
            xy(0.0, nav_y - 2.0):
            zoom(1.0):
            diffuse(event_color[0], event_color[1], event_color[2], event_color[3]):
            horizalign(center):
            z(4)
        ));
        children.push(act!(text:
            font("miso"):
            settext("&MENURiGHT;"):
            align(0.5, 0.5):
            xy(icon_x, nav_y):
            zoom(1.0):
            diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
            z(4)
        ));
    }

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [center_x, center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z,
    }
}

#[must_use]
pub fn build_event_progress_boxes(
    asset_manager: &AssetManager,
    side: profile_data::PlayerSide,
    single_player: bool,
    progress: &[score_data::EventProgress],
) -> Vec<Actor> {
    if progress.is_empty() {
        return Vec::new();
    }
    let upper_origin_x = match side {
        profile_data::PlayerSide::P1 => screen_center_x() - 155.0,
        profile_data::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let dir = if side == profile_data::PlayerSide::P1 {
        -1.0
    } else {
        1.0
    };
    let (center_x, center_y, pane_width, pane_height) = if single_player {
        (381.0f32.mul_add(-dir, upper_origin_x), 109.0, 156.0, 144.0)
    } else {
        (211.0f32.mul_add(dir, upper_origin_x), 274.0, 118.0, 180.0)
    };
    let stack_gap = 8.0;
    let visible = progress.len().min(2);
    let first_offset = if single_player {
        0.0
    } else {
        -((visible - 1) as f32) * (pane_height + stack_gap) * 0.5
    };
    progress
        .iter()
        .take(visible)
        .enumerate()
        .map(|(idx, event)| {
            build_upper_panel(
                asset_manager,
                center_x,
                (idx as f32).mul_add(pane_height + stack_gap, center_y + first_offset),
                pane_width,
                pane_height,
                event,
                104,
            )
        })
        .collect()
}

#[must_use]
pub fn build_event_overlay(
    asset_manager: &AssetManager,
    single_player: bool,
    song: Option<&SongData>,
    translated_titles: bool,
    panels: &[(profile_data::PlayerSide, &score_data::EventProgress, usize)],
    machine_font: MachineFont,
) -> Vec<Actor> {
    if panels.is_empty() {
        return Vec::new();
    }

    let pane_width = if panels.len() == 1 { 330.0 } else { 230.0 };
    let pane_height = 360.0;
    let center_y = screen_center_y() - 15.0;
    let mut actors = Vec::with_capacity(2 + panels.len());
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_center_x() * 2.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(2000)
    ));

    for (idx, (side, progress, page_idx)) in panels.iter().enumerate() {
        let center_x = if single_player {
            screen_center_x()
        } else if idx == 0 && *side == profile_data::PlayerSide::P1 {
            screen_center_x() - 160.0
        } else if idx == 0 && *side == profile_data::PlayerSide::P2 {
            screen_center_x() + 160.0
        } else if *side == profile_data::PlayerSide::P1 {
            screen_center_x() - 160.0
        } else {
            screen_center_x() + 160.0
        };
        actors.push(build_overlay_panel(
            asset_manager,
            center_x,
            center_y,
            pane_width,
            pane_height,
            song,
            translated_titles,
            progress,
            *page_idx,
            machine_font,
            2001,
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext(POPUP_DISMISS_TEXT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        horizalign(center):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(2002)
    ));

    actors
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_cached_event_progress_boxes(
    out: &mut Vec<Actor>,
    cache: &mut EventActorCache,
    player_index: usize,
    revision: u64,
    asset_manager: &AssetManager,
    side: profile_data::PlayerSide,
    single_player: bool,
    progress: &[score_data::EventProgress],
) {
    if progress.is_empty() || player_index >= cache.upper.len() {
        return;
    }
    let key = UpperEventCacheKey {
        revision,
        side,
        single_player,
        fonts: event_font_key(asset_manager, MachineFont::Wendy),
    };
    let slot = &mut cache.upper[player_index];
    if !slot.as_ref().is_some_and(|(cached, _)| *cached == key) {
        *slot = Some((
            key,
            Arc::from(build_event_progress_boxes(
                asset_manager,
                side,
                single_player,
                progress,
            )),
        ));
    }
    push_shared_root(
        out,
        Arc::clone(&slot.as_ref().expect("upper event cache was populated").1),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_cached_event_overlay(
    out: &mut Vec<Actor>,
    cache: &mut EventActorCache,
    revisions: [u64; 2],
    pages: [usize; 2],
    asset_manager: &AssetManager,
    single_player: bool,
    song: Option<&SongData>,
    translated_titles: bool,
    panels: &[(profile_data::PlayerSide, &score_data::EventProgress, usize)],
    machine_font: MachineFont,
) {
    if panels.is_empty() {
        return;
    }
    let mut sides = [None; 2];
    for (slot, (side, _, _)) in sides.iter_mut().zip(panels) {
        *slot = Some(*side);
    }
    let key = OverlayEventCacheKey {
        revisions,
        pages,
        sides,
        panel_count: panels.len().min(u8::MAX as usize) as u8,
        single_player,
        translated_titles,
        machine_font,
        song_address: song.map_or(0, |song| std::ptr::from_ref(song) as usize),
        fonts: event_font_key(asset_manager, machine_font),
    };
    if !cache
        .overlay
        .as_ref()
        .is_some_and(|(cached, _)| *cached == key)
    {
        cache.overlay = Some((
            key,
            Arc::from(build_event_overlay(
                asset_manager,
                single_player,
                song,
                translated_titles,
                panels,
                machine_font,
            )),
        ));
    }
    push_shared_root(
        out,
        Arc::clone(
            &cache
                .overlay
                .as_ref()
                .expect("event overlay cache was populated")
                .1,
        ),
    );
}

#[cfg(any(test, feature = "bench-support"))]
fn benchmark_progress() -> Vec<score_data::EventProgress> {
    let leaderboard = (1..=OVERLAY_LB_ROWS as u32)
        .map(|rank| score_data::LeaderboardEntry {
            rank,
            name: format!("PLAYER{rank:02}"),
            machine_tag: None,
            score: 10_000.0 - f64::from(rank) * 7.5,
            date: "2026-08-30".into(),
            is_rival: rank == 2,
            is_self: rank == 5,
            is_fail: rank == 13,
        })
        .collect();
    vec![
        score_data::EventProgress {
            kind: score_data::EventProgressKind::Itl,
            name: "ITL Online 2026".into(),
            score_hundredths: 9_876,
            score_delta_hundredths: 25,
            current_points: 1_250,
            point_delta: 50,
            current_ranking_points: 700,
            ranking_delta: 12,
            current_song_points: 450,
            song_delta: 30,
            current_ex_points: 1_900,
            ex_delta: 45,
            current_total_points: 8_500,
            total_delta: 87,
            total_passes: 4,
            overlay_pages: vec![score_data::EventOverlayPage::Leaderboard(leaderboard)],
            ..Default::default()
        },
        score_data::EventProgress {
            kind: score_data::EventProgressKind::Srpg,
            name: "Stamina RPG 10".into(),
            score_hundredths: 9_432,
            score_delta_hundredths: 17,
            rate_hundredths: Some(125),
            rate_delta_hundredths: Some(5),
            overlay_pages: vec![score_data::EventOverlayPage::Text(
                "Quest complete! Earned 250 gold and unlocked a new title.".into(),
            )],
            ..Default::default()
        },
    ]
}

/// Stable old/new fixture for compact event-progress boxes.
#[cfg(any(test, feature = "bench-support"))]
pub struct EventProgressCacheBenchmark {
    assets: AssetManager,
    progress: Vec<score_data::EventProgress>,
    cache: EventActorCache,
}

#[cfg(any(test, feature = "bench-support"))]
impl EventProgressCacheBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let mut fixture = Self {
            assets: super::benchmark_asset_manager(),
            progress: benchmark_progress(),
            cache: EventActorCache::default(),
        };
        let mut warm = Vec::new();
        let _ = fixture.retained_frame(&mut warm);
        fixture
    }

    #[must_use]
    pub fn legacy_frame(&mut self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_event_progress_boxes(
            &self.assets,
            profile_data::PlayerSide::P1,
            true,
            &self.progress,
        ));
        actor_tree_checksum(out)
    }

    #[must_use]
    pub fn retained_frame(&mut self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_cached_event_progress_boxes(
            out,
            &mut self.cache,
            0,
            1,
            &self.assets,
            profile_data::PlayerSide::P1,
            true,
            &self.progress,
        );
        actor_tree_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for EventProgressCacheBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable old/new fixture for the full event overlay actor tree.
#[cfg(any(test, feature = "bench-support"))]
pub struct EventOverlayCacheBenchmark {
    assets: AssetManager,
    progress: Vec<score_data::EventProgress>,
    cache: EventActorCache,
}

#[cfg(any(test, feature = "bench-support"))]
impl EventOverlayCacheBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let mut fixture = Self {
            assets: super::benchmark_asset_manager(),
            progress: benchmark_progress(),
            cache: EventActorCache::default(),
        };
        let mut warm = Vec::new();
        let _ = fixture.retained_frame(&mut warm);
        fixture
    }

    #[must_use]
    pub fn legacy_frame(&mut self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let panels = [
            (profile_data::PlayerSide::P1, &self.progress[0], 0),
            (profile_data::PlayerSide::P2, &self.progress[1], 0),
        ];
        out.extend(build_event_overlay(
            &self.assets,
            false,
            None,
            false,
            &panels,
            MachineFont::Mega,
        ));
        actor_tree_checksum(out)
    }

    #[must_use]
    pub fn retained_frame(&mut self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let panels = [
            (profile_data::PlayerSide::P1, &self.progress[0], 0),
            (profile_data::PlayerSide::P2, &self.progress[1], 0),
        ];
        push_cached_event_overlay(
            out,
            &mut self.cache,
            [1, 1],
            [0, 0],
            &self.assets,
            false,
            None,
            false,
            &panels,
            MachineFont::Mega,
        );
        actor_tree_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for EventOverlayCacheBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn actor_tree_checksum(actors: &[Actor]) -> u64 {
    let semantic_actors = match actors {
        [Actor::SharedFrame { children, .. }] => children.as_ref(),
        _ => actors,
    };
    let stats = deadlib_present::actors::actor_tree_stats(semantic_actors);
    (u64::from(stats.total) << 32) | u64::from(stats.text_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_achievement_titles_wrap_without_shrinking_body_text() {
        let short = body_layout_with_measure(
            "Completed the \"Short Achievement\" Achievement!",
            330.0,
            OVERLAY_ROW_HEIGHT.mul_add(-2.5, 360.0),
            BODY_FALLBACK_HEIGHT,
            BODY_FALLBACK_SPACING,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        let long = body_layout_with_measure(
            "Completed the \"This Achievement Title Is Extremely Long And Should Wrap Instead Of Shrinking The Popup Text\" Achievement!",
            330.0,
            OVERLAY_ROW_HEIGHT.mul_add(-2.5, 360.0),
            BODY_FALLBACK_HEIGHT,
            BODY_FALLBACK_SPACING,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        assert_eq!(short.zoom, 1.0);
        assert_eq!(long.zoom, short.zoom);
        assert!(long.text.contains('\n'));
    }

    #[test]
    fn tall_body_text_still_scales_down_to_fit_height() {
        let text = (0..18)
            .map(|idx| format!("Line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let layout = body_layout_with_measure(
            text.as_str(),
            330.0,
            OVERLAY_ROW_HEIGHT.mul_add(-2.5, 180.0),
            BODY_FALLBACK_HEIGHT,
            BODY_FALLBACK_SPACING,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        assert!(layout.zoom < 1.0);
    }

    #[test]
    fn tall_body_text_reserves_more_information_row() {
        let text = (0..14)
            .map(|idx| format!("Line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let layout = body_layout_with_measure(
            text.as_str(),
            330.0,
            OVERLAY_ROW_HEIGHT.mul_add(-2.5, 360.0),
            BODY_FALLBACK_HEIGHT,
            BODY_FALLBACK_SPACING,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        let block_height = 13.0f32.mul_add(BODY_FALLBACK_SPACING, BODY_FALLBACK_HEIGHT);
        let available_height = OVERLAY_ROW_HEIGHT.mul_add(-2.5, 360.0);

        assert!((layout.zoom - 0.9).abs() < f32::EPSILON);
        assert!(block_height * layout.zoom <= available_height);
    }

    #[test]
    fn doubles_itl_header_wraps_in_narrow_progress_box() {
        let layout = header_layout_with_measure(
            "ITL 2026 Doubles",
            118.0,
            UPPER_ROW_HEIGHT,
            36.0,
            36.0,
            |candidate| candidate.chars().count() as f32 * 16.0,
        );
        assert_eq!(layout.zoom, 0.5);
        assert_eq!(layout.text, "ITL 2026\nDoubles");
    }

    #[test]
    fn retained_progress_boxes_match_legacy_and_reuse_the_shared_slice() {
        let mut fixture = EventProgressCacheBenchmark::new();
        let legacy = build_event_progress_boxes(
            &fixture.assets,
            profile_data::PlayerSide::P1,
            true,
            &fixture.progress,
        );
        let mut retained = Vec::new();
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children,
                align,
                offset,
                tint,
                blend,
                ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected retained progress boxes in one shared frame");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));
        assert_eq!(*align, [0.0, 0.0]);
        assert_eq!(*offset, [0.0, 0.0]);
        assert_eq!(*tint, [1.0; 4]);
        assert_eq!(*blend, None);

        let children = Arc::clone(children);
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected retained progress boxes in one shared frame");
        };
        assert!(Arc::ptr_eq(&children, repeated));
    }

    #[test]
    fn retained_event_overlay_matches_legacy_and_reuses_the_shared_slice() {
        let mut fixture = EventOverlayCacheBenchmark::new();
        let panels = [
            (profile_data::PlayerSide::P1, &fixture.progress[0], 0),
            (profile_data::PlayerSide::P2, &fixture.progress[1], 0),
        ];
        let legacy = build_event_overlay(
            &fixture.assets,
            false,
            None,
            false,
            &panels,
            MachineFont::Mega,
        );
        let mut retained = Vec::new();
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children,
                align,
                offset,
                tint,
                blend,
                ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected retained event overlay in one shared frame");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));
        assert_eq!(*align, [0.0, 0.0]);
        assert_eq!(*offset, [0.0, 0.0]);
        assert_eq!(*tint, [1.0; 4]);
        assert_eq!(*blend, None);

        let children = Arc::clone(children);
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected retained event overlay in one shared frame");
        };
        assert!(Arc::ptr_eq(&children, repeated));
    }
}
