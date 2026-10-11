//! The chart preview window, and the song menu that opens it.
//!
//! The original's window, at the original's size and place: 304 by 380, centred
//! over the pack page, receptors near the top and notes rising to meet them at
//! a fixed C516 at half size. The arrows are the reader's own noteskin, drawn a
//! piece at a time -- the same pieces Player Options previews with -- rather
//! than a notefield, which would mean standing up most of gameplay to show a
//! twenty-second picture.
//!
//! What the window draws a skin with is decided here too, for the shell's
//! loader to make ready off the game thread: the textures of those pieces and
//! nothing else, and their model geometry, built once.

use std::fmt::Write as _;
use std::sync::Arc;

use deadlib_present::actors::Actor;
use deadsync_assets::noteskin::{Noteskin, SpriteSlot};
use deadsync_notefield::ModelMeshCache;
use deadsync_online::smo_songs::{PreviewChart, PreviewPhase};

use super::preview::{self, ARROW_PX, MAX_LANES, PX_PER_SECOND, Preview};
use super::render::{accent, format_bytes};
use super::spinner;
use super::state::{SmoSync, State, focused_pack, install_for, is_installed, smo_sync_of};
use crate::act;
use crate::color;
use crate::screens::components::shared::noteskin_draw;

const PANEL_W: f32 = 304.0;
const PANEL_H: f32 = 380.0;
const PANEL_Y: f32 = 48.0;
/// Where the receptors sit, and where notes come into view, from the panel top.
const RECEPTOR_DY: f32 = 96.0;
const ENTRY_DY: f32 = PANEL_H - 34.0;
const EQ_BARS: usize = 28;
/// The skin's columns the window draws: doubles reuses them, lane % 4.
const SKIN_COLS: usize = 4;
/// How long a struck receptor swells and glows, the original's `Snd.HIT`, and
/// by how much: it reads as a step whatever the skin's own press looks like.
/// How far past its receptor a mine carries on before it has faded out: one
/// arrow, which is as far as it can go before the difficulty chips above.
const MINE_EXIT_PX: f32 = ARROW_PX;
const HIT_SECONDS: f32 = 0.085;
const HIT_SWELL: f32 = 0.30;
const HIT_GLOW: f32 = 0.75;

/// Above the pack page, under any popup.
const Z_WINDOW: i16 = 40;
const Z_RECEPTOR: i16 = 44;
const Z_NOTE: i16 = 46;
const Z_EXPLOSION: i16 = 48;
const Z_WINDOW_TEXT: i16 = 52;
const Z_MENU_SCRIM: i16 = 80;
const Z_MENU_PANEL: i16 = 84;
const Z_MENU_TEXT: i16 = 90;

// --- the skin, made ready off the game thread ----------------------------------

/// A noteskin's model geometry for the window, built where the skin is loaded
/// rather than on the game thread: only the model slots the window draws,
/// registered and then sealed, the way gameplay's notefield cache is. Opaque:
/// the shell only carries it from its worker to [`super::set_preview_skin`].
pub struct PreviewSkinModels(ModelMeshCache);

impl PreviewSkinModels {
    pub(super) fn into_cache(self) -> ModelMeshCache {
        self.0
    }
}

/// Build a skin's [`PreviewSkinModels`]. For the skin loader's worker: a
/// model skin's geometry is real work, and nothing here touches the GPU.
#[must_use]
pub fn preview_skin_models(skin: &Noteskin) -> PreviewSkinModels {
    let mut slots: Vec<&SpriteSlot> = Vec::new();
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot| {
        if slot.model.is_some() && !slots.iter().any(|known| std::ptr::eq(*known, slot)) {
            slots.push(slot);
        }
    });
    // Each SpriteSlot (including a clone) owns a unique stable ID.
    // Borrowed identity therefore deduplicates exactly the cache's keys.
    let mut cache = ModelMeshCache::with_capacity(slots.len());
    for slot in slots {
        let _ = cache.prewarm_slot(slot);
    }
    cache.seal();
    cache.reset_stats();
    PreviewSkinModels(cache)
}

/// Every texture the window draws a skin with, once each, and whether a model
/// samples it: the pieces of four columns -- taps and lifts at every
/// quantization, mines, receptors, the W1 explosion -- and not the whole skin,
/// most of which (holds, rolls, the other judgments) the window never shows.
#[must_use]
pub fn preview_skin_textures(skin: &Noteskin) -> Vec<(Arc<str>, bool)> {
    let mut textures: Vec<(Arc<str>, bool)> = Vec::new();
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot: &SpriteSlot| {
        for texture_slot in std::iter::once(slot).chain(slot.model_additive.as_deref()) {
            for key in std::iter::once(texture_slot.texture_key_shared())
                .chain(texture_slot.model_texture_keys.iter().cloned())
            {
                match textures.iter_mut().find(|(known, _)| *known == key) {
                    Some((_, model)) => *model |= slot.model.is_some(),
                    None => textures.push((key, slot.model.is_some())),
                }
            }
        }
    });
    textures
}

