//! Drawing, ported from the ITGmania Content Browser's own screen files.
//!
//! Draw order matters and is the original's: a full-screen scrim, the opaque
//! band along the bottom, the tab strip, whichever strip this tab puts under
//! it, the list, the info pane and the footer. Two things draw over all of
//! that and hide everything else while they do -- the detail page, and the
//! search prompt.

use crate::color;
use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadsync_online::itgdb::ItgdbPhase;
use deadsync_online::pack_page::PagePhase;
use deadsync_online::popular_packs::PopularPhase;
use deadsync_online::smo_details::DetailsPhase;
use deadsync_online::stepmaniaonline::{CatalogPhase, InstallPhase, PackInfo};

use super::chart_window;
use super::detail;
use super::layout as lo;
use super::spinner;
use super::state::{
    FEATURED_VISIBLE, ReloadPrompt, SmoSync, State, TABS, Tab, Zone, awaiting_order, band,
    band_count, band_showing, banner_expected, banner_is_lost, banner_key, beginner_building,
    beginner_showing, details_for, doubles_column, doubles_cursor, doubles_showing,
    featured_heading, focused_pack, grid_showing, install_for, installed_catalog_entry,
    installed_page, installed_pages, installed_showing, installed_smo_sync, is_installed,
    list_page, more_mark, row_count, search_capped, search_field_showing, search_running,
    selected_pack, smo_sync_of, tab, total_pages, visible_rows, window_start, year_at,
    year_indexing, year_label, years_showing,
};
use super::sync_dialog::{self, PackCheck, SHIFT_MS, SyncAction, SyncDialog, SyncStep};
use crate::act;
use crate::screens::components::shared::{banner, transitions, visual_style_bg};
use deadsync_chart::song::SyncPref;

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

// Draw order, back to front.
pub(super) const Z_SCRIM: i16 = 2;
pub(super) const Z_PANEL: i16 = 8;
pub(super) const Z_ROW_BG: i16 = 12;
pub(super) const Z_ART: i16 = 16;
pub(super) const Z_TEXT: i16 = 20;
pub(super) const Z_BADGE: i16 = 24;
const Z_MODAL_SCRIM: i16 = 80;
const Z_MODAL_PANEL: i16 = 84;
const Z_MODAL_TEXT: i16 = 90;
/// Just under the shared pack sync review, which draws at 1496 and up.
const Z_PACK_SYNC_SCRIM: i16 = 1490;

/// The original's greys, kept as named values because several are close and
/// swapping two of them is invisible in a diff and obvious on screen.
const RULE_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 0.16];
pub(super) const META_RGBA: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const READOUT_RGBA: [f32; 4] = [0.62, 0.62, 0.62, 1.0];
const TAB_IDLE_LABEL: [f32; 4] = [0.62, 0.62, 0.62, 1.0];
const TAB_IDLE_ICON: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
const TAB_FOCUS_INK: [f32; 4] = [0.08, 0.08, 0.08, 1.0];
const FOOTER_RGBA: [f32; 4] = [0.75, 0.75, 0.75, 1.0];
/// A finished download this session, versus a group that was already there.
pub(super) const INSTALLED_NOW: [f32; 4] = [0.4, 1.0, 0.4, 1.0];
const IN_LIBRARY: [f32; 4] = [0.45, 0.8, 0.45, 1.0];
pub(super) const ERROR_RGBA: [f32; 4] = [1.0, 0.4, 0.4, 1.0];

/// Simply Love's active colour, which is what the original accents with --
/// not a fixed blue.
pub(super) fn accent(state: &State) -> [f32; 4] {
    color::simply_love_rgba(state.active_color_index)
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let w = lo::width();
    let h = lo::height();

    actors.reserve(lo::ROWS * 8 + 120);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    // The scrim the whole browser sits on, then the opaque band the footer
    // lives against.
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.66): z(Z_SCRIM)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, lo::CONTENT_BOT): zoomto(w, h - lo::CONTENT_BOT):
        diffuse(0.0, 0.0, 0.0, 1.0): z(Z_SCRIM)
    ));

    push_header(actors, w);

    // The original refuses to lay out below 700 wide rather than squashing:
    // the tab strip alone needs 851px and nothing here reflows.
    if lo::too_narrow() {
        push_too_narrow(actors, w, h);
        return;
    }

    // The detail page fills the screen, and everything else stands down.
    if state.zone == Zone::Detail {
        detail::push_page(actors, state);
        chart_window::push_window(actors, state, w);
        push_footer(actors, state, w);
        chart_window::push_song_menu(actors, state, w, h);
        return;
    }

    push_tabs(actors, state);
    push_readout(actors, state, w);

    if search_field_showing(state) {
        push_search_field(actors, state, asset_manager, w);
    } else if band_showing(state) {
        push_context_band(actors, state, w);
    } else if years_showing(state) {
        push_year_strip(actors, state, w);
    } else if grid_showing(state) {
        push_featured(actors, state, w);
    }

    if doubles_showing(state) {
        push_doubles(actors, state);
    } else if installed_showing(state) {
        push_installed(actors, state, w);
    } else {
        push_list(actors, state);
        push_info_pane(actors, state);
    }
    push_footer(actors, state, w);

    // Over everything, because it is the only question on the screen while it
    // is up and the only thing any key will answer.
    if let Some(name) = state.removing.as_deref() {
        push_remove_confirm(actors, state, name, w, h);
    } else if let Some(dialog) = state.sync_dialog.as_ref() {
        push_sync_dialog(actors, state, dialog, w, h);
    } else if let Some(prompt) = state.reload_prompt.as_ref() {
        push_reload_dialog(actors, state, prompt, w, h);
    }

    // DeadSync's pack sync review, over everything once a measure starts.
    if sync_dialog::overlay_visible(state) {
        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.8): z(Z_PACK_SYNC_SCRIM)
        ));
        crate::screens::pack_sync::push_overlay(
            actors,
            &state.pack_sync_overlay,
            state.active_color_index,
            visual_policy.machine_font,
        );
    }
}

/// The original's reload dialog: what this session added, and whether to
/// rescan the library now so it shows up in the song wheel.
fn push_reload_dialog(
    actors: &mut Vec<Actor>,
    state: &State,
    prompt: &ReloadPrompt,
    w: f32,
    h: f32,
) {
    const CHOICE_W: f32 = 180.0;
    const CHOICE_H: f32 = 30.0;
    let accent = accent(state);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let panel_w = (w * 0.62).min(520.0);
    let panel_h = 150.0;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.62): z(Z_MODAL_SCRIM)
    ));
    push_bordered(
        actors,
        cx,
        cy,
        panel_w,
        panel_h,
        [0.05, 0.05, 0.07, 1.0],
        Z_MODAL_PANEL,
    );
    // The original's words for what changed: it said "New Packs Installed"
    // over a single song, because it only ever knew that something had.
    let packs = state
        .installed_dirs
        .iter()
        .filter(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| name != deadsync_online::smo_songs::SINGLES_GROUP)
        })
        .count();
    let songs = super::preview::songs_added(state);
    let title = if packs == 0 && songs > 0 {
        "NEW SONGS INSTALLED"
    } else {
        "NEW CONTENT INSTALLED"
    };
    actors.push(act!(text:
        font("wendy"): settext(title.to_owned()):
        align(0.5, 0.5): xy(cx, cy - 50.0): zoom(0.5): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MODAL_TEXT)
    ));

    if prompt.reloading {
        let line = match prompt.progress.as_ref() {
            Some((done, total, pack)) => format!("{done} of {total} songs  -  {pack}"),
            None => "Starting...".to_owned(),
        };
        spinner::with_label(
            actors,
            cx - 70.0,
            cy - 10.0,
            spinner::SMALL_PX,
            "Reloading songs",
            Z_MODAL_TEXT,
            accent,
        );
        actors.push(act!(text:
            font("miso"): settext(line):
            align(0.5, 0.5): xy(cx, cy + 22.0): zoom(0.5): horizalign(center):
            maxwidth(panel_w - 40.0):
            diffuse(0.75, 0.75, 0.75, 1.0): z(Z_MODAL_TEXT)
        ));
        return;
    }

    let mut bits: Vec<String> = Vec::with_capacity(2);
    if packs > 0 {
        bits.push(format!(
            "{packs} {}",
            if packs == 1 { "pack" } else { "packs" }
        ));
    }
    if songs > 0 {
        bits.push(format!(
            "{songs} {}",
            if songs == 1 { "song" } else { "songs" }
        ));
    }
    let what = if packs == 0 && songs > 0 {
        "song"
    } else {
        "content"
    };
    let added = if bits.is_empty() {
        "Library changed.".to_owned()
    } else {
        format!("{} added.", bits.join(" and "))
    };
    actors.push(act!(text:
        font("miso"): settext(added):
        align(0.5, 0.5): xy(cx, cy - 26.0): zoom(0.6): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(format!("Reload songs now so the new {what} shows up?")):
        align(0.5, 0.5): xy(cx, cy - 8.0): zoom(0.5): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(0.85, 0.85, 0.85, 1.0): z(Z_MODAL_TEXT)
    ));
    for (index, label) in ["Reload songs", "Not yet"].into_iter().enumerate() {
        let picked = prompt.choice == index;
        let x = cx + (index as f32 - 0.5) * (CHOICE_W + 12.0);
        let plate = if picked {
            [accent[0], accent[1], accent[2], 0.9]
        } else {
            [1.0, 1.0, 1.0, 0.07]
        };
        let ink = if picked {
            [0.08, 0.08, 0.08, 1.0]
        } else {
            [0.75, 0.75, 0.75, 1.0]
        };
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x, cy + 24.0): zoomto(CHOICE_W, CHOICE_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_MODAL_PANEL + 2)
        ));
        actors.push(act!(text:
            font("miso"): settext(label.to_owned()):
            align(0.5, 0.5): xy(x, cy + 24.0): zoom(0.58): horizalign(center):
            diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_MODAL_TEXT)
        ));
    }
    actors.push(act!(text:
        font("miso"): settext("LEFT/RIGHT choose    START go    BACK cancel".to_owned()):
        align(0.5, 0.5): xy(cx, cy + 56.0): zoom(0.5): horizalign(center):
        diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]):
        z(Z_MODAL_TEXT)
    ));
}

