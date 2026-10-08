//! The pack detail page.
//!
//! Two columns, as the original: a left column of width `floor(W * 0.38)`
//! holding the banner, the pack name, an eight-row fact table, the download
//! status line and its progress bar; and a right column holding the song list
//! with the DOWNLOAD PACK button sharing its header line.
//!
//! The song list is the reason this page exists. The catalogue publishes a
//! count and nothing else, so the titles, jackets, charters and per-song meters
//! come from the pack's own page on the site -- read once, on arrival here,
//! and cached so backing out and returning is free.

use std::borrow::Cow;

use deadlib_present::actors::Actor;
use deadsync_online::pack_page::{PackPage, PagePhase, SongRow};
use deadsync_online::stepmaniaonline::{InstallPhase, PackInfo};

use super::layout as lo;
use super::render::{
    ERROR_RGBA, INSTALLED_NOW, META_RGBA, Z_ART, Z_BADGE, Z_PANEL, Z_ROW_BG, Z_TEXT, accent,
    commify, format_bytes, format_date, meaningful, meter_color, page_for, push_art_or_status,
};
use super::spinner;
use super::state::{
    State, banner_expected, details_for, focused_pack, install_for, is_installed, smo_sync_of,
    song_art_key,
};
use crate::act;

/// The whole page. Everything else on the screen has already stood down, so
/// this owns y 48..440 across the full width.
pub(super) fn push_page(actors: &mut Vec<Actor>, state: &State) {
    let Some(pack) = focused_pack(state) else {
        return;
    };
    push_left_column(actors, state, pack);
    push_song_list(actors, state, pack);
}

// --- the left column ----------------------------------------------------------

fn push_left_column(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo) {
    let accent = accent(state);
    let left_w = lo::det_left_w();
    let cx = lo::det_left_cx();

    // Banner, fitted into a box 110 tall centred at y 105.
    push_art_or_status(
        actors,
        state,
        pack.id,
        lo::LIST_X + 10.0,
        50.0,
        left_w - 20.0,
        110.0,
        Z_ART,
        accent,
        banner_expected(state, pack.id),
    );

    actors.push(act!(text:
        font("miso"): settext(pack.name.clone()):
        align(0.5, 0.5): xy(cx, 178.0): zoom(0.85): maxwidth(left_w - 16.0): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));

    push_fact_table(actors, state, pack, left_w);
    push_download_status(actors, state, pack, cx, left_w);
    push_histogram(actors, state, pack, cx, left_w);
}

/// Eight label:value rows with a zebra band behind the odd ones.
fn push_fact_table(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo, left_w: f32) {
    let accent = accent(state);
    let table_w = left_w - 2.0 * (lo::DET_TAB_X - lo::LIST_X);
    let facts = facts(state, pack);

    for (row, (label, value)) in facts.iter().enumerate() {
        if row.is_multiple_of(2) {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(lo::DET_TAB_X - 6.0, lo::DET_TAB_Y + row as f32 * lo::DET_TAB_H):
                zoomto(table_w, lo::DET_TAB_H):
                diffuse(1.0, 1.0, 1.0, 0.035): z(Z_ROW_BG)
            ));
        }
        let y = lo::det_tab_y(row);
        actors.push(act!(text:
            font("miso"): settext((*label).to_owned()):
            align(0.0, 0.5): xy(lo::DET_TAB_X, y): zoom(0.46): horizalign(left):
            maxwidth(lo::DET_TAB_VX - lo::DET_TAB_X - 6.0):
            diffuse(0.52, 0.52, 0.52, 1.0): z(Z_TEXT)
        ));

        // A fact that is still coming says so where its value will land,
        // rather than showing an empty row that reads as "none".
        if value.is_empty() && pending(state, pack) {
            actors.push(spinner::accent_actor(
                lo::DET_TAB_VX + 8.0,
                y,
                15.0,
                Z_TEXT,
                accent,
            ));
            continue;
        }
        actors.push(act!(text:
            font("miso"): settext(value.clone()):
            align(0.0, 0.5): xy(lo::DET_TAB_VX, y): zoom(0.5): horizalign(left):
            maxwidth(table_w - (lo::DET_TAB_VX - lo::DET_TAB_X) - 4.0):
            diffuse(0.86, 0.86, 0.86, 1.0): z(Z_TEXT)
        ));
    }
}