// --- the window ----------------------------------------------------------------

/// The window, while a preview is up or fading.
pub(super) fn push_window(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let Some(preview) = state.preview.as_ref() else {
        return;
    };
    let alpha = preview.fade.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let accent = accent(state);
    let cx = w * 0.5;
    let x0 = cx - PANEL_W * 0.5;
    let snapshot = preview::snapshot_for(state, preview);
    let chart =
        snapshot.and_then(|snapshot| preview.chart.and_then(|index| snapshot.charts.get(index)));

    // The panel: a dark plate with a thin accent edge.
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x0 - 1.0, PANEL_Y - 1.0): zoomto(PANEL_W + 2.0, PANEL_H + 2.0):
        diffuse(accent[0], accent[1], accent[2], 0.5 * alpha): z(Z_WINDOW)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x0, PANEL_Y): zoomto(PANEL_W, PANEL_H):
        diffuse(0.03, 0.03, 0.05, 0.97 * alpha): z(Z_WINDOW + 1)
    ));

    actors.push(act!(text:
        font("wendy"): settext("CHART PREVIEW".to_owned()):
        align(0.5, 0.5): xy(cx, PANEL_Y + 14.0): zoom(0.38): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], alpha): z(Z_WINDOW_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(preview.title.clone()):
        align(0.5, 0.5): xy(cx, PANEL_Y + 32.0): zoom(0.6): horizalign(center):
        maxwidth(PANEL_W - 20.0):
        diffuse(1.0, 1.0, 1.0, alpha): z(Z_WINDOW_TEXT)
    ));
    // The difficulty and the chips once the sample plays, as the original's
    // appear at its Begin; the tempo from the start.
    let shown = chart.filter(|_| preview.playing);
    if let Some(line) = chart_line(state, preview, shown, snapshot.map_or(0.0, |s| s.bpm)) {
        actors.push(act!(text:
            font("miso"): settext(line):
            align(0.5, 0.5): xy(cx, PANEL_Y + 48.0): zoom(0.45): horizalign(center):
            maxwidth(PANEL_W - 20.0):
            diffuse(0.8, 0.8, 0.8, alpha): z(Z_WINDOW_TEXT)
        ));
    }
    if preview.playing
        && let Some(snapshot) = snapshot
        && !snapshot.charts.is_empty()
    {
        push_chips(actors, state, &snapshot.charts, preview.chart, cx, alpha);
    }

    // The chart's own lanes once there is one; a guess only until then, so
    // the guess costs nothing once the window is playing.
    let lanes = match chart {
        Some(chart) => usize::from(chart.lanes).clamp(1, MAX_LANES),
        None => preview::predicted_lanes(state, preview),
    };
    let receptor_y = PANEL_Y + RECEPTOR_DY;
    let lane_x = |lane: usize| cx + (lane as f32 - (lanes as f32 - 1.0) * 0.5) * ARROW_PX;
    let time = preview::elapsed(state);
    let skin = state.preview_skin.as_deref();
    // Borrowed once for the frame: every model the skin draws below shares
    // its geometry, built when the skin was loaded.
    let mut models = skin.map(|_| state.preview_model_cache.borrow_mut());
    let beat = preview::beat(state).unwrap_or(0.0);
    let now = time.unwrap_or(0.0);

    // Receptors, lit by their last hit: the skin's own press, and over it the
    // original's swell and glow, which read as a step on any skin.
    for lane in 0..lanes {
        let press_age = preview.hit_at[lane]
            .map(|at| now - at)
            .filter(|age| *age >= 0.0);
        let hit = press_age.map_or(0.0, |age| (1.0 - age / HIT_SECONDS).max(0.0));
        let center = [lane_x(lane), receptor_y];
        match skin {
            Some(skin) => {
                let first = actors.len();
                noteskin_draw::draw_receptor(
                    actors,
                    skin,
                    lane % SKIN_COLS,
                    center,
                    ARROW_PX * (1.0 + HIT_SWELL * hit),
                    alpha,
                    Z_RECEPTOR,
                    now,
                    beat,
                    press_age,
                    models.as_deref_mut(),
                );
                if hit > 0.0 {
                    glow_white(&mut actors[first..], HIT_GLOW * hit);
                }
            }
            None => push_plain_receptor(actors, center, alpha, hit),
        }
    }

    // Notes rising to them. A forward-only cursor: nothing behind the
    // receptors is walked, and the walk stops at the first note still below
    // the window.
    if let (Some(chart), Some(time)) = (chart, time) {
        let entry_y = PANEL_Y + ENTRY_DY;
        for note in chart.notes.get(preview.passed..).unwrap_or_default() {
            let y = receptor_y + (note.time - time) * PX_PER_SECOND;
            if y > entry_y {
                break;
            }
            let quant = usize::from(note.quant);
            for lane in 0..lanes.min(MAX_LANES) {
                let bit = 1u8 << lane;
                if note.cols & bit == 0 {
                    continue;
                }
                let center = [lane_x(lane), y];
                let col = lane % SKIN_COLS;
                match skin {
                    Some(skin) if note.mines & bit != 0 => noteskin_draw::draw_mine(
                        actors,
                        skin,
                        col,
                        center,
                        ARROW_PX,
                        alpha,
                        Z_NOTE,
                        now,
                        beat,
                        models.as_deref_mut(),
                    ),
                    Some(skin) => noteskin_draw::draw_tap(
                        actors,
                        skin,
                        col,
                        quant,
                        note.lifts & bit != 0,
                        center,
                        ARROW_PX,
                        alpha,
                        Z_NOTE,
                        now,
                        beat,
                        models.as_deref_mut(),
                    ),
                    // The original's own arrow is for taps: without the
                    // skin's mine, a mine is not drawn at all.
                    None if note.mines & bit != 0 => {}
                    None => push_plain_note(actors, center, quant, alpha),
                }
            }
        }

        // Mines are not stepped on: they carry on past the receptors, as an
        // avoided mine does in play, fading out before the chips above. Walked
        // back from the cursor, and only as far as one can still be seen.
        if let Some(skin) = skin {
            let behind = chart.notes.get(..preview.passed).unwrap_or_default();
            for note in behind.iter().rev() {
                let risen = (time - note.time) * PX_PER_SECOND;
                if risen > MINE_EXIT_PX {
                    break;
                }
                if note.mines == 0 {
                    continue;
                }
                let risen = risen.max(0.0);
                let fade = alpha * (1.0 - risen / MINE_EXIT_PX);
                for lane in 0..lanes.min(MAX_LANES) {
                    if note.mines & (1u8 << lane) == 0 {
                        continue;
                    }
                    noteskin_draw::draw_mine(
                        actors,
                        skin,
                        lane % SKIN_COLS,
                        [lane_x(lane), receptor_y - risen],
                        ARROW_PX,
                        fade,
                        Z_NOTE,
                        now,
                        beat,
                        models.as_deref_mut(),
                    );
                }
            }
        }

        // The explosions the hits set off.
        if let Some(skin) = skin {
            for lane in 0..lanes {
                let Some(age) = preview.hit_at[lane].map(|at| now - at) else {
                    continue;
                };
                if age < 0.0 || age > noteskin_draw::explosion_duration(skin, lane % SKIN_COLS) {
                    continue;
                }
                noteskin_draw::draw_explosion(
                    actors,
                    skin,
                    lane % SKIN_COLS,
                    [lane_x(lane), receptor_y],
                    ARROW_PX,
                    Z_EXPLOSION,
                    age,
                    beat,
                    models.as_deref_mut(),
                );
            }
        }
        if chart.notes.is_empty() {
            actors.push(act!(text:
                font("miso"): settext("nothing in this part of the song".to_owned()):
                align(0.5, 0.5): xy(cx, PANEL_Y + 230.0): zoom(0.5): horizalign(center):
                diffuse(0.75, 0.75, 0.75, alpha): z(Z_WINDOW_TEXT)
            ));
        }
    }

    // Loading: a spinner and how much of the sample has arrived.
    let loading =
        !preview.closing && snapshot.is_none_or(|snapshot| snapshot.phase == PreviewPhase::Loading);
    if loading {
        let progress = snapshot.and_then(|snapshot| snapshot.progress);
        actors.push(spinner::accent_actor(
            cx,
            PANEL_Y + 210.0,
            30.0,
            Z_WINDOW_TEXT,
            accent,
        ));
        let label = match progress {
            Some(fraction) => format!("loading sample  {:.0}%", fraction * 100.0),
            None => format!("loading sample{}", spinner::ellipsis()),
        };
        actors.push(act!(text:
            font("miso"): settext(label):
            align(0.5, 0.5): xy(cx, PANEL_Y + 242.0): zoom(0.5): horizalign(center):
            diffuse(0.8, 0.8, 0.8, alpha): z(Z_WINDOW_TEXT)
        ));
        if let Some(fraction) = progress {
            let bar_w = PANEL_W - 80.0;
            actors.push(act!(quad:
                align(0.0, 0.5): xy(cx - bar_w * 0.5, PANEL_Y + 260.0): zoomto(bar_w, 4.0):
                diffuse(1.0, 1.0, 1.0, 0.12 * alpha): z(Z_WINDOW_TEXT)
            ));
            actors.push(act!(quad:
                align(0.0, 0.5): xy(cx - bar_w * 0.5, PANEL_Y + 260.0):
                zoomto(bar_w * fraction.clamp(0.0, 1.0), 4.0):
                diffuse(accent[0], accent[1], accent[2], alpha): z(Z_WINDOW_TEXT)
            ));
        }
    }

    // The equalizer along the bottom: cosmetic, keyed to the beat, as the
    // original's is.
    if time.is_some() {
        let bar_w = (PANEL_W - 24.0) / EQ_BARS as f32;
        for bar in 0..EQ_BARS {
            let phase = beat * std::f32::consts::TAU + bar as f32 * 0.9;
            let height = 3.0 + 14.0 * (0.5 + 0.5 * (phase.sin() * (bar as f32 * 1.7).cos())).abs();
            actors.push(act!(quad:
                align(0.0, 1.0): xy(x0 + 12.0 + bar as f32 * bar_w, PANEL_Y + PANEL_H - 6.0):
                zoomto(bar_w - 2.0, height):
                diffuse(accent[0], accent[1], accent[2], 0.45 * alpha): z(Z_WINDOW + 2)
            ));
        }
    }
}