/// The sync dialog: what the pack says about itself, then DeadSync's ways of
/// changing its sync, a confirm before every simfile is rewritten, and a panel
/// that stays up while the rewrite runs.
fn push_sync_dialog(actors: &mut Vec<Actor>, state: &State, dialog: &SyncDialog, w: f32, h: f32) {
    const ROW_H: f32 = 40.0;
    const ROW_GAP: f32 = 6.0;
    let accent = accent(state);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let panel_w = (w * 0.72).min(600.0);
    let offered = sync_dialog::actions(state);
    let to_null = dialog.shift_to_null();
    let (from, to) = if to_null {
        ("ITG", "NULL")
    } else {
        ("NULL", "ITG")
    };
    let way = if to_null { "later" } else { "earlier" };
    let panel_h = match dialog.step {
        SyncStep::Choose => 170.0 + offered.len() as f32 * (ROW_H + ROW_GAP),
        SyncStep::ConfirmShift | SyncStep::Working { .. } => 190.0,
    };
    let top = cy - panel_h * 0.5;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.62): z(Z_MODAL_SCRIM)
    ));
    push_bordered(
        actors,
        cx,
        cy,
        panel_w,
        panel_h,
        [0.05, 0.05, 0.07, 1.0],
        Z_MODAL_PANEL,
    );

    let title = match dialog.step {
        SyncStep::Choose => "SYNC THIS PACK".to_owned(),
        SyncStep::ConfirmShift => format!("SHIFT {from} TO {to}?"),
        SyncStep::Working { .. } => format!("SHIFTING {from} TO {to}"),
    };
    actors.push(act!(text:
        font("wendy"): settext(title):
        align(0.5, 0.5): xy(cx, top + 24.0): zoom(0.46): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MODAL_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(dialog.group.clone()):
        align(0.5, 0.5): xy(cx, top + 48.0): zoom(0.66): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));

    let body_line = |actors: &mut Vec<Actor>, y: f32, text: String, rgba: [f32; 4]| {
        actors.push(act!(text:
            font("miso"): settext(text):
            align(0.5, 0.5): xy(cx, y): zoom(0.46): horizalign(center):
            maxwidth(panel_w - 40.0):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_MODAL_TEXT)
        ));
    };
    let hint = |actors: &mut Vec<Actor>, text: &str| {
        actors.push(act!(text:
            font("miso"): settext(text.to_owned()):
            align(0.5, 0.5): xy(cx, top + panel_h - 18.0): zoom(0.5): horizalign(center):
            diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]):
            z(Z_MODAL_TEXT)
        ));
    };
    let songs = if dialog.songs == 1 {
        "1 song".to_owned()
    } else {
        format!("{} songs", dialog.songs)
    };

    match dialog.step {
        SyncStep::Working { .. } => {
            spinner::with_label(
                actors,
                cx - 70.0,
                top + 92.0,
                spinner::SMALL_PX,
                "Moving offsets",
                Z_MODAL_TEXT,
                accent,
            );
            body_line(
                actors,
                top + 124.0,
                format!("{songs}, {SHIFT_MS} ms {way} each"),
                [0.75, 0.75, 0.75, 1.0],
            );
            return;
        }
        SyncStep::ConfirmShift => {
            body_line(
                actors,
                top + 82.0,
                format!("Every offset in {songs} moves {SHIFT_MS} ms {way}."),
                [0.86, 0.86, 0.86, 1.0],
            );
            body_line(
                actors,
                top + 104.0,
                format!(
                    "Each simfile keeps a .old copy of itself, and the pack is recorded as {to}"
                ),
                [0.66, 0.66, 0.66, 1.0],
            );
            body_line(
                actors,
                top + 120.0,
                "in its Pack.ini, so the same shift is never made twice.".to_owned(),
                [0.66, 0.66, 0.66, 1.0],
            );
            hint(actors, "START shift    BACK go back");
            return;
        }
        SyncStep::Choose => {}
    }

    // What the pack says about itself, and what the site says, both named:
    // a pack can declare NULL while the site says nothing about it.
    let declares = match dialog.declared {
        SyncPref::Itg => "its Pack.ini says ITG",
        SyncPref::Null => "its Pack.ini says NULL",
        SyncPref::Default => "it has no sync in a Pack.ini",
    };
    let smo = state
        .installed
        .iter()
        .find(|entry| entry.name == dialog.group)
        .map_or(SmoSync::Unknown, |entry| installed_smo_sync(state, entry));
    body_line(
        actors,
        top + 68.0,
        format!("{songs}   -   {declares}   -   {}", smo.short()),
        [0.7, 0.7, 0.7, 1.0],
    );

    let row_w = panel_w - 40.0;
    let mut y = top + 88.0 + ROW_H * 0.5;
    for &offer in offered {
        let picked = offer == dialog.action;
        let (label, blurb) = match offer {
            SyncAction::Measure => (
                "MEASURE WITH NULL-OR-DIE".to_owned(),
                "measure every song against its music, review the offsets, then save".to_owned(),
            ),
            SyncAction::Shift => (
                format!("SHIFT {from} TO {to}"),
                if to_null {
                    format!(
                        "move every offset {SHIFT_MS} ms later, for a pack synced the old ITG way"
                    )
                } else {
                    format!("move every offset {SHIFT_MS} ms earlier and record the pack as ITG")
                },
            ),
            SyncAction::PackIni => (
                format!(
                    "RECORD IN PACK.INI ONLY:   {}",
                    if dialog.record == SyncPref::Itg {
                        "< ITG >"
                    } else {
                        "< NULL >"
                    }
                ),
                "leave the simfiles alone; the engine applies it while Pack.ini Offsets is on"
                    .to_owned(),
            ),
        };
        let usable = dialog.usable();
        let plate = match (picked, usable) {
            (true, true) => [accent[0], accent[1], accent[2], 0.9],
            (true, false) => [1.0, 1.0, 1.0, 0.16],
            (false, _) => [1.0, 1.0, 1.0, 0.07],
        };
        let ink = if picked && usable {
            [0.08, 0.08, 0.08, 1.0]
        } else if usable {
            [0.78, 0.78, 0.78, 1.0]
        } else {
            [0.45, 0.45, 0.45, 1.0]
        };
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, y): zoomto(row_w, ROW_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_MODAL_PANEL + 2)
        ));
        actors.push(act!(text:
            font("miso"): settext(label):
            align(0.5, 0.5): xy(cx, y - 8.0): zoom(0.56): horizalign(center):
            maxwidth(row_w - 20.0):
            diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_MODAL_TEXT)
        ));
        actors.push(act!(text:
            font("miso"): settext(blurb):
            align(0.5, 0.5): xy(cx, y + 9.0): zoom(0.4): horizalign(center):
            maxwidth(row_w - 20.0):
            diffuse(ink[0], ink[1], ink[2], if picked { 0.8 } else { 0.6 }): z(Z_MODAL_TEXT)
        ));
        y += ROW_H + ROW_GAP;
    }

    // What the choice leaves behind -- or why there is no choice at all.
    let (note, rgba) = if let PackCheck::Refused(reason) = &dialog.check {
        (
            format!("Its sync cannot be changed here: {reason}."),
            [0.97, 0.78, 0.30, 1.0],
        )
    } else if !dialog.usable() {
        (
            "Checking the pack's folder...".to_owned(),
            [0.6, 0.6, 0.6, 1.0],
        )
    } else {
        let note = match dialog.action {
            SyncAction::Measure => {
                "Each saved simfile keeps a .old copy, and the pack is then recorded as NULL."
                    .to_owned()
            }
            SyncAction::Shift => {
                format!("Each simfile keeps a .old copy, and the pack is then recorded as {to}.")
            }
            SyncAction::PackIni => "Only the Pack.ini changes.".to_owned(),
        };
        (note, [0.6, 0.6, 0.6, 1.0])
    };
    body_line(actors, y - ROW_GAP + 12.0, note, rgba);
    hint(
        actors,
        if dialog.action == SyncAction::PackIni {
            "UP/DOWN choose    LEFT/RIGHT ITG or NULL    START save    BACK close"
        } else {
            "UP/DOWN choose    START go    BACK close"
        },
    );
}

/// The one thing here that cannot be undone, asked about plainly.
///
/// It names the pack and says what will happen to it in the words that
/// actually describe it -- files removed from the disk -- rather than
/// "remove", which sounds like it removes it from a list.
fn push_remove_confirm(actors: &mut Vec<Actor>, state: &State, name: &str, w: f32, h: f32) {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let panel_w = (w * 0.62).min(520.0);
    let panel_h = 150.0;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.62): z(Z_MODAL_SCRIM)
    ));
    // Red rather than the accent: this is the one dialog whose answer costs
    // something, and it should not look like every other one.
    push_bordered(
        actors,
        cx,
        cy,
        panel_w,
        panel_h,
        [0.30, 0.06, 0.08, 1.0],
        Z_MODAL_PANEL,
    );

    actors.push(act!(text:
        font("wendy"): settext("DELETE THIS PACK?".to_owned()):
        align(0.5, 0.5): xy(cx, cy - 46.0): zoom(0.5): horizalign(center):
        diffuse(1.0, 0.62, 0.62, 1.0): z(Z_MODAL_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(name.to_owned()):
        align(0.5, 0.5): xy(cx, cy - 18.0): zoom(0.72): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));

    let songs = state
        .installed
        .get(state.installed_cursor)
        .map_or(0, |entry| entry.songs);
    actors.push(act!(text:
        font("miso"):
        settext(format!(
            "{songs} song folders will be deleted from this machine. This cannot be undone."
        )):
        align(0.5, 0.5): xy(cx, cy + 14.0): zoom(0.5): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(0.88, 0.88, 0.88, 1.0): z(Z_MODAL_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext("START delete    BACK keep it".to_owned()):
        align(0.5, 0.5): xy(cx, cy + 48.0): zoom(0.55): horizalign(center):
        diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]):
        z(Z_MODAL_TEXT)
    ));
}

/// The title and the 1px rule under it.
fn push_header(actors: &mut Vec<Actor>, w: f32) {
    // Wendy at 0.6, left-aligned at (10, 15) -- the theme's own header text,
    // reproduced here because the browser is a screen rather than an overlay.
    actors.push(act!(text:
        font("wendy"): settext("Find Content"):
        align(0.0, 0.5): xy(10.0, 15.0): zoom(0.6): horizalign(left):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(lo::LIST_X, lo::HEADER_RULE_Y): zoomto(w - 2.0 * lo::LIST_X, 1.0):
        diffuse(RULE_RGBA[0], RULE_RGBA[1], RULE_RGBA[2], RULE_RGBA[3]): z(Z_TEXT)
    ));
}

fn push_too_narrow(actors: &mut Vec<Actor>, w: f32, h: f32) {
    actors.push(act!(text:
        font("miso"):
        settext("The content browser is built for a widescreen display.".to_owned()):
        align(0.5, 0.5): xy(w * 0.5, h * 0.5): zoom(0.7): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.8): z(Z_TEXT)
    ));
}

// --- the search prompt --------------------------------------------------------