/// Whether the pack page is still on its way, which is what turns the empty
/// facts into spinners rather than blanks.
fn pending(state: &State, pack: &PackInfo) -> bool {
    state.page.pack_id == pack.id && state.page.phase == PagePhase::Loading
}

/// The eight rows, in the original's order. A row whose answer is not known
/// yet is empty rather than absent, so the table keeps its shape.
fn facts(state: &State, pack: &PackInfo) -> [(&'static str, String); 8] {
    let details = details_for(state, pack);
    let page = page_for(state, pack);
    let lost = state.page.pack_id == pack.id && state.page.phase == PagePhase::Error;
    let unknown = || if lost { "--".to_owned() } else { String::new() };

    let charts = page
        .and_then(|p| p.chart_count.clone())
        .unwrap_or_else(unknown);
    let difficulty =
        page.and_then(PackPage::difficulty_span)
            .map_or_else(unknown, |(low, high)| {
                if low == high {
                    low.to_string()
                } else {
                    format!("{low} - {high}")
                }
            });
    // The pack page knows this for certain, per song. Everything else is a
    // guess from the catalogue's one coarse column, which is what made a pack
    // with a whole doubles set in it report "Singles".
    let style = match page.and_then(PackPage::style_label) {
        Some(label) => label.to_owned(),
        None => match pack.pack_type.as_deref().filter(|v| meaningful(v)) {
            Some(kind) if kind.eq_ignore_ascii_case("keyboard") => "Keyboard".to_owned(),
            _ if super::state::is_dedicated_doubles(state, pack) => "Doubles".to_owned(),
            _ => unknown(),
        },
    };
    // What the site says, and nothing more.
    //
    // This row used to claim "the engine corrects ITG sync", which is not
    // true: `MachinePackIniOffsets` is off by default and `DefaultSyncOffset`
    // is Null, so a stock install applies no pack correction at all. Saying
    // otherwise here told a player their old ITG-synced packs were handled
    // when they were about to play nine milliseconds early.
    let sync = smo_sync_of(pack).short().to_owned();
    let added = details
        .and_then(|d| d.date_added.as_deref())
        .map_or_else(unknown, |date| format_date(date, false));
    let authors = match page {
        Some(page) if !page.authors.is_empty() => {
            let shown = page.authors.len().min(2);
            let mut names = page.authors[..shown].join(", ");
            if page.authors.len() > shown {
                names.push_str(" and more");
            }
            names
        }
        _ => unknown(),
    };

    [
        ("Songs", commify(pack.song_count as usize)),
        ("Charts", charts),
        ("Size", format_bytes(pack.size_bytes)),
        ("Difficulty", difficulty),
        ("Style", style),
        ("Sync", sync),
        ("Added", added),
        ("Charts by", authors),
    ]
}

/// The download status line, and the bar under it while one is running.
fn push_download_status(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    cx: f32,
    left_w: f32,
) {
    let accent = accent(state);
    let y = lo::DET_TAB_Y + 8.0 * lo::DET_TAB_H + 12.0;

    let (text, rgba, fraction) = match install_for(state, pack) {
        Some(install) => match install.phase {
            InstallPhase::Queued => ("queued".to_owned(), META_RGBA, None),
            InstallPhase::Downloading => {
                let fraction = if install.total_bytes > 0 {
                    install.downloaded_bytes as f64 / install.total_bytes as f64
                } else {
                    0.0
                };
                (
                    format!(
                        "downloading  {}%   ({} / {})",
                        (fraction * 100.0) as u32,
                        format_bytes(install.downloaded_bytes),
                        format_bytes(install.total_bytes)
                    ),
                    [0.78, 0.78, 0.78, 1.0],
                    Some(fraction as f32),
                )
            }
            InstallPhase::Extracting => ("installing...".to_owned(), accent, Some(1.0)),
            InstallPhase::Installed => ("installed".to_owned(), INSTALLED_NOW, None),
            InstallPhase::Error => (
                format!(
                    "download failed: {}",
                    install.message.as_deref().unwrap_or("unknown error")
                ),
                ERROR_RGBA,
                None,
            ),
        },
        None if is_installed(state, pack) => (
            "already in your library".to_owned(),
            [0.78, 0.78, 0.78, 1.0],
            None,
        ),
        None => (String::new(), META_RGBA, None),
    };

    if !text.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(text):
            align(0.5, 0.5): xy(cx, y): zoom(0.5): maxwidth(left_w - 16.0): horizalign(center):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_TEXT)
        ));
    }

    if let Some(fraction) = fraction {
        let track_w = left_w - 24.0;
        actors.push(act!(quad:
            align(0.0, 0.0): xy(28.0, y + 4.0): zoomto(track_w, 5.0):
            diffuse(1.0, 1.0, 1.0, 0.12): z(Z_PANEL)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(28.0, y + 4.0): zoomto(track_w * fraction.clamp(0.0, 1.0), 5.0):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
        ));
    }
}