/// A white glow of `amount` over pieces just drawn, the original's
/// `rec:glow(1, 1, 1, ...)` on the whole receptor. Each piece glows as far as
/// it is opaque, as a glow under a diffuse alpha does, so a faint layer -- an
/// idle flash between beats -- does not light up as a solid square; and a
/// glow the skin itself is already showing brighter is left alone.
fn glow_white(actors: &mut [Actor], amount: f32) {
    for actor in actors {
        if let Actor::Sprite { tint, glow, .. } | Actor::TexturedMesh { tint, glow, .. } = actor {
            let lit = amount * tint[3];
            if lit > glow[3] {
                *glow = [1.0, 1.0, 1.0, lit];
            }
        }
    }
}

/// A difficulty as the wheel writes it, the original's `Snd.ChartLabel`: the
/// simfile's own name -- CHALLENGE, not EXPERT -- or the style where it has
/// none, and the meter where there is one. Written onto the end of `out`, so
/// a line that carries it is built in one buffer.
fn write_chart_label(out: &mut String, chart: &PreviewChart) {
    if chart.difficulty.is_empty() {
        out.push_str(if chart.lanes == 8 {
            "DOUBLES"
        } else {
            "SINGLES"
        });
    } else {
        out.extend(chart.difficulty.chars().flat_map(char::to_uppercase));
    }
    if chart.meter > 0 {
        let _ = write!(out, "  {}", chart.meter);
    }
}