/// The search box, live the moment the tab is open.
///
/// Not a prompt to open: the reader arrives on this tab and types. It sits in
/// the band's own space, so it displaces the heading rather than floating over
/// anything, and the list below it answers as the characters land.
fn push_search_field(actors: &mut Vec<Actor>, state: &State, asset_manager: &AssetManager, w: f32) {
    /// Where the typed line starts, clear of the magnifier.
    const TEXT_X: f32 = 40.0;
    const TEXT_ZOOM: f32 = 0.68;
    /// Air between the last glyph and the caret, so the bar does not touch it.
    const CARET_GAP: f32 = 2.0;

    let accent = accent(state);
    let x = lo::LIST_X;
    let y = lo::BAND_Y;
    let field_w = w - 2.0 * lo::LIST_X;
    let field_h = lo::BAND_H;

    // Ringed in the accent on every side and darker inside, so it reads as the
    // thing the keyboard is pointed at.
    //
    // A single edge along the bottom was not enough: against a screen that is
    // mostly translucent plates, one accent line looks like a divider rather
    // than a focused control, and nothing said where typing would go.
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x - 2.0, y - 2.0): zoomto(field_w + 4.0, field_h + 4.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_PANEL)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x, y): zoomto(field_w, field_h):
        diffuse(0.04, 0.04, 0.06, 1.0): z(Z_ROW_BG)
    ));
    actors.push(act!(sprite("content_browser/search.png"):
        align(0.5, 0.5): xy(x + 22.0, y + field_h * 0.5): setsize(17.0, 17.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    let blink = (state.caret_elapsed * 2.0) as u32;
    let lit = blink.is_multiple_of(2);
    let max_w = field_w - 200.0;
    let (line, alpha) = if state.query.is_empty() {
        ("Search packs, chart authors and songs".to_owned(), 0.5)
    } else {
        (state.query.clone(), 1.0)
    };
    actors.push(act!(text:
        font("miso"): settext(line):
        align(0.0, 0.5): xy(x + TEXT_X, y + field_h * 0.5): zoom(TEXT_ZOOM): horizalign(left):
        maxwidth(max_w):
        diffuse(1.0, 1.0, 1.0, alpha): z(Z_TEXT)
    ));
    // A drawn caret rather than an underscore glyph, so it is the accent and
    // is visible against the placeholder too -- an empty box with no cursor in
    // it does not look like somewhere to type.
    //
    // It sits where the text actually ends, measured in the font the line is
    // drawn in. A per-character guess drifts: miso is proportional, so every
    // narrow letter left the bar further from the word. `maxwidth` follows the
    // zoom above, so it caps the zoomed width, and the caret caps with it.
    if lit {
        let typed = if state.query.is_empty() {
            0.0
        } else {
            (text_width(asset_manager, "miso", state.query.as_str()) * TEXT_ZOOM).min(max_w)
                + CARET_GAP
        };
        actors.push(act!(quad:
            align(0.0, 0.5): xy(x + TEXT_X + typed, y + field_h * 0.5):
            zoomto(2.0, 20.0):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
        ));
    }

    if search_running(state) {
        actors.push(spinner::accent_actor(
            w - lo::LIST_X - 34.0,
            y + field_h * 0.5,
            26.0,
            Z_TEXT,
            accent,
        ));
    }
}

/// A filled rectangle with a 2px white border, centred on a point.
fn push_bordered(
    actors: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    fill: [f32; 4],
    z: i16,
) {
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(w + 4.0, h + 4.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(w, h):
        diffuse(fill[0], fill[1], fill[2], fill[3]): z(z + 1)
    ));
}

// --- the tab strip ------------------------------------------------------------

fn push_tabs(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let focused_zone = state.zone == Zone::Tabs;

    for (index, entry) in TABS.iter().enumerate() {
        let x = lo::tab_x(index);
        let selected = index == state.tab_index;

        // Three states, exactly as the original: focus (solid accent plate,
        // near-black ink), shown (accent plate at 0.26, accent ink), idle
        // (white 0.05 plate, grey label).
        // The original gives the icon its own grey, a shade darker than the
        // label's, in the idle state only.
        let (plate, ink, icon_ink) = if selected && focused_zone {
            (
                [accent[0], accent[1], accent[2], 1.0],
                TAB_FOCUS_INK,
                TAB_FOCUS_INK,
            )
        } else if selected {
            let lit = [accent[0], accent[1], accent[2], 1.0];
            ([accent[0], accent[1], accent[2], 0.26], lit, lit)
        } else {
            ([1.0, 1.0, 1.0, 0.05], TAB_IDLE_LABEL, TAB_IDLE_ICON)
        };

        actors.push(act!(quad:
            align(0.0, 0.5): xy(x - 3.0, lo::TABS_Y): zoomto(lo::TAB_W, lo::TAB_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_PANEL)
        ));
        // The icon takes the ink colour too, so a focused pill reads as one
        // object rather than as a bright label beside a grey glyph.
        actors.push(act!(sprite(entry.icon()):
            align(0.5, 0.5): xy(x + lo::TAB_ICON_INSET, lo::TABS_Y):
            setsize(lo::TAB_ICON_PX, lo::TAB_ICON_PX):
            diffuse(icon_ink[0], icon_ink[1], icon_ink[2], icon_ink[3]): z(Z_TEXT)
        ));
        actors.push(act!(text:
            font("miso"): settext(entry.label()):
            align(0.0, 0.5): xy(x + lo::TAB_LABEL_INSET, lo::TABS_Y): zoom(lo::TAB_LABEL_ZOOM):
            maxwidth(lo::TAB_W - lo::TAB_LABEL_INSET - 4.0): horizalign(left):
            diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_TEXT)
        ));
    }

    // The strip's own spinner, at the far right, turning whenever a service
    // this view depends on is still working. Never in the year view: that one
    // has a spinner of its own beside the year being indexed, and two wheels
    // for one job is one too many.
    if services_busy(state) && !years_showing(state) {
        actors.push(spinner::accent_actor(
            lo::width() - lo::LIST_X - 12.0,
            lo::TABS_Y,
            20.0,
            Z_TEXT,
            accent,
        ));
    }
}

/// Whether anything the browser depends on is still loading.
fn services_busy(state: &State) -> bool {
    state.snapshot.phase == CatalogPhase::Loading
        || state.details.phase == DetailsPhase::Loading
        || search_running(state)
        || (doubles_showing(state) && state.itgdb.phase == ItgdbPhase::Loading)
}

/// The right-hand readout: what this view is showing, in its own words.
fn push_readout(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let text = readout_text(state);
    if text.is_empty() {
        return;
    }
    actors.push(act!(text:
        font("miso"): settext(text):
        align(1.0, 0.5): xy(w - lo::LIST_X - 16.0, lo::FEAT_LABEL_Y): zoom(0.5):
        horizalign(right):
        diffuse(READOUT_RGBA[0], READOUT_RGBA[1], READOUT_RGBA[2], READOUT_RGBA[3]): z(Z_TEXT)
    ));
}

/// The original's branches, in the original's order -- first match wins.
fn readout_text(state: &State) -> String {
    match tab(state) {
        Tab::Installed => format!("{} installed", state.installed.len()),
        // Never a status here. The year view says it is indexing with the
        // progress bar beside this line, and a second indicator that flips
        // between "indexing..." and a count reads as a glitch rather than as
        // progress.
        Tab::Years => format!("{} packs", commify(state.results.len())),
        // The doubles view is not paged and has two counts, so it reports
        // neither here; its columns say how long they are themselves.
        Tab::Doubles if state.query.is_empty() => String::new(),
        // Nor the beginner list: it stops after a helping and waits to be
        // asked, so it has no page count worth reporting.
        Tab::Beginner if state.query.is_empty() => String::new(),
        _ if !state.query.is_empty() => {
            // The "+" is the only thing on screen that says more matched than
            // the search will show.
            let more = if search_capped(state) { "+" } else { "" };
            let still = if search_running(state) {
                spinner::ellipsis()
            } else {
                ""
            };
            format!(
                "{}{more} results for {:?}{still}",
                commify(state.results.len()),
                state.query
            )
        }
        _ if state.snapshot.phase == CatalogPhase::Loading => {
            format!("loading{}", spinner::ellipsis())
        }
        // Nor a count over skeleton rows, which is a count of what is not
        // on screen yet.
        _ if state.results.is_empty() || awaiting_order(state) => String::new(),
        _ => format!(
            "Page {}/{}   -   {} packs",
            list_page(state) + 1,
            total_pages(state),
            commify(state.results.len())
        ),
    }
}