/// Charts per difficulty, along the bottom of the left column.
fn push_histogram(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo, cx: f32, left_w: f32) {
    let Some(page) = page_for(state, pack) else {
        return;
    };
    if page.meter_counts.is_empty() {
        return;
    }
    let hist_w = left_w - 24.0;
    let max_count = page.meter_counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bars = page.meter_counts.len();
    let bar_w = ((hist_w / bars as f32).floor() - 2.0).max(2.0);
    let x0 = 28.0 + (hist_w - bars as f32 * (bar_w + 2.0)) * 0.5;

    for (index, count) in page.meter_counts.iter().enumerate() {
        let meter = page.meter_labels.get(index).copied().unwrap_or(1);
        let rgba = meter_color(meter);
        let height = (*count as f32 / max_count * lo::BHIST_H).max(2.0);
        let x = x0 + index as f32 * (bar_w + 2.0);
        actors.push(act!(quad:
            align(0.0, 1.0): xy(x, lo::BHIST_Y): zoomto(bar_w, height):
            diffuse(rgba[0], rgba[1], rgba[2], 0.95): z(Z_ART)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::BHIST_Y + 3.0): zoomto(bar_w, 3.0):
            diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z_ART)
        ));
        // The meter number only fits under a wide enough bar; under a narrow
        // one it would overlap its neighbours, so it is dropped entirely.
        if bar_w >= 9.0 {
            actors.push(act!(text:
                font("miso"): settext(meter.to_string()):
                align(0.5, 0.5): xy(x + bar_w * 0.5, lo::BHIST_Y + 12.0): zoom(0.34):
                horizalign(center):
                diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z_TEXT)
            ));
        }
    }

    if let Some((low, high)) = page.difficulty_span() {
        actors.push(act!(text:
            font("miso"): settext(format!("charts per difficulty  ({low} - {high})")):
            align(0.5, 0.5): xy(cx, lo::BHIST_Y + 24.0): zoom(0.5): horizalign(center):
            maxwidth(left_w): diffuse(0.6, 0.6, 0.6, 1.0): z(Z_TEXT)
        ));
    }
}

// --- the song list ------------------------------------------------------------