#[cfg(test)]
fn chart_label(chart: &PreviewChart) -> String {
    let mut label = String::new();
    write_chart_label(&mut label, chart);
    label
}

/// `CHALLENGE  12   -   140 bpm`: the chart on show, and the tempo -- the
/// pack page's, which is a range as often as a number, and the simfile's own
/// only where the page said nothing. Either half alone when that is all there
/// is, as the original joins whichever it has.
fn chart_line(
    state: &State,
    preview: &Preview,
    chart: Option<&PreviewChart>,
    simfile_bpm: f32,
) -> Option<String> {
    let page_bpm = state
        .page
        .page
        .as_ref()
        .and_then(|page| page.songs.get(preview.row))
        .filter(|song| song.title == preview.title && !song.bpm.is_empty())
        .map(|song| song.bpm.as_str());
    let mut line = String::with_capacity(40);
    if let Some(chart) = chart {
        write_chart_label(&mut line, chart);
    }
    if page_bpm.is_some() || simfile_bpm > 0.0 {
        if !line.is_empty() {
            line.push_str("   -   ");
        }
        match page_bpm {
            Some(bpm) => line.push_str(bpm),
            None => {
                let _ = write!(line, "{simfile_bpm:.0}");
            }
        }
        line.push_str(" bpm");
    }
    (!line.is_empty()).then_some(line)
}