/// Groups of three from the right, as the original's `Commify`.
pub(super) fn commify(mut n: usize) -> String {
    const DIGITS: usize = usize::MAX.ilog10() as usize + 1;
    let mut bytes = [0; DIGITS + (DIGITS - 1) / 3];
    let mut start = bytes.len();
    let mut group = 0;
    loop {
        start -= 1;
        bytes[start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
        group += 1;
        if group == 3 {
            start -= 1;
            bytes[start] = b',';
            group = 0;
        }
    }
    std::str::from_utf8(&bytes[start..])
        .expect("decimal digits and commas are ASCII")
        .to_owned()
}

// --- the search results band --------------------------------------------------

/// The heading band: what this view is, and what it is made of.
///
/// It stands where the featured grid would be, and its subtitle deliberately
/// overhangs the bottom of it -- which is why the list's tight top is
/// `FEAT_TOP + 63` rather than the band's own height.
fn push_context_band(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let accent = accent(state);
    let Some((title, blurb)) = band(state) else {
        return;
    };
    actors.push(act!(quad:
        align(0.0, 0.0): xy(lo::LIST_X, lo::BAND_Y): zoomto(w - 2.0 * lo::LIST_X, lo::BAND_H):
        diffuse(1.0, 1.0, 1.0, 0.05): z(Z_PANEL)
    ));
    actors.push(act!(text:
        font("wendy"): settext(title):
        align(0.0, 0.5): xy(lo::BAND_TITLE_X, lo::BAND_TITLE_Y): zoom(0.42): horizalign(left):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    // A search quotes the query; every other view states its rule and then
    // says how many packs matched it. Two spaces before the count, as the
    // original's format string has.
    let busy = band_busy(state) || search_running(state);
    let subtitle = if state.query.is_empty() {
        if busy && tab(state) == Tab::Beginner {
            // The count matters here: the walk reads about four packs for
            // every one it keeps, so "checking..." alone reads as stuck.
            format!(
                "{blurb}  Checking pack {} of {}{}",
                state.beginner.checked.max(1),
                state.beginner.candidates.max(1),
                spinner::ellipsis()
            )
        } else if busy {
            format!("{blurb}  Checking packs{}", spinner::ellipsis())
        } else {
            format!("{blurb}  {} found.", band_count(state))
        }
    } else {
        format!("{:?} {blurb}", state.query)
    };
    actors.push(act!(text:
        font("miso"): settext(subtitle):
        align(0.0, 0.5): xy(lo::BAND_TITLE_X, lo::BAND_SUB_Y): zoom(0.55): horizalign(left):
        maxwidth(w - 2.0 * lo::LIST_X - 120.0):
        diffuse(0.78, 0.78, 0.78, 1.0): z(Z_TEXT)
    ));

    if busy {
        actors.push(spinner::accent_actor(
            w - lo::LIST_X - 34.0,
            lo::BAND_Y + lo::BAND_H * 0.5,
            32.0,
            Z_TEXT,
            accent,
        ));
    }
}

/// Whether the band's own view is still being assembled.
fn band_busy(state: &State) -> bool {
    match tab(state) {
        // Reading a pack page per candidate, and saying so: this is the one
        // view that takes real time, and a bare spinner over a short list
        // looks like it has finished and found almost nothing.
        Tab::Beginner => state.beginner.phase == deadsync_online::beginner::BeginnerPhase::Loading,
        // Both doubles columns come from services rather than from the CSV,
        // so this view is not finished until both have landed.
        Tab::Doubles => {
            state.itgdb.phase == ItgdbPhase::Loading || state.details.phase == DetailsPhase::Loading
        }
        // Stamina and all-around wait for their set to be described, and say
        // they are checking rather than print a count of the undescribed set.
        _ => state.snapshot.phase == CatalogPhase::Loading || awaiting_order(state),
    }
}

// --- the year strip -----------------------------------------------------------

/// The year picker: a header, an index progress bar, and one chip per year
/// back to the floor plus an OLDER bucket.
///
/// The focus ring is drawn first so the chips land on top of it and it reads
/// as a border rather than as a box behind them.
fn push_year_strip(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let accent = accent(state);
    let indexing = year_indexing(state);
    let chip_w = lo::year_chip_w();
    let slots = lo::year_slots();
    let focused = state.zone == Zone::Years;

    actors.push(act!(text:
        font("miso"): settext(year_header(state)):
        align(0.0, 0.5): xy(lo::LIST_X, lo::FEAT_LABEL_Y): zoom(0.55): horizalign(left):
        maxwidth(w - 2.0 * lo::LIST_X - 200.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    // The walk's progress, and never a full bar left sitting there afterwards:
    // a finished bar is furniture.
    //
    // It sits below the readout rather than beside it. The two were a couple
    // of pixels apart, which at this size reads as one of them being drawn on
    // top of the other.
    if indexing {
        let track_x = w - lo::LIST_X - 156.0;
        let track_y = lo::FEAT_LABEL_Y + 16.0;
        actors.push(act!(quad:
            align(0.0, 0.5): xy(track_x, track_y):
            zoomto(lo::YEAR_PROGRESS_W, lo::YEAR_PROGRESS_H):
            diffuse(1.0, 1.0, 1.0, 0.16): z(Z_PANEL)
        ));
        let fill = (lo::YEAR_PROGRESS_W * index_fraction(state)).max(1.0);
        actors.push(act!(quad:
            align(0.0, 0.5): xy(track_x, track_y): zoomto(fill, lo::YEAR_PROGRESS_H):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
        ));
        actors.push(spinner::accent_actor(
            track_x - 14.0,
            track_y,
            18.0,
            Z_TEXT,
            accent,
        ));
    }

    if focused {
        let x = lo::year_chip_x(state.year_slot.min(slots - 1));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x - 2.0, lo::YEAR_CHIP_Y - 2.0):
            zoomto(chip_w + 4.0, lo::YEAR_CHIP_H + 4.0):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_PANEL)
        ));
    }

    for slot in 0..slots {
        let x = lo::year_chip_x(slot);
        let selected = slot == state.year_slot;
        let plate = if selected {
            [
                accent[0],
                accent[1],
                accent[2],
                if focused { 0.5 } else { 0.3 },
            ]
        } else {
            [0.0, 0.0, 0.0, 0.55]
        };
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::YEAR_CHIP_Y): zoomto(chip_w, lo::YEAR_CHIP_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_ROW_BG)
        ));
        // Never accent-coloured: white on the picked chip, grey on the rest.
        let lit = if selected { 1.0 } else { 0.6 };
        actors.push(act!(text:
            font("wendy"): settext(year_label(slot)):
            align(0.5, 0.5): xy(x + chip_w * 0.5, lo::YEAR_CHIP_Y + lo::YEAR_CHIP_H * 0.5 - 1.0):
            zoom(0.42): horizalign(center): maxwidth(chip_w - 8.0):
            diffuse(lit, lit, lit, 1.0): z(Z_TEXT)
        ));
    }
}

/// The year view's own heading: which slice, and how much is in it.
///
/// Always the count. It used to swap to "building index..." whenever a request
/// was in flight, which -- once the view started fetching a page per round
/// trip -- meant the line rewrote itself several times a second while the
/// reader was trying to read it.
fn year_header(state: &State) -> String {
    let slice = match year_at(state.year_slot) {
        Some(year) => format!("PACKS ADDED TO SMO IN {year}"),
        None => format!("PACKS ADDED TO SMO BEFORE {}", lo::YEAR_FLOOR),
    };
    format!("{slice}   {} packs", commify(state.results.len()))
}

/// How far through the catalogue the details walk has got.
///
/// The walk publishes after every page, so this is real progress rather than a
/// guess: what has been indexed against what the catalogue says exists.
fn index_fraction(state: &State) -> f32 {
    let total = state.snapshot.catalog.len();
    if total == 0 {
        return 0.0;
    }
    (state.details.by_id.len() as f32 / total as f32).clamp(0.0, 1.0)
}

// --- the featured grid --------------------------------------------------------

/// A heading, a plate, two rows of cards, and the page dots.
fn push_featured(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let accent = accent(state);
    let focused = state.zone == Zone::Featured;
    let panel_w = lo::feat_panel_w();
    let card_w = lo::feat_card_w();

    actors.push(act!(text:
        font("miso"): settext(featured_heading(state).to_owned()):
        align(0.0, 0.5): xy(lo::LIST_X, lo::FEAT_LABEL_Y): zoom(0.55): horizalign(left):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    actors.push(act!(quad:
        align(0.0, 0.0): xy(lo::LIST_X, lo::FEAT_PANEL_Y): zoomto(panel_w, lo::FEAT_PANEL_H):
        diffuse(1.0, 1.0, 1.0, 0.04): z(Z_PANEL)
    ));

    for slot in 0..FEATURED_VISIBLE {
        let column = slot % lo::FEAT_COLS;
        let row = slot / lo::FEAT_COLS;
        let x = lo::LIST_X + lo::FEAT_PAD + column as f32 * (card_w + lo::FEAT_GAP);
        let y = lo::FEAT_TOP + row as f32 * (lo::FEAT_CARD_H + lo::FEAT_ROW_GAP);
        let card = state.featured_window + slot;
        let selected = focused && card == state.featured_index;

        let Some(index) = state.featured.get(card) else {
            // An empty slot still draws its plate, so the grid keeps its shape
            // while the catalogue is still arriving.
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, y): zoomto(card_w, lo::FEAT_CARD_H):
                diffuse(1.0, 1.0, 1.0, 0.045): z(Z_ROW_BG)
            ));
            continue;
        };
        let Some(pack) = state.snapshot.catalog.get(*index) else {
            continue;
        };

        if selected {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x - 2.0, y - 2.0):
                zoomto(card_w + 4.0, lo::FEAT_CARD_H + 4.0):
                diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_PANEL)
            ));
        }
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y): zoomto(card_w, lo::FEAT_CARD_H):
            diffuse(1.0, 1.0, 1.0, 0.06): z(Z_ROW_BG)
        ));

        push_art_or_status(
            actors,
            state,
            pack.id,
            x,
            y,
            card_w,
            lo::FEAT_CARD_H,
            Z_ART,
            accent,
            banner_expected(state, pack.id),
        );
    }

    // Page dots, one per page of cards, centred under the grid.
    let pages = state.featured.len().div_ceil(FEATURED_VISIBLE).max(1);
    if pages > 1 {
        const SPACING: f32 = 13.0;
        let here = state.featured_window / FEATURED_VISIBLE;
        let x0 = w * 0.5 - (pages - 1) as f32 * SPACING * 0.5;
        for dot in 0..pages {
            let lit = dot == here;
            // Round, not square. The original's comment: square quads with a
            // slightly wider rectangle for the current page "reads as grit on
            // the screen rather than as an indicator" at five pixels across.
            let size = if lit { 7.0 } else { 5.0 };
            let rgba = if lit {
                [accent[0], accent[1], accent[2], 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.28]
            };
            actors.push(act!(sprite("content_browser/dot.png"):
                align(0.5, 0.5): xy(x0 + dot as f32 * SPACING, lo::FEAT_RULE_Y):
                setsize(size, size):
                diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_TEXT)
            ));
        }
    }
}

// --- the pack list ------------------------------------------------------------

/// Where one row goes and how it is lit. The doubles columns hand different
/// values to the same drawing code, so the two lists cannot drift apart.
struct RowPlacement {
    x: f32,
    y: f32,
    width: f32,
    art_cx: f32,
    art_w: f32,
    text_x: f32,
    selected: bool,
    /// Whether the cursor is actually in this list, as opposed to on a strip
    /// above it. The original dims the focus rather than dropping it.
    focused: bool,
    badges: bool,
}