fn push_song_list(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo) {
    let accent = accent(state);
    let x = lo::det_songs_x();
    let w = lo::det_songs_w();
    let page = page_for(state, pack);

    push_download_button(actors, state, pack, x, w);

    let header = match page {
        Some(page) if page.songs.is_empty() => "This pack lists no songs".to_owned(),
        Some(page) => {
            // After the count, as the original's header carries it: what the
            // last preview or song request came to, else what the sample is
            // doing, else the single song last asked for from this pack --
            // whichever row the cursor is on, where the original holds a
            // window up for it -- else the picked song's own.
            let install_line = |pack_id: u64, title: &str, artist: &str| {
                super::preview::song_install(state, pack_id, title, artist)
                    .and_then(|install| super::preview::song_install_line(state, install))
                    .map(Cow::Owned)
            };
            let status = state
                .preview_message
                .as_deref()
                .map(Cow::Borrowed)
                .or_else(|| super::preview::sample_label(state))
                .or_else(|| {
                    state
                        .watched_song
                        .as_ref()
                        .filter(|(pack_id, ..)| *pack_id == pack.id)
                        .and_then(|(pack_id, title, artist)| install_line(*pack_id, title, artist))
                })
                .or_else(|| {
                    page.songs.get(state.song_pick).and_then(|song| {
                        let artist = super::preview::request_artist(&page.songs, song);
                        install_line(pack.id, &song.title, artist)
                    })
                });
            let mut header = format!(
                "Songs  {} of {}",
                (state.song_pick + 1).min(page.songs.len()),
                page.songs.len()
            );
            if let Some(status) = status {
                header.push_str("   -   ");
                header.push_str(&status);
            }
            header
        }
        None => match state.page.phase {
            PagePhase::Error => "Could not load this pack's song list".to_owned(),
            _ => format!("Loading song list{}", spinner::ellipsis()),
        },
    };
    actors.push(act!(text:
        font("miso"): settext(header):
        align(0.0, 0.5): xy(x, lo::DL_BTN_Y): zoom(0.6): horizalign(left):
        maxwidth(w - lo::DL_BTN_W - 16.0):
        diffuse(0.7, 0.7, 0.7, 1.0): z(Z_TEXT)
    ));

    let Some(page) = page else {
        // Skeleton rows while the page is on its way, so the column reads as
        // filling in rather than as a blank half of the screen.
        if state.page.phase == PagePhase::Error {
            actors.push(act!(text:
                font("miso"): settext("SELECT to try again".to_owned()):
                align(0.0, 0.5): xy(x, lo::SONG_TOP + 20.0): zoom(0.5): horizalign(left):
                diffuse(1.0, 1.0, 1.0, 0.5): z(Z_TEXT)
            ));
            return;
        }
        for slot in 0..lo::SONG_ROWS {
            push_song_skeleton(actors, x, w, slot, accent);
        }
        return;
    };

    for slot in 0..lo::SONG_ROWS {
        let index = state.song_window + slot;
        let Some(song) = page.songs.get(index) else {
            continue;
        };
        push_song_row(actors, state, song, index, slot, x, w, accent);
    }

    // One scrollbar, 8px right of the column.
    if page.songs.len() > lo::SONG_ROWS {
        let length = lo::SONG_ROWS as f32 * lo::SONG_ROW_PITCH - 8.0;
        let frac = lo::SONG_ROWS as f32 / page.songs.len() as f32;
        let thumb = (length * frac).max(14.0);
        let span = (page.songs.len() - lo::SONG_ROWS) as f32;
        let progress = (state.song_window as f32 / span).clamp(0.0, 1.0);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x + w + 8.0, lo::SONG_TOP): zoomto(lo::SCROLL_W, length):
            diffuse(1.0, 1.0, 1.0, 0.10): z(Z_PANEL)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x + w + 8.0, lo::SONG_TOP + (length - thumb) * progress):
            zoomto(lo::SCROLL_W, thumb):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
        ));
    }
}