/// One chip per chart, coloured like the song wheel, the showing one outlined,
/// with the original's `UP DOWN ... difficulty` against the row when there is
/// more than one to move between.
fn push_chips(
    actors: &mut Vec<Actor>,
    state: &State,
    charts: &[PreviewChart],
    showing: Option<usize>,
    cx: f32,
    alpha: f32,
) {
    const CHIP_H: f32 = 15.0;
    const GAP: f32 = 4.0;
    // Room either side for the labels, so a long row narrows its chips
    // rather than pushing the labels out of the window.
    const LABEL_ROOM: f32 = 130.0;
    let count = charts.len() as f32;
    let chip_w = ((PANEL_W - LABEL_ROOM) / count - GAP).clamp(16.0, 46.0);
    let row_w = count * (chip_w + GAP) - GAP;
    let y = PANEL_Y + 66.0;
    for (index, chart) in charts.iter().enumerate() {
        let x = cx - row_w * 0.5 + index as f32 * (chip_w + GAP);
        let rgba = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        let picked = showing == Some(index);
        if picked {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(x - 1.5, y): zoomto(chip_w + 3.0, CHIP_H + 3.0):
                diffuse(1.0, 1.0, 1.0, alpha): z(Z_WINDOW + 3)
            ));
        }
        let fill_alpha = if picked { 1.0 } else { 0.18 };
        actors.push(act!(quad:
            align(0.0, 0.5): xy(x, y): zoomto(chip_w, CHIP_H):
            diffuse(rgba[0], rgba[1], rgba[2], fill_alpha * alpha): z(Z_WINDOW + 4)
        ));
        // The meter, or the difficulty's first letters where the simfile
        // gave none, as the original's chips read.
        let mut label = String::with_capacity(4);
        if chart.doubles {
            label.push('D');
        }
        if chart.meter > 0 {
            let _ = write!(label, "{}", chart.meter);
        } else {
            label.extend(
                chart
                    .difficulty
                    .chars()
                    .take(3)
                    .flat_map(char::to_uppercase),
            );
        }
        let ink = if picked { 0.08 } else { 0.9 };
        actors.push(act!(text:
            font("miso"): settext(label):
            align(0.5, 0.5): xy(x + chip_w * 0.5, y): zoom(0.4): horizalign(center):
            maxwidth(chip_w - 2.0):
            diffuse(ink, ink, ink, alpha): z(Z_WINDOW_TEXT)
        ));
    }
    if charts.len() > 1 {
        actors.push(act!(text:
            font("miso"): settext("&MENUUP;&MENUDOWN;".to_owned()):
            align(1.0, 0.5): xy(cx - row_w * 0.5 - 5.0, y): zoom(0.34): horizalign(right):
            diffuse(0.75, 0.75, 0.75, alpha): z(Z_WINDOW_TEXT)
        ));
        actors.push(act!(text:
            font("miso"): settext("difficulty".to_owned()):
            align(0.0, 0.5): xy(cx + row_w * 0.5 + 5.0, y): zoom(0.34): horizalign(left):
            diffuse(0.6, 0.6, 0.6, alpha): z(Z_WINDOW_TEXT)
        ));
    }
}

/// The original's quantization colours, for when the noteskin is not ready.
const QUANT_RGB: [[f32; 3]; 9] = [
    [0.95, 0.25, 0.25],
    [0.25, 0.45, 0.95],
    [0.70, 0.30, 0.90],
    [0.95, 0.85, 0.25],
    [0.95, 0.45, 0.75],
    [0.95, 0.60, 0.20],
    [0.30, 0.85, 0.90],
    [0.40, 0.85, 0.40],
    [0.60, 0.60, 0.60],
];

/// A receptor without the skin: the same swell, and a light for the glow.
fn push_plain_receptor(actors: &mut Vec<Actor>, center: [f32; 2], alpha: f32, hit: f32) {
    let size = ARROW_PX * 0.85 * (1.0 + HIT_SWELL * hit);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(center[0], center[1]): zoomto(size, size):
        diffuse(1.0, 1.0, 1.0, (0.18 + 0.5 * hit) * alpha): z(Z_RECEPTOR)
    ));
}

fn push_plain_note(actors: &mut Vec<Actor>, center: [f32; 2], quant: usize, alpha: f32) {
    let rgb = QUANT_RGB[quant.min(QUANT_RGB.len() - 1)];
    let size = ARROW_PX * 0.8;
    actors.push(act!(quad:
        align(0.5, 0.5): xy(center[0], center[1]): zoomto(size, size):
        diffuse(rgb[0], rgb[1], rgb[2], alpha): z(Z_NOTE)
    ));
}

// --- the song menu -------------------------------------------------------------