/// Seven rows: art on the left, name and meta beside it, badges on the right.
fn push_list(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let list_w = lo::list_w();
    let top = lo::list_top(grid_showing(state));
    let rows = visible_rows();
    let list_focused = state.zone == Zone::List;
    let start = window_start(state);

    // The scrollbar sits behind the rows, 3px off the list's right edge.
    if row_count(state) > rows {
        let track_h = rows as f32 * lo::ROW_H - 3.0;
        let x = lo::LIST_X + list_w + 3.0;
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, top): zoomto(lo::SCROLL_W, track_h):
            diffuse(1.0, 1.0, 1.0, 0.10): z(Z_PANEL)
        ));
        let frac = rows as f32 / row_count(state) as f32;
        let thumb_h = (track_h * frac).max(14.0);
        let travel = track_h - thumb_h;
        let pages = total_pages(state).saturating_sub(1).max(1) as f32;
        let progress = (list_page(state) as f32 / pages).clamp(0.0, 1.0);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, top + travel * progress): zoomto(lo::SCROLL_W, thumb_h):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
        ));
    }

    // Skeletons while the catalogue is coming, while it is here but cannot yet
    // be put in the right order, and while a search is bringing its passes in.
    let loading = (state.snapshot.phase == CatalogPhase::Loading && state.results.is_empty())
        || awaiting_order(state)
        || (search_running(state) && state.results.is_empty());
    // The search tab before anything is typed. Not a loading state but a
    // shape: it shows where results will appear, which an empty half-screen
    // does not. So it has no spinners -- nothing is coming until the reader
    // types, and a wheel would promise otherwise.
    let search_placeholder =
        search_field_showing(state) && state.query.is_empty() && state.results.is_empty();
    // A beginner list still being gathered fills its empty slots with
    // skeletons, as the original does, so a list that is still assembling
    // reads as working rather than as empty.
    let building = beginner_building(state);

    for slot in 0..rows {
        let y = top + slot as f32 * lo::ROW_H;

        if loading || search_placeholder {
            push_skeleton_row(
                actors,
                lo::LIST_X,
                y,
                list_w,
                lo::ROW_ART_CX,
                accent,
                !search_placeholder,
            );
            continue;
        }
        let Some(index) = state.results.get(start + slot) else {
            if building {
                push_skeleton_row(actors, lo::LIST_X, y, list_w, lo::ROW_ART_CX, accent, true);
            } else if slot == 0 {
                push_empty_state(actors, state, top);
            }
            continue;
        };
        let Some(pack) = state.snapshot.catalog.get(*index) else {
            continue;
        };

        push_row(
            actors,
            state,
            pack,
            &RowPlacement {
                x: lo::LIST_X,
                y,
                width: list_w,
                art_cx: lo::ROW_ART_CX,
                art_w: lo::ROW_ART_W,
                text_x: lo::ROW_TEXT_X,
                selected: start + slot == state.cursor,
                focused: list_focused,
                badges: true,
            },
            accent,
        );
    }

    // The end of a list with more to find, marked under its last row. Not
    // while skeletons still fill the page: they already say more is coming,
    // and the mark would land on top of one.
    let on_page = state.results.len().saturating_sub(start).min(rows);
    let page_filling = building && on_page < rows;
    // Under the empty-state line when there are no rows at all, rather than on
    // top of it.
    let rows_bottom = if on_page == 0 {
        top + lo::ROW_H + 14.0
    } else {
        top + on_page as f32 * lo::ROW_H
    };
    if !loading
        && !page_filling
        && let Some(finding) = more_mark(state)
    {
        push_more_mark(actors, rows_bottom, list_w, finding, accent);
    }
}

fn push_row(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    at: &RowPlacement,
    accent: [f32; 4],
) {
    let focus_alpha = if at.selected && at.focused {
        0.16
    } else if at.selected {
        0.09
    } else {
        0.05
    };
    actors.push(act!(quad:
        align(0.0, 0.0): xy(at.x, at.y): zoomto(at.width, lo::ROW_FOCUS_H):
        diffuse(1.0, 1.0, 1.0, focus_alpha): z(Z_ROW_BG)
    ));

    // Art, or the accent spinner standing in for it.
    let art_x = at.x + at.art_cx - at.art_w * 0.5;
    let art_y = at.y + lo::ROW_ART_CY - lo::ROW_ART_H * 0.5;
    push_art_or_status(
        actors,
        state,
        pack.id,
        art_x,
        art_y,
        at.art_w,
        lo::ROW_ART_H,
        Z_ART,
        accent,
        banner_expected(state, pack.id),
    );

    let reserve = if at.badges {
        lo::ROW_TEXT_RESERVE
    } else {
        at.text_x + 12.0
    };
    let text_w = (at.width - reserve).max(40.0);
    actors.push(act!(text:
        font("miso"): settext(pack.name.clone()):
        align(0.0, 0.5): xy(at.x + at.text_x, at.y + lo::ROW_NAME_Y):
        zoom(lo::ROW_NAME_ZOOM): maxwidth(text_w): horizalign(left):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));

    let meta = meta_line(state, pack);
    if !meta.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(meta):
            align(0.0, 0.5): xy(at.x + at.text_x, at.y + lo::ROW_META_Y):
            zoom(lo::ROW_META_ZOOM): maxwidth(text_w): horizalign(left):
            diffuse(META_RGBA[0], META_RGBA[1], META_RGBA[2], META_RGBA[3]): z(Z_TEXT)
        ));
    }

    if at.badges {
        push_type_badge(actors, state, pack, at.width, at.y);
    }
    push_status_badge(actors, state, pack, at.x + at.width, at.y);
}

/// The mark under a list's last row that says there is more to find.
///
/// The original's three states in one spot: "more" with the down arrow while
/// a press would fetch another helping, "finding more" while one is being
/// gathered, and nothing once the list is complete. `rows_bottom` is where the
/// last drawn row ends.
fn push_more_mark(
    actors: &mut Vec<Actor>,
    rows_bottom: f32,
    list_w: f32,
    finding: bool,
    accent: [f32; 4],
) {
    let (label, rgba) = if finding {
        (
            format!("finding more{}", spinner::ellipsis()),
            [0.6, 0.6, 0.6, 1.0],
        )
    } else {
        (
            "&MENUDOWN; more".to_owned(),
            [accent[0], accent[1], accent[2], 1.0],
        )
    };
    actors.push(act!(text:
        font("miso"): settext(label):
        align(0.5, 0.5): xy(lo::LIST_X + list_w * 0.5, rows_bottom + lo::MORE_MARK_DY):
        zoom(0.5): horizalign(center):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_TEXT)
    ));
}

/// Nothing to show, and why. An empty list with no explanation is the one
/// thing a marketplace must never be.
fn push_empty_state(actors: &mut Vec<Actor>, state: &State, top: f32) {
    let accent = accent(state);
    if state.snapshot.phase == CatalogPhase::Loading {
        spinner::with_label(
            actors,
            lo::LIST_X + 20.0,
            top + lo::ROW_H,
            spinner::SMALL_PX,
            "loading the catalogue",
            Z_TEXT,
            accent,
        );
        return;
    }
    if state.snapshot.phase == CatalogPhase::Error {
        let message = state
            .snapshot
            .message
            .clone()
            .unwrap_or_else(|| "could not reach stepmaniaonline.net".to_owned());
        actors.push(act!(text:
            font("miso"): settext(message):
            align(0.0, 0.5): xy(lo::LIST_X, top + lo::ROW_H): zoom(0.55):
            maxwidth(lo::list_w()): horizalign(left):
            diffuse(ERROR_RGBA[0], ERROR_RGBA[1], ERROR_RGBA[2], ERROR_RGBA[3]): z(Z_TEXT)
        ));
        actors.push(act!(text:
            font("miso"): settext("SELECT to try again".to_owned()):
            align(0.0, 0.5): xy(lo::LIST_X, top + lo::ROW_H + 22.0): zoom(0.5): horizalign(left):
            diffuse(1.0, 1.0, 1.0, 0.5): z(Z_TEXT)
        ));
        return;
    }
    // A search still bringing its passes in is not an empty answer yet, and
    // saying so is the difference between "no results" and "not finished".
    if search_running(state) {
        spinner::with_label(
            actors,
            lo::LIST_X + 20.0,
            top + lo::ROW_H,
            spinner::SMALL_PX,
            "searching names, charters and songs",
            Z_TEXT,
            accent,
        );
        return;
    }
    // A view that is genuinely empty, in its own words -- except the beginner
    // list when the popularity list it walks never arrived. "None found"
    // would be a claim about packs; this is a failure to ask.
    let message = if beginner_showing(state) && state.popular.phase == PopularPhase::Error {
        "could not load the popular packs to look through"
    } else if beginner_showing(state) && state.can_load_more {
        // A helping that found nothing is not an answer about packs: it may
        // be that every page it read failed. The mark under this asks again.
        "none found in the packs checked so far"
    } else if state.query.is_empty() {
        tab(state).empty_message()
    } else {
        "nothing matches that search"
    };
    actors.push(act!(text:
        font("miso"): settext(message.to_owned()):
        align(0.0, 0.5): xy(lo::LIST_X, top + lo::ROW_H): zoom(0.55): horizalign(left):
        diffuse(1.0, 1.0, 1.0, 0.55): z(Z_TEXT)
    ));
}

/// A row that has no data yet: the plate, two bars where the name and the meta
/// line will be, and -- when something really is on its way -- the spinner
/// that says so.
fn push_skeleton_row(
    actors: &mut Vec<Actor>,
    x: f32,
    y: f32,
    width: f32,
    art_cx: f32,
    accent: [f32; 4],
    coming: bool,
) {
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x, y): zoomto(width, lo::ROW_FOCUS_H):
        diffuse(1.0, 1.0, 1.0, 0.045): z(Z_ROW_BG)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x + 76.0, y + 10.0): zoomto(width / 3.0, 6.0):
        diffuse(1.0, 1.0, 1.0, 0.07): z(Z_ART)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x + 76.0, y + 22.0): zoomto(width / 6.0, 4.0):
        diffuse(1.0, 1.0, 1.0, 0.05): z(Z_ART)
    ));
    if coming {
        actors.push(spinner::accent_actor(
            x + art_cx,
            y + lo::ROW_ART_CY,
            22.0,
            Z_ART,
            accent,
        ));
    }
}

/// `78 songs  -  640.5 MB  -  added Mar 14, 2026`, with any part absent.
/// Two spaces, hyphen, two spaces -- the original's separator.
fn meta_line(state: &State, pack: &PackInfo) -> String {
    let mut bits: Vec<String> = Vec::with_capacity(4);
    if pack.song_count > 0 {
        bits.push(format!("{} songs", pack.song_count));
    }
    bits.push(format_bytes(pack.size_bytes));
    if let Some(date) = details_for(state, pack).and_then(|d| d.date_added.as_deref()) {
        bits.push(format!("added {}", format_date(date, true)));
    }
    // Why this row is in a search's answer, last. A result that matched on a
    // charter's name rather than the pack's looks like a wrong answer until
    // the row says so.
    if let Some(why) = state.search_why.get(&pack.id) {
        bits.push(why.clone());
    }
    bits.join("  -  ")
}

/// `2026-03-14` as `March 14, 2026`, or `Mar 14, 2026` in short form.
///
/// Anything that is not an ISO date is handed back untouched, because the only
/// other thing it can be is a date the site already formatted.
pub(super) fn format_date(iso: &str, short: bool) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = iso.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_owned();
    };
    let (Ok(month), Ok(day)) = (month.parse::<usize>(), day.parse::<u32>()) else {
        return iso.to_owned();
    };
    let Some(name) = MONTHS.get(month.wrapping_sub(1)) else {
        return iso.to_owned();
    };
    let name = if short { &name[..3] } else { name };
    format!("{name} {day}, {year}")
}

/// The original's `FormatBytes`: two decimals for GB, one for MB.
pub(super) fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else {
        format!("{:.0} KB", bytes / 1024.0)
    }
}

