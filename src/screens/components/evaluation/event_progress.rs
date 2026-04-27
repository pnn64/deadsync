use crate::act;
use crate::assets::AssetManager;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::engine::present::actors::{Actor, SizeSpec, TextAttribute};
use crate::engine::present::color::{self, JUDGMENT_RGBA};
use crate::engine::present::font;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height};
use crate::game::{profile, scores, song::SongData};
use crate::screens::components::shared::banner as shared_banner;

use super::utils::format_machine_record_date;

const ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const POSITIVE_GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const NEGATIVE_RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const BODY_FONT_HEIGHT: f32 = 19.0;
const BODY_LINE_SPACING: f32 = 24.0;
const UPPER_ROW_HEIGHT: f32 = 25.0;
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

#[inline(always)]
fn header_name(name: &str, is_doubles: bool) -> String {
    let mut text = name.replacen("ITL Online", "ITL", 1);
    if is_doubles && !text.contains("Doubles") {
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
fn clear_type_name(clear_type: u8) -> &'static str {
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
fn build_box_body(progress: &scores::ItlEventProgress) -> String {
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
fn build_stat_improvements(progress: &scores::ItlEventProgress) -> Option<String> {
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
fn build_overlay_body(progress: &scores::ItlEventProgress) -> String {
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
    if let Some(improvement) = build_stat_improvements(progress) {
        text.push_str("\n\n");
        text.push_str(improvement.as_str());
    }
    text
}

#[inline(always)]
fn active_overlay_page(
    progress: &scores::ItlEventProgress,
    page_idx: usize,
) -> Option<&scores::ItlOverlayPage> {
    progress
        .overlay_pages
        .get(page_idx)
        .or_else(|| progress.overlay_pages.first())
}

#[inline(always)]
fn leaderboard_name(entry: &scores::LeaderboardEntry) -> String {
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

#[inline(always)]
fn wrap_body_text_with_measure<F>(
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
    pane_height: f32,
    row_height: f32,
    mut measure: F,
) -> BodyLayout
where
    F: FnMut(&str) -> f32,
{
    let max_height = pane_height - row_height * 1.5;
    let mut best = BodyLayout {
        text: text.to_string(),
        zoom: 0.1,
    };

    for zoom_step in (2..=20).rev() {
        let zoom = zoom_step as f32 / 20.0;
        let wrapped = wrap_body_text_with_measure(text, pane_width, zoom, &mut measure);
        let line_count = wrapped.split('\n').count().max(1) as f32;
        let block_height = BODY_FONT_HEIGHT + (line_count - 1.0).max(0.0) * BODY_LINE_SPACING;
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
fn body_layout(
    asset_manager: &AssetManager,
    text: &str,
    pane_width: f32,
    pane_height: f32,
    row_height: f32,
) -> BodyLayout {
    asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |miso_font| {
                body_layout_with_measure(text, pane_width, pane_height, row_height, |candidate| {
                    font::measure_line_width_logical(miso_font, candidate, all_fonts) as f32
                })
            })
        })
        .unwrap_or_else(|| {
            body_layout_with_measure(text, pane_width, pane_height, row_height, |candidate| {
                candidate.chars().count() as f32 * 8.0
            })
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

fn build_body_attributes(text: &str) -> Vec<TextAttribute> {
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
                _ => ITL_PINK,
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
fn build_header_text(text: String, pane_width: f32, y: f32, z: i16) -> Actor {
    let font_key = current_machine_font_key_for_text(FontRole::Header, &text);
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
    pane_height: f32,
    row_height: f32,
    z: i16,
) -> Actor {
    let layout = body_layout(
        asset_manager,
        text.as_str(),
        wrap_width,
        pane_height,
        row_height,
    );
    let mut actor = act!(text:
        font("miso"):
        settext(layout.text):
        align(0.5, 0.0):
        xy(0.0, -pane_height * 0.5 + row_height * 1.5):
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
        *attributes = build_body_attributes(content.as_str());
    }
    actor
}

fn build_overlay_leaderboard(
    entries: &[scores::LeaderboardEntry],
    pane_width: f32,
    single_player: bool,
    z: i16,
) -> Vec<Actor> {
    let rank_x = -(pane_width - OVERLAY_LB_GRID_W) * 0.5 - OVERLAY_LB_GRID_W * 0.5 + 32.0;
    let name_x = -(pane_width - OVERLAY_LB_GRID_W) * 0.5 - OVERLAY_LB_GRID_W * 0.5 + 100.0;
    let score_x = -(pane_width - OVERLAY_LB_GRID_W) * 0.5 + OVERLAY_LB_GRID_W * 0.5 - 2.0;
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
                format_machine_record_date(&entry.date),
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
        let y = first_row_y + OVERLAY_ROW_HEIGHT * idx as f32;
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

fn build_overlay_banner_and_song(song: &SongData, z: i16) -> Vec<Actor> {
    let mut children = Vec::with_capacity(2);
    if let Some(banner_path) = song.banner_path.as_ref() {
        let banner_key = banner_path.to_string_lossy().into_owned();
        children.push(shared_banner::sprite(
            banner_key, 0.0, 112.0, 418.0, 164.0, 0.34, z,
        ));
    }
    children.push(act!(text:
        font("miso"):
        settext(song.display_full_title(crate::config::get().translated_titles)):
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
    progress: &scores::ItlEventProgress,
    z: i16,
) -> Actor {
    let border_width = 2.0;
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
    children.push(build_header_text(
        header_name(progress.name.as_str(), progress.is_doubles),
        pane_width,
        -pane_height * 0.5 + 15.0,
        2,
    ));
    children.push(build_body_text(
        asset_manager,
        build_box_body(progress),
        pane_width - border_width,
        pane_height,
        UPPER_ROW_HEIGHT,
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
    progress: &scores::ItlEventProgress,
    page_idx: usize,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let header_y = -pane_height * 0.5 + 12.0;
    let header_bar_y = -pane_height * 0.5 + OVERLAY_ROW_HEIGHT * 0.5;
    let has_more_info = progress.overlay_pages.len() > 1;
    let single_player = pane_width > OVERLAY_LB_GRID_W;
    let mut children = Vec::with_capacity(11 + OVERLAY_LB_ROWS * 5);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width + border_width, pane_height + border_width + 1.0):
        diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
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
        diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
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
        header_name(progress.name.as_str(), progress.is_doubles),
        pane_width,
        header_y,
        4,
    ));
    children.push(act!(text:
        font(current_machine_font_key_for_text(FontRole::Header, "EX")):
        settext("EX"):
        align(0.5, 0.5):
        xy(pane_width * 0.5 - 18.0, header_y):
        zoom(0.5):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(4)
    ));
    match active_overlay_page(progress, page_idx) {
        Some(scores::ItlOverlayPage::Leaderboard(entries)) => {
            children.extend(build_overlay_leaderboard(
                entries.as_slice(),
                pane_width,
                single_player,
                4,
            ));
            if let Some(song) = song {
                children.extend(build_overlay_banner_and_song(song, 4));
            }
        }
        Some(scores::ItlOverlayPage::Text(text)) => children.push(build_body_text(
            asset_manager,
            text.clone(),
            pane_width,
            pane_height,
            OVERLAY_ROW_HEIGHT,
            4,
        )),
        None => children.push(build_body_text(
            asset_manager,
            build_overlay_body(progress),
            pane_width,
            pane_height,
            OVERLAY_ROW_HEIGHT,
            4,
        )),
    }
    if has_more_info {
        let nav_y = pane_height * 0.5 - OVERLAY_ROW_HEIGHT * 0.5;
        let icon_x = OVERLAY_PANE_NAV_WIDTH * 0.5 - 10.0;
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
            diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
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

pub fn build_itl_progress_box(
    asset_manager: &AssetManager,
    side: profile::PlayerSide,
    single_player: bool,
    progress: &scores::ItlEventProgress,
) -> Vec<Actor> {
    let upper_origin_x = match side {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let dir = if side == profile::PlayerSide::P1 {
        -1.0
    } else {
        1.0
    };
    let (center_x, center_y, pane_width, pane_height) = if single_player {
        (upper_origin_x - 381.0 * dir, 109.0, 156.0, 144.0)
    } else {
        (upper_origin_x + 211.0 * dir, 274.0, 118.0, 180.0)
    };
    vec![build_upper_panel(
        asset_manager,
        center_x,
        center_y,
        pane_width,
        pane_height,
        progress,
        104,
    )]
}

pub fn build_itl_event_overlay(
    asset_manager: &AssetManager,
    single_player: bool,
    song: Option<&SongData>,
    panels: &[(profile::PlayerSide, &scores::ItlEventProgress, usize)],
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
        } else if idx == 0 && *side == profile::PlayerSide::P1 {
            screen_center_x() - 160.0
        } else if idx == 0 && *side == profile::PlayerSide::P2 {
            screen_center_x() + 160.0
        } else if *side == profile::PlayerSide::P1 {
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
            progress,
            *page_idx,
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

#[cfg(test)]
mod tests {
    use super::{OVERLAY_ROW_HEIGHT, body_layout_with_measure};

    #[test]
    fn long_achievement_titles_wrap_without_shrinking_body_text() {
        let short = body_layout_with_measure(
            "Completed the \"Short Achievement\" Achievement!",
            330.0,
            360.0,
            OVERLAY_ROW_HEIGHT,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        let long = body_layout_with_measure(
            "Completed the \"This Achievement Title Is Extremely Long And Should Wrap Instead Of Shrinking The Popup Text\" Achievement!",
            330.0,
            360.0,
            OVERLAY_ROW_HEIGHT,
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
            180.0,
            OVERLAY_ROW_HEIGHT,
            |candidate| candidate.chars().count() as f32 * 8.0,
        );
        assert!(layout.zoom < 1.0);
    }
}