/// "Listen or Download?": preview this song, get just this song, or the pack.
pub(super) fn push_song_menu(actors: &mut Vec<Actor>, state: &State, w: f32, h: f32) {
    let Some(menu) = state.song_menu.as_ref() else {
        return;
    };
    let Some(pack) = focused_pack(state) else {
        return;
    };
    let Some((song, artist)) = state.page.page.as_ref().and_then(|page| {
        let song = page.songs.get(state.song_pick)?;
        Some((song, super::preview::request_artist(&page.songs, song)))
    }) else {
        return;
    };
    let accent = accent(state);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let panel_w = (w * 0.72).min(600.0);

    // The original's body under the song's own line: the pack's size, and
    // the two things worth knowing before choosing -- that the pack is here
    // already, or on its way.
    const LINE_H: f32 = 14.0;
    let installed = is_installed(state, pack);
    let downloading = install_for(state, pack).is_some_and(|install| {
        matches!(
            install.phase,
            deadsync_online::stepmaniaonline::InstallPhase::Queued
                | deadsync_online::stepmaniaonline::InstallPhase::Downloading
                | deadsync_online::stepmaniaonline::InstallPhase::Extracting
        )
    });
    let itg = itg_sync(state, pack);
    let lines = 1 + usize::from(itg) + usize::from(installed) + usize::from(downloading);
    let body_h = lines as f32 * LINE_H;
    let panel_h = 196.0 + body_h;
    let top = cy - panel_h * 0.5;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.62): z(Z_MENU_SCRIM)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(panel_w + 4.0, panel_h + 4.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MENU_PANEL)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(panel_w, panel_h):
        diffuse(0.05, 0.05, 0.07, 1.0): z(Z_MENU_PANEL + 1)
    ));
    actors.push(act!(text:
        font("wendy"): settext("LISTEN OR DOWNLOAD?".to_owned()):
        align(0.5, 0.5): xy(cx, top + 22.0): zoom(0.45): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MENU_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(song.title.clone()):
        align(0.5, 0.5): xy(cx, top + 46.0): zoom(0.65): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MENU_TEXT)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!(
            "from {}   -   a single song lands in \"{}\"",
            pack.name,
            deadsync_online::smo_songs::SINGLES_GROUP
        )):
        align(0.5, 0.5): xy(cx, top + 66.0): zoom(0.45): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(0.7, 0.7, 0.7, 1.0): z(Z_MENU_TEXT)
    ));
    let mut line_y = top + 66.0 + LINE_H;
    let size = format!(
        "{}  -  {} songs",
        format_bytes(pack.size_bytes),
        pack.song_count
    );
    for (show, line) in [
        (true, size.as_str()),
        (
            itg,
            "taken as ITG-synced: its offset is moved 9 ms to NULL as it installs",
        ),
        (
            installed,
            "already in your library - remove it from the Installed tab if you want it again",
        ),
        (downloading, "(this pack is downloading already)"),
    ] {
        if !show {
            continue;
        }
        actors.push(act!(text:
            font("miso"): settext(line.to_owned()):
            align(0.5, 0.5): xy(cx, line_y): zoom(0.45): horizalign(center):
            maxwidth(panel_w - 40.0):
            diffuse(0.7, 0.7, 0.7, 1.0): z(Z_MENU_TEXT)
        ));
        line_y += LINE_H;
    }

    let known = deadsync_online::smo_songs::runtime_known_charts(pack.id, &song.title, artist);
    let mut preview_label = String::from("Preview");
    if let Some(chart) = known
        .as_deref()
        .and_then(|charts| menu.chart.and_then(|index| charts.get(index)))
    {
        preview_label.push_str("  ");
        write_chart_label(&mut preview_label, chart);
    }
    let pack_label = super::detail::download_label(state, pack).0;
    const CHOICE_W: f32 = 170.0;
    const CHOICE_H: f32 = 34.0;
    let choice_y = top + 112.0 + body_h;
    for (index, label) in [preview_label, "Get this song".to_owned(), pack_label]
        .into_iter()
        .enumerate()
    {
        let picked = menu.choice == index;
        let x = cx + (index as f32 - 1.0) * (CHOICE_W + 12.0);
        let plate = if picked {
            [accent[0], accent[1], accent[2], 0.9]
        } else {
            [1.0, 1.0, 1.0, 0.07]
        };
        let ink = if picked { 0.08 } else { 0.78 };
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x, choice_y): zoomto(CHOICE_W, CHOICE_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_MENU_PANEL + 2)
        ));
        actors.push(act!(text:
            font("miso"): settext(label):
            align(0.5, 0.5): xy(x, choice_y): zoom(0.52): horizalign(center):
            maxwidth(CHOICE_W - 12.0):
            diffuse(ink, ink, ink, 1.0): z(Z_MENU_TEXT)
        ));
    }
    let hint = if menu.choice == 0 && known.as_ref().is_some_and(|charts| charts.len() > 1) {
        "LEFT/RIGHT choose    UP/DOWN difficulty    START go    BACK cancel"
    } else {
        "LEFT/RIGHT choose    START go    BACK cancel"
    };
    actors.push(act!(text:
        font("miso"): settext(hint.to_owned()):
        align(0.5, 0.5): xy(cx, top + 164.0 + body_h): zoom(0.5): horizalign(center):
        diffuse(0.6, 0.6, 0.6, 1.0): z(Z_MENU_TEXT)
    ));
}