/// The badge left of the status: which tab this row would otherwise have been
/// found under.
///
/// It reports the catalogue's **pack type**, never its substyle. A search
/// draws results from every tab at once, so every row says where it belongs;
/// outside a search the tab already implies the type, so only the two that it
/// does not -- a mixed pad/keyboard pack and a DDR pack -- say anything.
///
/// Substyle deliberately does not appear here. The catalogue tags packs
/// "stamina" or "technical" on the uploader's say-so, and printing that beside
/// a search result reads as a claim the browser is making about the pack.
fn push_type_badge(actors: &mut Vec<Actor>, state: &State, pack: &PackInfo, list_w: f32, y: f32) {
    const KEYBOARD_INK: [f32; 4] = [0.55, 0.72, 1.0, 1.0];
    const DDR_INK: [f32; 4] = [1.0, 0.72, 0.35, 1.0];
    const PAD_INK: [f32; 4] = [0.55, 0.55, 0.55, 1.0];

    let kind = pack
        .pack_type
        .as_deref()
        .map(str::trim)
        .filter(|value| meaningful(value))
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (text, rgba) = if state.query.is_empty() {
        match kind.as_str() {
            "mixed" => ("PAD+KEY", KEYBOARD_INK),
            "ddr" => ("DDR", DDR_INK),
            _ => return,
        }
    } else {
        match kind.as_str() {
            "keyboard" => ("KEY", KEYBOARD_INK),
            "mixed" => ("PAD+KEY", KEYBOARD_INK),
            "ddr" => ("DDR", DDR_INK),
            _ => ("PAD", PAD_INK),
        }
    };

    actors.push(act!(text:
        font("miso"): settext(text.to_owned()):
        align(0.0, 0.5):
        xy(lo::LIST_X + list_w - lo::ROW_TYPE_BADGE_INSET, y + lo::ROW_BADGE_Y):
        zoom(0.5): maxwidth(40.0): horizalign(left):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_BADGE)
    ));
}

/// The right-hand badge: what the download queue or the library says.
pub(super) fn push_status_badge(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    right: f32,
    y: f32,
) {
    let accent = accent(state);
    let (text, rgba) = if let Some(install) = install_for(state, pack) {
        match install.phase {
            InstallPhase::Queued => ("queued".to_owned(), META_RGBA),
            InstallPhase::Downloading => {
                let pct = if install.total_bytes > 0 {
                    (install.downloaded_bytes as f64 / install.total_bytes as f64 * 100.0) as u32
                } else {
                    0
                };
                (format!("{pct}%"), accent)
            }
            InstallPhase::Extracting => ("Installing".to_owned(), accent),
            InstallPhase::Installed => ("Installed".to_owned(), INSTALLED_NOW),
            InstallPhase::Error => ("Error".to_owned(), ERROR_RGBA),
        }
    } else if is_installed(state, pack) {
        ("In Library".to_owned(), IN_LIBRARY)
    } else {
        return;
    };

    actors.push(act!(text:
        font("miso"): settext(text):
        align(1.0, 0.5): xy(right - lo::ROW_STATUS_BADGE_INSET, y + lo::ROW_BADGE_Y):
        zoom(0.5): horizalign(right):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_BADGE)
    ));
}

// --- the doubles view ---------------------------------------------------------

/// Two columns side by side.
///
/// A pack built for doubles and a pack with four doubles charts buried in it
/// are two different things to go looking for, so each gets a column rather
/// than one list with a seam in it the reader has to find.
fn push_doubles(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let width = lo::dbl_w();
    let top = lo::dbl_top();
    let in_rows = state.zone == Zone::DoublesRows;
    let here = doubles_cursor(state);

    for column in 0..2usize {
        let x = lo::dbl_x(column);
        let list: Vec<usize> = doubles_column(state, column).to_vec();
        let picked = column == state.doubles_column;

        // The picker's focus is the whole column lit as one thing, so "you are
        // choosing between two lists" is something the screen says rather than
        // something the reader has to deduce from a cursor.
        if picked && !in_rows {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x - 4.0, lo::LIST_TOP_TIGHT):
                zoomto(width + 8.0, lo::DBL_HEAD + lo::DBL_ROWS as f32 * lo::ROW_H):
                diffuse(accent[0], accent[1], accent[2], 0.10): z(Z_PANEL)
            ));
        }

        let head = if column == 0 {
            "MADE FOR DOUBLES"
        } else {
            "SOME DOUBLES"
        };
        let heading = if list.is_empty() {
            head.to_owned()
        } else {
            format!("{head}   {}", list.len())
        };
        let head_rgba = if picked { accent } else { [0.6, 0.6, 0.6, 1.0] };
        actors.push(act!(text:
            font("miso"): settext(heading):
            align(0.0, 0.5): xy(x, lo::LIST_TOP_TIGHT + 7.0): zoom(0.5): horizalign(left):
            maxwidth(width - 8.0):
            diffuse(head_rgba[0], head_rgba[1], head_rgba[2], head_rgba[3]): z(Z_TEXT)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::LIST_TOP_TIGHT + 15.0): zoomto(width, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.13): z(Z_TEXT)
        ));

        // Each column is still coming on its own terms: the curated list is
        // complete the moment itgdb answers, while the candidates are not done
        // until the details walk is. One spinner for the whole tab left a
        // finished column looking busy.
        let building = if column == 0 {
            state.itgdb.phase == ItgdbPhase::Loading || state.doubles_left_waiting
        } else {
            state.details.phase == DetailsPhase::Loading
        };

        for slot in 0..lo::DBL_ROWS {
            let y = top + slot as f32 * lo::ROW_H;
            let at = state.doubles_window[column] + slot;
            let Some(index) = list.get(at) else {
                if building {
                    push_skeleton_row(actors, x, y, width, lo::DBL_ART_CX, accent, true);
                }
                continue;
            };
            let Some(pack) = state.snapshot.catalog.get(*index) else {
                continue;
            };
            push_row(
                actors,
                state,
                pack,
                &RowPlacement {
                    x,
                    y,
                    width,
                    art_cx: lo::DBL_ART_CX,
                    art_w: lo::DBL_ART_W,
                    text_x: lo::DBL_TEXT_X,
                    selected: picked && at == here,
                    focused: in_rows,
                    badges: false,
                },
                accent,
            );
        }

        // What a column says when it has nothing in it and nothing coming.
        if list.is_empty() && !building {
            let message = if column == 0 && state.itgdb.phase == ItgdbPhase::Error {
                "itgdb.net could not be reached, so this list is unavailable."
            } else {
                "Nothing here."
            };
            actors.push(act!(text:
                font("miso"): settext(message.to_owned()):
                align(0.0, 0.5): xy(x + 4.0, top + 14.0): zoom(0.5): horizalign(left):
                maxwidth(width - 12.0):
                diffuse(0.55, 0.55, 0.55, 1.0): z(Z_TEXT)
            ));
        }

        // One scrollbar per column, 5px off its right edge.
        if list.len() > lo::DBL_ROWS {
            let track_h = lo::DBL_ROWS as f32 * lo::ROW_H - 8.0;
            let frac = lo::DBL_ROWS as f32 / list.len() as f32;
            let thumb_h = (track_h * frac).max(14.0);
            let span = (list.len() - lo::DBL_ROWS) as f32;
            let progress = (state.doubles_window[column] as f32 / span).clamp(0.0, 1.0);
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x + width + 5.0, top): zoomto(lo::SCROLL_W, track_h):
                diffuse(1.0, 1.0, 1.0, 0.10): z(Z_PANEL)
            ));
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x + width + 5.0, top + (track_h - thumb_h) * progress):
                zoomto(lo::SCROLL_W, thumb_h):
                diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
            ));
        }
    }
}

// --- the library grid ---------------------------------------------------------