/// One song: jacket on the left, title and one combined grey subline beside it,
/// the meter string right-aligned and tinted by its hardest number.
#[allow(clippy::too_many_arguments)]
fn push_song_row(
    actors: &mut Vec<Actor>,
    state: &State,
    song: &SongRow,
    index: usize,
    slot: usize,
    x: f32,
    w: f32,
    accent: [f32; 4],
) {
    let y = lo::SONG_TOP + slot as f32 * lo::SONG_ROW_PITCH;
    let picked = index == state.song_pick;

    // Stripes are keyed to screen position, not to song index, so they do not
    // crawl as the list scrolls. The picked song dims while the cursor is on
    // the download button, so only one thing looks focused.
    let plate = if picked {
        let lit = if state.detail_on_button { 0.13 } else { 0.30 };
        [accent[0], accent[1], accent[2], lit]
    } else if slot.is_multiple_of(2) {
        [0.0, 0.0, 0.0, 0.52]
    } else {
        [0.0, 0.0, 0.0, 0.34]
    };
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 6.0, y): zoomto(w + 12.0, lo::SONG_ROW_H):
        diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_ROW_BG)
    ));

    let art_x = x + 4.0;
    let art_y = y + (lo::SONG_ROW_H - lo::SONG_ART_H) * 0.5;
    match song.image_url.as_deref() {
        Some(url) => push_art_or_status(
            actors,
            state,
            song_art_key(url),
            art_x,
            art_y,
            lo::SONG_ART_W,
            lo::SONG_ART_H,
            Z_ART,
            accent,
            true,
        ),
        // The site has no jacket for this song at all, which is a different
        // thing from one that has not arrived -- so it gets the placeholder
        // straight away rather than a wheel that never stops.
        None => actors.push(act!(sprite("content_browser/nobanner.png"):
            align(0.5, 0.5): xy(art_x + lo::SONG_ART_W * 0.5, art_y + lo::SONG_ART_H * 0.5):
            setsize(lo::SONG_ART_H, lo::SONG_ART_H):
            diffuse(1.0, 1.0, 1.0, 0.17): z(Z_ART)
        )),
    }

    let text_x = x + lo::SONG_ART_W + 16.0;
    let text_w = (w - (lo::SONG_ART_W + 16.0) - 92.0).max(60.0);
    actors.push(act!(text:
        font("miso"): settext(song.title.clone()):
        align(0.0, 0.5): xy(text_x, y + 15.0): zoom(0.66): maxwidth(text_w): horizalign(left):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));

    let subline = song.subline();
    if !subline.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(subline):
            align(0.0, 0.5): xy(text_x, y + 30.0): zoom(0.42): maxwidth(text_w): horizalign(left):
            diffuse(0.58, 0.58, 0.58, 1.0): z(Z_TEXT)
        ));
    }

    // A doubles chart in the row is worth calling out: it is the difference
    // between a pack a doubles player can use and one they cannot, and the
    // pack-level label cannot say which songs carry it.
    if song.styles.iter().any(|style| style.ends_with("-double")) {
        actors.push(act!(sprite("content_browser/doubles.png"):
            align(1.0, 0.5): xy(x + w, y + lo::SONG_ROW_H - 12.0): setsize(20.0, 10.0):
            diffuse(accent[0], accent[1], accent[2], 0.85): z(Z_BADGE)
        ));
    }

    // The row whose sample is playing says so, with a few bars keyed to the
    // song's beat -- the original's row equalizer, smaller -- and the row
    // whose sample is on its way, with how much has arrived, in the same
    // corner.
    if let Some(preview) = state.preview.as_ref()
        && preview.row == index
        && !preview.closing
    {
        let bottom = y + lo::SONG_ROW_H - 6.0;
        if preview.playing {
            let beat = super::preview::beat(state).unwrap_or(0.0);
            for bar in 0..ROW_EQ_BARS {
                let phase = beat * std::f32::consts::TAU + bar as f32 * 1.3;
                let height = 3.0 + 9.0 * (0.5 + 0.5 * phase.sin());
                actors.push(act!(quad:
                    align(0.0, 1.0):
                    xy(x + w - ROW_MARK_RIGHT - ROW_EQ_W + bar as f32 * 4.0, bottom):
                    zoomto(3.0, height):
                    diffuse(accent[0], accent[1], accent[2], 0.9): z(Z_BADGE)
                ));
            }
        } else {
            let progress =
                super::preview::snapshot_for(state, preview).and_then(|snapshot| snapshot.progress);
            push_sample_bar(actors, x + w - ROW_MARK_RIGHT, bottom, progress, accent);
        }
    }

    if !song.meters.is_empty() {
        // The whole string is tinted by its single hardest number, which is
        // the one thing a reader scanning a pack actually wants.
        let rgba = meter_color(song.top_meter());
        actors.push(act!(text:
            font("miso"): settext(song.meters.clone()):
            align(1.0, 0.5): xy(x + w, y + 15.0): zoom(0.74): horizalign(right):
            diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z_BADGE)
        ));
    }
}

/// The row equalizer's bars, and where the row's sample marks end.
const ROW_EQ_BARS: usize = 6;
const ROW_EQ_W: f32 = ROW_EQ_BARS as f32 * 4.0 - 1.0;
const ROW_MARK_RIGHT: f32 = 127.0;
/// The loading bar, the original's `PROG_W` by 5.
const SAMPLE_BAR_W: f32 = 118.0;
const SAMPLE_BAR_H: f32 = 5.0;

/// How much of a sample has arrived, ending at `right` on the row's `bottom`:
/// a fill once the size is known, and before then a block that travels rather
/// than a bar that would be inventing a number, as the original's does.
fn push_sample_bar(
    actors: &mut Vec<Actor>,
    right: f32,
    bottom: f32,
    progress: Option<f32>,
    accent: [f32; 4],
) {
    let track_x = right - SAMPLE_BAR_W;
    actors.push(act!(quad:
        align(0.0, 1.0): xy(track_x, bottom): zoomto(SAMPLE_BAR_W, SAMPLE_BAR_H):
        diffuse(1.0, 1.0, 1.0, 0.16): z(Z_BADGE)
    ));
    let (fill_x, fill_w) = match progress {
        Some(fraction) => (track_x, (SAMPLE_BAR_W * fraction.clamp(0.0, 1.0)).max(1.0)),
        None => {
            let block = SAMPLE_BAR_W * 0.3;
            let mut at = (spinner::seconds() * 0.7) % 2.0;
            if at > 1.0 {
                at = 2.0 - at;
            }
            (track_x + at * (SAMPLE_BAR_W - block), block)
        }
    };
    actors.push(act!(quad:
        align(0.0, 1.0): xy(fill_x, bottom): zoomto(fill_w, SAMPLE_BAR_H):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_BADGE + 1)
    ));
}