/// The sync a single song from this pack is filed under: the original's rule,
/// NULL only when the site says the pack is NULL.
pub(super) fn itg_sync(state: &State, pack: &deadsync_online::stepmaniaonline::PackInfo) -> bool {
    let _ = state;
    !matches!(smo_sync_of(pack), SmoSync::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_online::smo_songs::{PreviewNote, PreviewSnapshot};

    fn chart(doubles: bool, difficulty: &str, meter: u32) -> PreviewChart {
        PreviewChart {
            doubles,
            difficulty: difficulty.to_owned(),
            meter,
            lanes: if doubles { 8 } else { 4 },
            notes: Arc::from(Vec::<PreviewNote>::new()),
        }
    }

    fn dance_skin(name: &str) -> Arc<Noteskin> {
        crate::tests::init_paths();
        deadsync_assets::noteskin::load_itg_skin_cached(
            &deadsync_noteskin::Style {
                num_cols: 4,
                num_players: 1,
            },
            name,
        )
        .unwrap()
    }

    /// The original's `Snd.ChartLabel`: the simfile's name, the style where
    /// it gave none, and no meter where there is none.
    #[test]
    fn charts_are_labelled_as_the_wheel_writes_them() {
        assert_eq!(chart_label(&chart(false, "Challenge", 12)), "CHALLENGE  12");
        assert_eq!(chart_label(&chart(true, "", 0)), "DOUBLES");
        assert_eq!(chart_label(&chart(false, "", 7)), "SINGLES  7");
        assert_eq!(chart_label(&chart(false, "Edit", 0)), "EDIT");
    }

    /// The chart and the tempo, joined, or whichever of them there is.
    #[test]
    fn the_chart_line_says_what_it_knows() {
        let mut state = super::super::state::init();
        preview::start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        let preview = state.preview.clone().expect("started");
        let hard = chart(false, "Hard", 9);
        assert_eq!(
            chart_line(&state, &preview, Some(&hard), 140.0).as_deref(),
            Some("HARD  9   -   140 bpm")
        );
        assert_eq!(
            chart_line(&state, &preview, None, 140.0).as_deref(),
            Some("140 bpm")
        );
        assert_eq!(
            chart_line(&state, &preview, Some(&hard), 0.0).as_deref(),
            Some("HARD  9")
        );
        assert_eq!(chart_line(&state, &preview, None, 0.0), None);
    }

    #[test]
    fn previews_warm_every_model_material_image() {
        let mut skin = (*dance_skin("cyber")).clone();
        let piece = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/model-texture-images/model.txt");
        let slots =
            deadsync_assets::noteskin::load_itg_model_slots(&piece, &piece, &piece).unwrap();
        skin.mine_layers = vec![slots; SKIN_COLS];
        for textures in [
            preview_skin_textures(&skin),
            crate::screens::player_options::noteskin_preview_textures(&skin, 1 << 8),
        ] {
            for image in [
                "frame-red.png",
                "frame-blue.png",
                "alpha-green.png",
                "alpha-white.png",
            ] {
                let key = deadsync_assets::textures::model_texture_key(
                    &deadsync_assets::textures::canonical_texture_key(
                        piece.parent().unwrap().join(image),
                    ),
                );
                assert_eq!(
                    textures
                        .iter()
                        .filter(|(source, model)| **source == key && *model)
                        .count(),
                    1,
                    "every native material image must be resident before preview: {image}"
                );
            }
        }
    }

    /// The loader readies the window's pieces once each, and every model the
    /// window draws is built then rather than while drawing.
    #[test]
    fn the_window_readies_what_it_draws_once() {
        let skin = dance_skin("cyber");
        let textures = preview_skin_textures(&skin);
        assert!(
            textures.iter().any(|(_, model)| *model),
            "cyber's notes are models"
        );
        let mut keys: Vec<_> = textures.iter().map(|(key, _)| key.clone()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), textures.len(), "each texture once");

        let mut cache = preview_skin_models(&skin).into_cache();
        let mut actors = Vec::new();
        for col in 0..SKIN_COLS {
            for quant in [0, 1, 3, 8] {
                noteskin_draw::draw_tap(
                    &mut actors,
                    &skin,
                    col,
                    quant,
                    quant == 3,
                    [100.0, 100.0],
                    ARROW_PX,
                    1.0,
                    Z_NOTE,
                    0.4,
                    0.8,
                    Some(&mut cache),
                );
            }
            noteskin_draw::draw_mine(
                &mut actors,
                &skin,
                col,
                [100.0, 100.0],
                ARROW_PX,
                1.0,
                Z_NOTE,
                0.4,
                0.8,
                Some(&mut cache),
            );
            noteskin_draw::draw_receptor(
                &mut actors,
                &skin,
                col,
                [100.0, 100.0],
                ARROW_PX,
                1.0,
                Z_RECEPTOR,
                0.4,
                0.8,
                Some(0.0),
                Some(&mut cache),
            );
            noteskin_draw::draw_explosion(
                &mut actors,
                &skin,
                col,
                [100.0, 100.0],
                ARROW_PX,
                Z_EXPLOSION,
                0.01,
                0.8,
                Some(&mut cache),
            );
        }
        assert!(
            actors
                .iter()
                .any(|actor| matches!(actor, Actor::TexturedMesh { .. }))
        );
        let stats = cache.stats();
        assert_eq!(stats.misses, 0, "nothing built while drawing");
        assert_eq!(stats.saturated_misses, 0);
    }

    /// A receptor struck a moment ago glows white over the skin's own press,
    /// as the original's does, and is back to the skin's look 85 ms later.
    #[test]
    fn a_struck_receptor_swells_and_glows() {
        let mut state = super::super::state::init();
        state.preview_skin = Some(dance_skin("default"));
        preview::start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        state.pending_songs.clear();
        let mut taps = chart(false, "Challenge", 12);
        taps.notes = Arc::from(vec![PreviewNote {
            time: 0.5,
            cols: 0b0001,
            lifts: 0,
            mines: 0,
            quant: 0,
        }]);
        state.song_preview = Arc::new(PreviewSnapshot {
            phase: PreviewPhase::Ready,
            pack_id: 7,
            title: "Song A".to_owned(),
            length: 15.0,
            bpm: 120.0,
            clip: true,
            charts: Arc::from(vec![taps]),
            audio_path: Some(std::path::PathBuf::from("cache/preview.ogg")),
            ..PreviewSnapshot::default()
        });
        preview::sync(&mut state);
        state.music_time = Some(0.5);
        for _ in 0..20 {
            preview::update(&mut state, 0.02);
        }
        // The brightest white glow on the window: the struck receptor's.
        let white = |state: &State| {
            let mut actors = Vec::new();
            push_window(&mut actors, state, 854.0);
            actors
                .iter()
                .filter_map(|actor| match actor {
                    Actor::Sprite { glow, .. } | Actor::TexturedMesh { glow, .. }
                        if glow[..3] == [1.0; 3] =>
                    {
                        Some(glow[3])
                    }
                    _ => None,
                })
                .fold(0.0_f32, f32::max)
        };
        let struck = white(&state);
        assert!(
            struck > 0.5 && struck <= HIT_GLOW + 1e-4,
            "struck: the receptor glows ({struck})"
        );
        state.music_time = Some(0.5 + HIT_SECONDS + 0.01);
        assert!(white(&state) < 1e-4, "and settles");
    }

    /// A mine is not stepped on: it presses no receptor and sets off nothing,
    /// and carries on past the receptors until it has faded out above them.
    #[test]
    fn a_mine_scrolls_past_its_receptor() {
        let mut state = super::super::state::init();
        state.preview_skin = Some(dance_skin("default"));
        preview::start(&mut state, 7, "Song A".to_owned(), String::new(), 0, None);
        state.pending_songs.clear();
        let mut mines = chart(false, "Challenge", 12);
        mines.notes = Arc::from(vec![PreviewNote {
            time: 0.5,
            cols: 0b0001,
            lifts: 0,
            mines: 0b0001,
            quant: 0,
        }]);
        state.song_preview = Arc::new(PreviewSnapshot {
            phase: PreviewPhase::Ready,
            pack_id: 7,
            title: "Song A".to_owned(),
            length: 15.0,
            bpm: 120.0,
            clip: true,
            charts: Arc::from(vec![mines]),
            audio_path: Some(std::path::PathBuf::from("cache/preview.ogg")),
            ..PreviewSnapshot::default()
        });
        preview::sync(&mut state);
        let drawn = |state: &State| {
            let mut actors = Vec::new();
            push_window(&mut actors, state, 854.0);
            actors
        };
        let notes = |actors: &[Actor]| {
            actors
                .iter()
                .filter(|actor| match actor {
                    Actor::Sprite { z, .. } | Actor::TexturedMesh { z, .. } => *z == Z_NOTE,
                    _ => false,
                })
                .count()
        };
        let explosions = |actors: &[Actor]| {
            actors
                .iter()
                .filter(|actor| match actor {
                    Actor::Sprite { z, .. } | Actor::TexturedMesh { z, .. } => *z == Z_EXPLOSION,
                    _ => false,
                })
                .count()
        };

        // A moment past the receptor: still there, nothing pressed.
        state.music_time = Some(0.5 + 0.05);
        for _ in 0..5 {
            preview::update(&mut state, 0.01);
        }
        let past = drawn(&state);
        assert_eq!(state.preview.as_ref().map(|p| p.passed), Some(1));
        assert_eq!(state.preview.as_ref().and_then(|p| p.hit_at[0]), None);
        assert!(notes(&past) > 0, "the mine carries on past the receptor");
        assert_eq!(explosions(&past), 0, "and sets nothing off");

        // Further on it has faded out of the window.
        state.music_time = Some(0.5 + 2.0 * MINE_EXIT_PX / PX_PER_SECOND);
        preview::update(&mut state, 0.01);
        assert_eq!(notes(&drawn(&state)), 0, "gone");
    }
}

#[cfg(test)]
mod preview_dataflows {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/perf/preview_models.rs"
    ));
}