/// What is already in the library: two columns of eleven, paged.
///
/// A list would waste half the screen on packs whose names are short, and the
/// library is the one view where somebody is looking for a name they already
/// know rather than reading rows -- so it fits twenty-two on a page instead of
/// seven, and orders them alphabetically.
fn push_installed(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let accent = accent(state);
    let cell_w = lo::inst_w();
    let focused = state.zone == Zone::Installed;
    let window = installed_page(state) * lo::INST_ROWS;

    let mut heading = format!("{} PACKS IN YOUR LIBRARY", state.installed.len());
    if state.installed.len() > lo::INST_ROWS {
        heading.push_str(
            format!(
                "   PAGE {}/{}",
                installed_page(state) + 1,
                installed_pages(state)
            )
            .as_str(),
        );
    }
    actors.push(act!(text:
        font("miso"): settext(heading):
        align(0.0, 0.5): xy(lo::LIST_X, lo::FEAT_LABEL_Y): zoom(0.55): horizalign(left):
        maxwidth(w - 2.0 * lo::LIST_X - 200.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_TEXT)
    ));

    if state.installed.is_empty() {
        actors.push(act!(text:
            font("miso"): settext("No song packs found in your Songs folder.".to_owned()):
            align(0.5, 0.5): xy(w * 0.5, lo::INST_TOP + 60.0): zoom(0.6): horizalign(center):
            diffuse(0.7, 0.7, 0.7, 1.0): z(Z_TEXT)
        ));
        return;
    }

    for slot in 0..lo::INST_ROWS {
        let column = slot / lo::INST_PER_COL;
        let row = slot % lo::INST_PER_COL;
        let x = lo::inst_x(column);
        let y = lo::inst_y(row);
        let Some(entry) = state.installed.get(window + slot) else {
            continue;
        };
        let selected = window + slot == state.installed_cursor;

        // Alternating plates rather than a flat field: twenty-two rows of the
        // same colour is a wall, and the cursor has to be findable in it.
        let plate = if selected && focused {
            [accent[0], accent[1], accent[2], 0.30]
        } else if selected {
            [accent[0], accent[1], accent[2], 0.14]
        } else if slot.is_multiple_of(2) {
            [0.0, 0.0, 0.0, 0.34]
        } else {
            [0.0, 0.0, 0.0, 0.5]
        };
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y): zoomto(cell_w, lo::INST_ROW_H - 4.0):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_ROW_BG)
        ));

        // The pack is on this machine and so is its artwork. Nothing here goes
        // to the network and nothing spins.
        //
        // Pushed unconditionally rather than gated on the texture already
        // being loaded: drawing the sprite is what asks for it. Checking
        // `texture_dims` first meant the answer was always no, because nothing
        // had asked -- so the grid drew no artwork at all, ever.
        let known = installed_catalog_entry(state, entry);
        let art_h = lo::INST_ROW_H - 8.0;
        let art_cx = x + lo::INST_ART_CX;
        let art_cy = y + 2.0 + art_h * 0.5;
        match entry.banner.as_deref() {
            Some(key) => actors.push(banner::cover_sprite(
                key.to_owned(),
                art_cx,
                art_cy,
                lo::INST_ART_W,
                art_h,
                1.0,
                Z_ART,
            )),
            None => actors.push(act!(sprite("content_browser/nobanner.png"):
                align(0.5, 0.5): xy(art_cx, art_cy):
                setsize(art_h * 2.5, art_h):
                diffuse(1.0, 1.0, 1.0, 0.17): z(Z_ART)
            )),
        }

        let text_w = cell_w - lo::INST_TEXT_X - 96.0;
        actors.push(act!(text:
            font("miso"): settext(entry.name.clone()):
            align(0.0, 0.5): xy(x + lo::INST_TEXT_X, y + 8.0): zoom(0.55): horizalign(left):
            maxwidth(text_w):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
        ));

        // What the game knows for certain, then what the catalogue adds.
        let mut detail = format!("{} songs", entry.songs);
        if let Some(pack) = known {
            detail.push_str(format!("  -  {}", format_bytes(pack.size_bytes)).as_str());
            if let Some(date) = details_for(state, pack).and_then(|d| d.date_added.as_deref()) {
                detail.push_str(format!("  -  added {}", format_date(date, true)).as_str());
            }
        }
        actors.push(act!(text:
            font("miso"): settext(detail):
            align(0.0, 0.5): xy(x + lo::INST_TEXT_X, y + 20.0): zoom(0.42): horizalign(left):
            maxwidth(text_w):
            diffuse(META_RGBA[0], META_RGBA[1], META_RGBA[2], META_RGBA[3]): z(Z_TEXT)
        ));

        // What this pack's own Pack.ini says about its sync -- the thing START
        // edits. Spelled out rather than abbreviated: "ITG" alone in a corner
        // is a word, not a statement, and a reader has no way to tell whether
        // it is a label, a genre or a warning.
        let (chip, chip_rgba) = match entry.sync {
            SyncPref::Itg => ("sync: ITG", [0.97, 0.78, 0.30, 1.0]),
            SyncPref::Null => ("sync: NULL", [0.40, 0.85, 0.45, 1.0]),
            SyncPref::Default => ("sync: not set", [0.45, 0.45, 0.45, 1.0]),
        };
        actors.push(act!(text:
            font("miso"): settext(chip.to_owned()):
            align(1.0, 0.5): xy(x + cell_w - 10.0, y + 20.0): zoom(0.4): horizalign(right):
            diffuse(chip_rgba[0], chip_rgba[1], chip_rgba[2], chip_rgba[3]): z(Z_BADGE)
        ));
    }

    // What the last delete did. Kept on screen rather than flashed, because a
    // partial delete is the case worth reading.
    if let Some(note) = state.remove_result.as_deref() {
        actors.push(act!(text:
            font("miso"): settext(note.to_owned()):
            align(1.0, 0.5): xy(w - lo::LIST_X - 16.0, lo::FEAT_LABEL_Y + 14.0): zoom(0.45):
            horizalign(right): maxwidth(w * 0.5):
            diffuse(0.8, 0.8, 0.8, 1.0): z(Z_TEXT)
        ));
    }

    // One scrollbar for the whole grid, off the right of the second column.
    if state.installed.len() > lo::INST_ROWS {
        let track_h = lo::INST_PER_COL as f32 * lo::INST_ROW_H - 4.0;
        let x = lo::inst_x(lo::INST_COLS - 1) + cell_w + 5.0;
        let frac = lo::INST_ROWS as f32 / state.installed.len() as f32;
        let thumb = (track_h * frac).max(14.0);
        let pages = installed_pages(state).saturating_sub(1).max(1) as f32;
        let progress = (installed_page(state) as f32 / pages).clamp(0.0, 1.0);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::INST_TOP): zoomto(lo::SCROLL_W, track_h):
            diffuse(1.0, 1.0, 1.0, 0.10): z(Z_PANEL)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, lo::INST_TOP + (track_h - thumb) * progress):
            zoomto(lo::SCROLL_W, thumb):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_ROW_BG)
        ));
    }
}

// --- the info pane ------------------------------------------------------------

/// The pane beside the list: the selected pack, larger.
fn push_info_pane(actors: &mut Vec<Actor>, state: &State) {
    let accent = accent(state);
    let x = lo::pane_x();
    let w = lo::pane_w();
    let top = lo::list_top(grid_showing(state));
    // The pane ends where the list ends. It used to run to the bottom of the
    // content area, which left the list stopping short of it by half a row --
    // two panels side by side that disagreed about where the screen ends.
    let h = visible_rows() as f32 * lo::ROW_H - 3.0;
    let picked = if state.zone == Zone::Featured {
        focused_pack(state)
    } else {
        selected_pack(state)
    };
    let Some(pack) = picked else {
        return;
    };

    actors.push(act!(quad:
        align(0.0, 0.0): xy(x, top): zoomto(w, h):
        diffuse(1.0, 1.0, 1.0, 0.07): z(Z_PANEL)
    ));

    // The banner is height-limited for every aspect SMO publishes, so it draws
    // 62px tall centred 38px down the pane.
    push_art_or_status(
        actors,
        state,
        pack.id,
        x + 20.0,
        top + 7.0,
        w - 40.0,
        62.0,
        Z_ART,
        accent,
        banner_expected(state, pack.id),
    );

    actors.push(act!(text:
        font("miso"): settext(pack.name.clone()):
        align(0.5, 0.5): xy(x + w * 0.5, top + 84.0): zoom(0.7): maxwidth(w - 24.0):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));

    for (line, text) in pane_lines(state, pack).into_iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        actors.push(act!(text:
            font("miso"): settext(text):
            align(0.5, 0.5): xy(x + w * 0.5, top + 96.0 + (line + 1) as f32 * 13.0):
            zoom(0.5): maxwidth(w - 24.0): horizalign(center):
            diffuse(0.75, 0.75, 0.75, 1.0): z(Z_TEXT)
        ));
    }

    push_pane_histogram(actors, state, pack, x, top, w, h);
}

/// The pane's five lines, in the original's order and wording. The separator
/// is three spaces, hyphen, three spaces -- wider than the row's.
fn pane_lines(state: &State, pack: &PackInfo) -> [String; 5] {
    let details = details_for(state, pack);
    let page = page_for(state, pack);

    let mut songs = format!("{} songs", pack.song_count);
    if let Some(charts) = page.and_then(|p| p.chart_count.as_deref()) {
        songs.push_str(format!("   -   {charts} charts").as_str());
    }

    let mut size = format_bytes(pack.size_bytes);
    if let Some((low, high)) = page.and_then(deadsync_online::pack_page::PackPage::difficulty_span)
    {
        size.push_str(format!("   -   difficulty {low} - {high}").as_str());
    }

    let added = details
        .and_then(|d| d.date_added.as_deref())
        .map(|date| format!("Added to SMO on {}", format_date(date, false)))
        .unwrap_or_default();

    let authors = match page {
        Some(page) if !page.authors.is_empty() => {
            let shown = page.authors.len().min(3);
            let mut names = page.authors[..shown].join(", ");
            if page.authors.len() > shown {
                names.push_str(", ...");
            }
            format!("charts by {names}")
        }
        Some(_) => String::new(),
        None if state.page.pack_id == pack.id => match state.page.phase {
            PagePhase::Loading => format!("loading details{}", spinner::ellipsis()),
            PagePhase::Error => "could not load details".to_owned(),
            PagePhase::Idle | PagePhase::Ready => String::new(),
        },
        None => String::new(),
    };

    let sync = format!("sync: {}", smo_sync_of(pack).short());

    [songs, size, added, authors, sync]
}

/// The pack page for this pack, when it happens to be the one being read.
pub(super) fn page_for<'a>(
    state: &'a State,
    pack: &PackInfo,
) -> Option<&'a deadsync_online::pack_page::PackPage> {
    if state.page.pack_id != pack.id {
        return None;
    }
    state.page.page.as_deref()
}

/// The pane's difficulty histogram, along its bottom.
fn push_pane_histogram(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    x: f32,
    top: f32,
    w: f32,
    h: f32,
) {
    let Some(page) = page_for(state, pack) else {
        return;
    };
    if page.meter_counts.is_empty() {
        return;
    }
    const HIST_H: f32 = 42.0;
    let hist_w = w - 48.0;
    let baseline = top + h - 24.0;
    let max_count = page.meter_counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bars = page.meter_counts.len();
    let bar_w = ((hist_w / bars as f32).floor() - 2.0).max(2.0);
    let x0 = x + 24.0 + (hist_w - bars as f32 * (bar_w + 2.0)) * 0.5;

    for (index, count) in page.meter_counts.iter().enumerate() {
        let meter = page.meter_labels.get(index).copied().unwrap_or(1);
        let rgba = meter_color(meter);
        let height = (*count as f32 / max_count * HIST_H).max(2.0);
        let bar_x = x0 + index as f32 * (bar_w + 2.0);
        actors.push(act!(quad:
            align(0.0, 1.0): xy(bar_x, baseline): zoomto(bar_w, height):
            diffuse(rgba[0], rgba[1], rgba[2], 0.95): z(Z_ART)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(bar_x, baseline + 3.0): zoomto(bar_w, 3.0):
            diffuse(rgba[0], rgba[1], rgba[2], 1.0): z(Z_ART)
        ));
    }

    if let Some((low, high)) = page.difficulty_span() {
        actors.push(act!(text:
            font("miso"): settext(format!("charts per difficulty  ({low} - {high})")):
            align(0.5, 0.5): xy(x + w * 0.5, baseline + 16.0): zoom(0.5): horizalign(center):
            maxwidth(w - 16.0):
            diffuse(0.6, 0.6, 0.6, 1.0): z(Z_TEXT)
        ));
    }
}

/// The theme's own difficulty ramp, bucketed the way the original buckets it.
pub(super) fn meter_color(meter: u32) -> [f32; 3] {
    const COLORS: [[f32; 3]; 7] = [
        [0.28, 0.76, 0.45],
        [0.40, 0.84, 0.38],
        [0.62, 0.88, 0.32],
        [0.94, 0.84, 0.28],
        [0.97, 0.60, 0.24],
        [0.95, 0.34, 0.30],
        [0.85, 0.20, 0.44],
    ];
    let bucket = match meter {
        0..=4 => 0,
        5..=8 => 1,
        9..=11 => 2,
        12..=13 => 3,
        14..=15 => 4,
        16..=17 => 5,
        _ => 6,
    };
    COLORS[bucket]
}

// --- shared helpers -----------------------------------------------------------

/// SMO writes absent values as "None", "null" or "n/a" rather than leaving the
/// column empty, and showing those verbatim reads like a bug.
pub(super) fn meaningful(value: &str) -> bool {
    let trimmed = value.trim();
    !(trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed.eq_ignore_ascii_case("null")
        || trimmed.eq_ignore_ascii_case("n/a"))
}