fn push_song_skeleton(actors: &mut Vec<Actor>, x: f32, w: f32, slot: usize, accent: [f32; 4]) {
    let y = lo::SONG_TOP + slot as f32 * lo::SONG_ROW_PITCH;
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 6.0, y): zoomto(w + 12.0, lo::SONG_ROW_H):
        diffuse(0.0, 0.0, 0.0, if slot.is_multiple_of(2) { 0.52 } else { 0.34 }): z(Z_ROW_BG)
    ));
    let art_x = x + 4.0;
    let art_y = y + (lo::SONG_ROW_H - lo::SONG_ART_H) * 0.5;
    actors.push(spinner::accent_actor(
        art_x + lo::SONG_ART_W * 0.5,
        art_y + lo::SONG_ART_H * 0.5,
        22.0,
        Z_ART,
        accent,
    ));
    let text_x = x + lo::SONG_ART_W + 16.0;
    actors.push(act!(quad:
        align(0.0, 0.0): xy(text_x, y + 10.0): zoomto(w * 0.4, 6.0):
        diffuse(1.0, 1.0, 1.0, 0.07): z(Z_ART)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(text_x, y + 26.0): zoomto(w * 0.25, 4.0):
        diffuse(1.0, 1.0, 1.0, 0.05): z(Z_ART)
    ));
}

/// The button that shares the song header's line, and says what the pack's
/// download would do. It acts only while it has the cursor -- UP from the
/// first song -- and is lit only then, as the original's is: START on a song
/// opens that song's menu, so a button lit all the time would claim START.
fn push_download_button(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo, x: f32, w: f32) {
    let accent = accent(state);
    let button_x = x + w - lo::DL_BTN_W;
    let top = lo::DL_BTN_Y - lo::DL_BTN_H * 0.5;

    let (label, armed) = download_label(state, pack);
    let focused = state.detail_on_button;
    let plate = match (focused, armed) {
        (true, true) => [accent[0], accent[1], accent[2], 0.85],
        (true, false) => [accent[0], accent[1], accent[2], 0.45],
        (false, true) => [1.0, 1.0, 1.0, 0.14],
        (false, false) => [1.0, 1.0, 1.0, 0.07],
    };
    let (icon, ink) = match (focused, armed) {
        (true, _) => ([0.08, 0.08, 0.08, 1.0], [0.08, 0.08, 0.08, 1.0]),
        (false, true) => ([1.0, 1.0, 1.0, 0.85], [1.0, 1.0, 1.0, 0.9]),
        (false, false) => ([1.0, 1.0, 1.0, 0.4], [1.0, 1.0, 1.0, 0.45]),
    };

    actors.push(act!(quad:
        align(0.0, 0.0): xy(button_x, top): zoomto(lo::DL_BTN_W, lo::DL_BTN_H):
        diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_PANEL)
    ));
    // A lit edge, so it reads as a control rather than a coloured strip --
    // until it has the cursor, when the plate says so on its own.
    if !focused {
        actors.push(act!(quad:
            align(0.0, 0.0): xy(button_x, top): zoomto(lo::DL_BTN_W, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.22): z(Z_ROW_BG)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(button_x, top + lo::DL_BTN_H - 1.0): zoomto(lo::DL_BTN_W, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.22): z(Z_ROW_BG)
        ));
    }
    actors.push(act!(sprite("content_browser/download.png"):
        align(0.5, 0.5): xy(button_x + 16.0, lo::DL_BTN_Y): setsize(13.0, 13.0):
        diffuse(icon[0], icon[1], icon[2], icon[3]): z(Z_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(label):
        align(0.0, 0.5): xy(button_x + 30.0, lo::DL_BTN_Y): zoom(0.5): horizalign(left):
        maxwidth(lo::DL_BTN_W - 40.0):
        diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_TEXT)
    ));
}