/// Draw artwork, or say what is happening instead.
///
/// Three states, and telling them apart is the whole point: it is here, it is
/// coming, or it is not coming. A row that draws the same spinner for the last
/// two spins forever on a banner that 404s.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_art_or_status(
    actors: &mut Vec<Actor>,
    state: &State,
    key: u64,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    z: i16,
    accent: [f32; 4],
    expected: bool,
) {
    if push_art_fitted(actors, key, x, y, w, h, z) {
        return;
    }
    // A spinner is a promise that something is coming. Nothing is coming for
    // a pack no service has given a URL for, and nothing more is coming for
    // one whose fetch is settled -- both get the placeholder.
    if !expected || banner_is_lost(state, key) {
        // The site's own placeholder, dim. It says "this pack has no picture",
        // which is the truth and is not a thing that changes.
        actors.push(act!(sprite("content_browser/nobanner.png"):
            align(0.5, 0.5): xy(x + w * 0.5, y + h * 0.5): setsize(h * 2.5, h):
            diffuse(1.0, 1.0, 1.0, 0.17): z(z)
        ));
        return;
    }
    actors.push(spinner::accent_actor(
        x + w * 0.5,
        y + h * 0.5,
        (h * 0.7).clamp(16.0, 36.0),
        z,
        accent,
    ));
}

/// Draw one piece of fetched artwork into a box, aspect preserved and centred.
///
/// Returns whether anything was drawn, so the caller can say what is happening
/// in the space instead.
pub(super) fn push_art_fitted(
    actors: &mut Vec<Actor>,
    key: u64,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    z: i16,
) -> bool {
    let name = banner_key(key);
    // texture_dims is registered synchronously by queue_texture_upload, so this
    // answers true on the frame the art is queued -- before the GPU texture
    // exists. Drawing early is safe: a sprite with no texture yet is skipped.
    let Some(meta) = deadlib_assets::texture_dims(name.as_str()) else {
        return false;
    };
    let zoom = lo::fit_zoom(meta.w as f32, meta.h as f32, w, h);
    actors.push(banner::sprite(
        name,
        x + w * 0.5,
        y + h * 0.5,
        meta.w as f32 * zoom,
        meta.h as f32 * zoom,
        1.0,
        z,
    ));
    true
}

// --- the footer ---------------------------------------------------------------

fn push_footer(actors: &mut Vec<Actor>, state: &State, w: f32) {
    actors.push(act!(text:
        font("miso"): settext(footer_hint(state)):
        align(0.5, 0.5): xy(w * 0.5, lo::VERSION_Y): zoom(0.5): horizalign(center):
        maxwidth(w - 160.0):
        diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]): z(Z_TEXT)
    ));
    // Centred, under the hint line.
    //
    // Both bottom corners belong to the engine's version watermark, and which
    // corner it takes is a setting -- so a credit anchored to either one
    // overlaps it for half the people who see it.
    actors.push(act!(text:
        font("miso"): settext("pack data from stepmaniaonline.net"):
        align(0.5, 0.5): xy(w * 0.5, lo::VERSION_Y + 14.0): zoom(lo::VERSION_ZOOM):
        horizalign(center):
        diffuse(0.55, 0.55, 0.55, 0.75): z(Z_TEXT)
    ));
}

/// What the keys do here, which is different in every zone -- and the reason
/// the navigation can afford to be unusual at all.
fn footer_hint(state: &State) -> String {
    let exit = if state.zone == Zone::Tabs {
        "BACK exit"
    } else {
        "BACK back"
    };
    let body = match state.zone {
        Zone::Detail if state.song_menu.is_some() => "LEFT/RIGHT choose   START go",
        Zone::Detail if super::preview::chart_showing(state) => {
            "UP/DOWN difficulty   SELECT/START stop the preview"
        }
        Zone::Detail if super::preview::busy(state) => {
            "UP/DOWN songs   SELECT/START stop the preview"
        }
        Zone::Detail if state.detail_on_button => {
            "START download this pack   DOWN back to the songs"
        }
        // No song to act on: START is the pack's download, and SELECT asks
        // for a page that never came, in the original's DetailLost words.
        Zone::Detail if state.page.page.is_none() && state.page.phase == PagePhase::Error => {
            "SELECT try again   START download"
        }
        Zone::Detail
            if state
                .page
                .page
                .as_ref()
                .is_none_or(|page| page.songs.is_empty()) =>
        {
            "START download"
        }
        Zone::Detail => "UP/DOWN songs   LEFT/RIGHT page   SELECT preview   START song options",
        Zone::Tabs if tab(state) == Tab::Search => "LEFT/RIGHT views   START type a search",
        Zone::Tabs => "LEFT/RIGHT views   DOWN open   SELECT reload",
        Zone::Years => "LEFT/RIGHT year   DOWN packs   UP views   SELECT reload",
        Zone::Featured => "LEFT/RIGHT packs   START details   DOWN the list",
        Zone::DoublesPick => "LEFT/RIGHT list   DOWN open   UP views",
        Zone::DoublesRows => "UP/DOWN browse   LEFT/RIGHT page   START details",
        Zone::Installed => "UP/DOWN  LEFT/RIGHT browse   START sync   SELECT delete",
        Zone::List => "UP/DOWN browse   LEFT/RIGHT page   START details   SELECT reload",
    };
    format!("{body}   {exit}")
}

// --- transitions --------------------------------------------------------------

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(lo::ROWS * 8 + 120);
    push_actors(&mut actors, state, asset_manager, Default::default());
    actors
}

/// A line's width in logical pixels at zoom 1, as the font will lay it out.
fn text_width(asset_manager: &AssetManager, font: &str, text: &str) -> f32 {
    asset_manager.with_fonts(|all_fonts| {
        asset_manager
            .with_font(font, |measure| {
                deadlib_present::font::measure_line_width_logical(measure, text, all_fonts) as f32
            })
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0)
    })
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commify_groups_from_the_right() {
        assert_eq!(commify(0), "0");
        assert_eq!(commify(999), "999");
        assert_eq!(commify(1000), "1,000");
        assert_eq!(commify(9639), "9,639");
        assert_eq!(commify(1_234_567), "1,234,567");
    }

    /// The original's `FormatBytes`: two decimals at GB, one at MB.
    #[test]
    fn sizes_read_the_way_the_original_prints_them() {
        assert_eq!(format_bytes(640 * 1024 * 1024), "640.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(419_304_234), "399.9 MB");
    }

    #[test]
    fn dates_are_spelled_out_and_anything_else_is_left_alone() {
        assert_eq!(format_date("2026-03-14", false), "March 14, 2026");
        assert_eq!(format_date("2026-03-14", true), "Mar 14, 2026");
        assert_eq!(format_date("Mar. 14, 2026", false), "Mar. 14, 2026");
        assert_eq!(format_date("", false), "");
    }

    #[test]
    fn absent_catalogue_values_are_not_shown_verbatim() {
        assert!(!meaningful("None"));
        assert!(!meaningful("null"));
        assert!(!meaningful("n/a"));
        assert!(!meaningful("   "));
        assert!(meaningful("technical"));
    }

    /// The ramp has to climb through its buckets, or a harder chart can come
    /// out a cooler colour than an easier one.
    #[test]
    fn the_difficulty_ramp_climbs_without_repeating() {
        let low = meter_color(1);
        let mid = meter_color(12);
        let high = meter_color(20);
        assert_ne!(low, mid);
        assert_ne!(mid, high);
        assert!(low[1] > high[1], "green at the bottom, red at the top");
    }

    /// The detail page's hint says what its keys do now: the song keys, the
    /// button's, or -- with no song list -- the pack's download and a retry.
    #[test]
    fn the_detail_hint_follows_what_start_would_do() {
        use deadsync_online::pack_page::{PackPage, PageSnapshot, SongRow};
        let mut state = super::super::state::init();
        state.zone = Zone::Detail;
        state.page = std::sync::Arc::new(PageSnapshot {
            phase: PagePhase::Error,
            pack_id: 7,
            page: None,
            message: Some("timed out".to_owned()),
            revision: 1,
        });
        assert!(footer_hint(&state).starts_with("SELECT try again   START download"));

        state.page = std::sync::Arc::new(PageSnapshot {
            phase: PagePhase::Ready,
            pack_id: 7,
            page: Some(std::sync::Arc::new(PackPage {
                songs: vec![SongRow {
                    title: "Song A".to_owned(),
                    ..SongRow::default()
                }],
                ..PackPage::default()
            })),
            message: None,
            revision: 2,
        });
        assert!(footer_hint(&state).contains("START song options"));
        state.detail_on_button = true;
        assert!(footer_hint(&state).starts_with("START download this pack"));
    }

    /// Each step of the sync dialog says what it is about to do, or doing.
    #[test]
    fn the_sync_dialog_names_each_step() {
        let mut state = super::super::state::init();
        let mut dialog = SyncDialog {
            group: "Old Pack".to_owned(),
            songs: 20,
            declared: SyncPref::Itg,
            check: PackCheck::Usable,
            action: SyncAction::Shift,
            record: SyncPref::Itg,
            step: SyncStep::Choose,
        };
        let texts = |state: &State, dialog: &SyncDialog| {
            let mut actors = Vec::new();
            push_sync_dialog(&mut actors, state, dialog, 854.0, 480.0);
            actors
                .iter()
                .filter_map(|actor| match actor {
                    Actor::Text { content, .. } => Some(content.as_str().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let choose = texts(&state, &dialog);
        assert!(choose.iter().any(|text| text == "SYNC THIS PACK"));
        assert!(choose.iter().any(|text| text == "MEASURE WITH NULL-OR-DIE"));
        assert!(choose.iter().any(|text| text == "SHIFT ITG TO NULL"));
        assert!(
            !choose
                .iter()
                .any(|text| text.starts_with("RECORD IN PACK.INI"))
        );
        state.pack_ini_offsets_on = true;
        assert!(
            texts(&state, &dialog)
                .iter()
                .any(|text| text.starts_with("RECORD IN PACK.INI"))
        );

        dialog.step = SyncStep::ConfirmShift;
        assert!(
            texts(&state, &dialog)
                .iter()
                .any(|text| text == "SHIFT ITG TO NULL?")
        );
        dialog.step = SyncStep::Working {
            frames: 0,
            sent: false,
        };
        assert!(
            texts(&state, &dialog)
                .iter()
                .any(|text| text == "SHIFTING ITG TO NULL")
        );

        // recorded NULL, the shift is the way back
        dialog.declared = SyncPref::Null;
        dialog.step = SyncStep::Choose;
        assert!(
            texts(&state, &dialog)
                .iter()
                .any(|text| text == "SHIFT NULL TO ITG")
        );

        // and a pack that cannot be changed says why
        dialog.check = PackCheck::Refused("'Old Pack' is in a read-only song folder".to_owned());
        assert!(
            texts(&state, &dialog)
                .iter()
                .any(|text| text.contains("cannot be changed here"))
        );
    }
}

#[cfg(test)]
#[path = "chrome_data_original.rs"]
mod course_data_original;

#[cfg(test)]
#[path = "chrome_data_perf.rs"]
mod course_data_perf;