/// What the button says, and whether START on it would start anything.
pub(super) fn download_label(state: &State, pack: &PackInfo) -> (String, bool) {
    if let Some(install) = install_for(state, pack) {
        return match install.phase {
            InstallPhase::Queued => ("QUEUED".to_owned(), false),
            InstallPhase::Downloading => {
                let pct = if install.total_bytes > 0 {
                    (install.downloaded_bytes as f64 / install.total_bytes as f64 * 100.0) as u32
                } else {
                    0
                };
                (format!("DOWNLOADING   {pct}%"), false)
            }
            InstallPhase::Extracting => ("INSTALLING...".to_owned(), false),
            InstallPhase::Installed => ("INSTALLED - RELOAD SONGS".to_owned(), false),
            InstallPhase::Error => ("DOWNLOAD FAILED - RETRY".to_owned(), true),
        };
    }
    if is_installed(state, pack) {
        return ("IN YOUR LIBRARY".to_owned(), false);
    }
    (
        format!("DOWNLOAD PACK   {}", format_bytes(pack.size_bytes)),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::content_browser::state;
    use deadsync_chart::song::SyncPref;
    use deadsync_online::stepmaniaonline::{CatalogPhase, Snapshot};
    use std::sync::Arc;

    fn pack() -> PackInfo {
        PackInfo::new(
            7,
            "Some Pack".to_owned(),
            42,
            700 * 1024 * 1024,
            None,
            None,
            None,
            None,
        )
    }

    fn with_pack() -> State {
        let mut base = state::init();
        base.snapshot = Arc::new(Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(vec![pack()]),
            revision: 1,
            message: None,
            installs: Vec::new(),
        });
        base.results = vec![0];
        base
    }

    /// Every row keeps its slot even before the pack page lands, so the table
    /// does not jump about as answers arrive one at a time.
    #[test]
    fn the_fact_table_always_has_eight_rows() {
        let state = with_pack();
        let rows = facts(&state, &pack());
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0], ("Songs", "42".to_owned()));
        assert_eq!(rows[2], ("Size", "700.0 MB".to_owned()));
        // and the ones the pack page would answer are blank, not wrong
        assert!(rows[1].1.is_empty(), "charts wait for the page");
        assert!(rows[3].1.is_empty(), "difficulty waits for the page");
    }

    /// A page that failed says so with a dash rather than staying blank, which
    /// would read as "this pack has none".
    #[test]
    fn a_lost_pack_page_prints_dashes_rather_than_blanks() {
        let mut state = with_pack();
        state.page = Arc::new(deadsync_online::pack_page::PageSnapshot {
            phase: PagePhase::Error,
            pack_id: 7,
            page: None,
            message: Some("timed out".to_owned()),
            revision: 1,
        });
        let rows = facts(&state, &pack());
        assert_eq!(rows[1].1, "--");
        assert_eq!(rows[3].1, "--");
        assert_eq!(rows[7].1, "--");
    }

    /// The button is the only armed control on the page, and it disarms the
    /// moment a download is running -- pressing START twice must not queue two.
    #[test]
    fn the_download_button_says_what_start_would_do() {
        let mut state = with_pack();
        let (label, armed) = download_label(&state, &pack());
        assert_eq!(label, "DOWNLOAD PACK   700.0 MB");
        assert!(armed);

        state.installed = vec![state::InstalledPack {
            name: "Some Pack".to_owned(),
            lower: "some pack".to_owned(),
            songs: 42,
            sync: SyncPref::Default,
            banner: None,
        }];
        let (label, armed) = download_label(&state, &pack());
        assert_eq!(label, "IN YOUR LIBRARY");
        assert!(!armed);
    }

    /// The button drops its edge lines and takes the accent only while the
    /// cursor is on it, so the page shows one focus at a time.
    #[test]
    fn the_download_button_lights_only_with_the_cursor_on_it() {
        let mut state = with_pack();
        let drawn = |state: &State| {
            let mut actors = Vec::new();
            push_download_button(&mut actors, state, &pack(), 300.0, 500.0);
            actors.len()
        };
        let resting = drawn(&state);
        state.detail_on_button = true;
        assert_eq!(drawn(&state), resting - 2, "no edge lines while focused");
    }

    /// The single song last asked for keeps its line in the header wherever
    /// the cursor goes, as the original's window stays up for it; what a
    /// preview came to says its piece first.
    #[test]
    fn the_header_follows_the_song_asked_for() {
        use deadsync_online::pack_page::PageSnapshot;
        use deadsync_online::smo_songs::{SongInstall, SongInstallPhase, SongInstallsSnapshot};
        let mut state = with_pack();
        state.page = Arc::new(PageSnapshot {
            phase: PagePhase::Ready,
            pack_id: 7,
            page: Some(Arc::new(PackPage {
                songs: ["Song A", "Song B"]
                    .iter()
                    .map(|title| SongRow {
                        title: (*title).to_owned(),
                        ..SongRow::default()
                    })
                    .collect(),
                ..PackPage::default()
            })),
            message: None,
            revision: 1,
        });
        state.song_installs = Arc::new(SongInstallsSnapshot {
            installs: Arc::from(vec![SongInstall {
                pack_id: 7,
                title: "Song B".to_owned(),
                artist: String::new(),
                group: deadsync_online::smo_songs::SINGLES_GROUP.to_owned(),
                phase: SongInstallPhase::Error,
                downloaded_bytes: 0,
                total_bytes: None,
                message: Some("HTTP 404".to_owned()),
            }]),
            revision: 1,
        });
        state.watched_song = Some((7, "Song B".to_owned(), String::new()));
        let header = |state: &State| {
            let mut actors = Vec::new();
            push_song_list(&mut actors, state, &pack());
            actors
                .iter()
                .find_map(|actor| match actor {
                    Actor::Text { content, .. } if content.as_str().starts_with("Songs  ") => {
                        Some(content.as_str().to_owned())
                    }
                    _ => None,
                })
                .expect("a header")
        };
        assert_eq!(
            header(&state),
            "Songs  1 of 2   -   could not get Song B: HTTP 404",
            "the cursor is on Song A"
        );
        state.preview_message = Some("no sample for this song".to_owned());
        assert_eq!(
            header(&state),
            "Songs  1 of 2   -   no sample for this song"
        );
    }

    /// The row a sample is coming in for carries the original's loading bar,
    /// and once the sample plays, the equalizer in its place.
    #[test]
    fn the_sampled_row_shows_loading_then_playing() {
        use deadsync_online::pack_page::PageSnapshot;
        use deadsync_online::smo_songs::{PreviewPhase, PreviewSnapshot};
        let mut state = with_pack();
        state.page = Arc::new(PageSnapshot {
            phase: PagePhase::Ready,
            pack_id: 7,
            page: Some(Arc::new(PackPage {
                songs: vec![SongRow {
                    title: "Song A".to_owned(),
                    ..SongRow::default()
                }],
                ..PackPage::default()
            })),
            message: None,
            revision: 1,
        });
        super::super::preview::start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        state.pending_songs.clear();
        state.song_preview = Arc::new(PreviewSnapshot {
            phase: PreviewPhase::Loading,
            pack_id: 7,
            title: "Song A".to_owned(),
            progress: Some(0.5),
            ..PreviewSnapshot::default()
        });
        let widths = |state: &State| {
            let mut actors = Vec::new();
            push_song_list(&mut actors, state, &pack());
            actors
                .iter()
                .filter_map(|actor| match actor {
                    // A quad's size is its zoom, as StepMania's zoomto is.
                    Actor::Sprite { scale, z, .. } if *z >= Z_BADGE && scale[1] <= SAMPLE_BAR_H => {
                        Some(scale[0])
                    }
                    _ => None,
                })
                .collect::<Vec<f32>>()
        };
        assert_eq!(
            widths(&state),
            vec![SAMPLE_BAR_W, SAMPLE_BAR_W * 0.5],
            "the track, half filled"
        );

        let mut ready = (*state.song_preview).clone();
        ready.phase = PreviewPhase::Ready;
        ready.audio_path = Some(std::path::PathBuf::from("cache/preview.ogg"));
        state.song_preview = Arc::new(ready);
        super::super::preview::sync(&mut state);
        assert!(
            widths(&state).iter().all(|w| (*w - 3.0).abs() < 1e-6),
            "the equalizer's bars instead"
        );
    }
}
