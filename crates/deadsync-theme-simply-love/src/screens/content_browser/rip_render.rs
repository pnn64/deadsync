//! Drawing the Rip Packs tab, ported from the original's
//! `26 screen - rip packs.lua`.
//!
//! Left, the options a rip is made with, a row each and the button under
//! them. Right, the stage: a booster pack that idles, shakes while songs are
//! looked for, tears open when they are found, and deals fifteen cards that
//! turn over as they land. Every coordinate is the original's absolute pixel
//! on the 854x480 basis, and every motion is read from `rip`'s functions of
//! the clock -- nothing here remembers anything.
//!
//! Everything on the tab draws at one layer, in the original's order, so the
//! cards dealt later lie over the ones dealt before and the picker's dim lies
//! over the tab strip. The footer is drawn after it and stays bright, as the
//! original's does.

use super::layout as lo;
use super::render::{Z_MODAL_PANEL, Z_MODAL_TEXT, Z_TEXT, accent, text_width};
use super::rip::{self, CardMotion, Choice, PackMotion, Pick, Row, Stage, TitleFit};
use super::spinner;
use super::state::{State, Zone, banner_is_lost};
use crate::act;
use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadsync_online::rip_packs::{BOOSTER_SONGS, Era, RipSong, SongState};
use std::borrow::Cow;
use std::sync::Arc;

/// The tab's one layer.
const Z: i16 = Z_TEXT;
const GOLD: [f32; 3] = [1.0, 0.8, 0.3];
/// The near-black of a label on a lit plate.
const INK: f32 = 0.08;
/// No limit on a line's width.
const NO_MAX: f32 = 1.0e6;
/// Miso's line spacing, which is how tall a line of it is at zoom 1.
const MISO_LINE: f32 = 24.0;

/// DeadSync's own arrow, wherever the original draws its game's logo.
const LOGO: &str = "content_browser/deadsync_logo.png";
const GLOW: &str = "content_browser/glow.png";
const DISC: &str = "content_browser/disc.png";
const DOWNLOAD: &str = "content_browser/download.png";

// A card's face, top to bottom: the picture, the meter's badge on its lower
// edge, the year, title and credit, and the pack along the foot.
const ART_H: f32 = 40.0;
const ART_Y: f32 = -lo::RIP_CARD_H / 2.0 + 2.0 + ART_H / 2.0;
const BADGE_Y: f32 = -lo::RIP_CARD_H / 2.0 + 2.0 + ART_H;
const INFO_Y: f32 = 22.0;
const CREDIT_ZOOM: f32 = 0.36;
const YEAR_ZOOM: f32 = 0.34;
/// What a card's lines may be, inside its border.
const CARD_TEXT_W: f32 = lo::RIP_CARD_W - 10.0;

/// A StepMania actor frame, done by hand.
///
/// A frame there turns and scales everything in it. A transient actor list
/// has no such frame, so each child's place is worked out in its frame's
/// turned, scaled space, and the child is drawn turned and scaled to match.
/// `rot` is StepMania's degrees, clockwise.
#[derive(Clone, Copy, Debug)]
struct Frame {
    x: f32,
    y: f32,
    rot: f32,
    sx: f32,
    sy: f32,
    alpha: f32,
}

impl Frame {
    const fn at(x: f32, y: f32, rot: f32) -> Self {
        Self {
            x,
            y,
            rot,
            sx: 1.0,
            sy: 1.0,
            alpha: 1.0,
        }
    }

    /// Where a point of this frame lands on screen.
    fn point(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (lx, ly) = (lx * self.sx, ly * self.sy);
        if self.rot == 0.0 {
            return (self.x + lx, self.y + ly);
        }
        let (sin, cos) = self.rot.to_radians().sin_cos();
        (self.x + lx * cos - ly * sin, self.y + lx * sin + ly * cos)
    }

    /// A frame inside this one, uniformly zoomed.
    fn child(&self, lx: f32, ly: f32, rot: f32, zoom: f32, alpha: f32) -> Self {
        let (x, y) = self.point(lx, ly);
        Self {
            x,
            y,
            rot: self.rot + rot,
            sx: self.sx * zoom,
            sy: self.sy * zoom,
            alpha: self.alpha * alpha,
        }
    }
}

/// A solid quad centred on a point of its frame. DeadSync's sprites turn the
/// other way from StepMania's, hence the minus.
fn quad(actors: &mut Vec<Actor>, f: &Frame, lx: f32, ly: f32, w: f32, h: f32, rgba: [f32; 4]) {
    let (x, y) = f.point(lx, ly);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(x, y): zoomto(w * f.sx, h * f.sy): rotationz(-f.rot):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3] * f.alpha): z(Z)
    ));
}

/// A quad shading from `top` down to `bottom`: the bottom colour, with the
/// top one over it fading out toward its foot.
#[allow(clippy::too_many_arguments)]
fn gradient(
    actors: &mut Vec<Actor>,
    f: &Frame,
    lx: f32,
    ly: f32,
    w: f32,
    h: f32,
    top: [f32; 3],
    bottom: [f32; 3],
    alpha: f32,
) {
    quad(
        actors,
        f,
        lx,
        ly,
        w,
        h,
        [bottom[0], bottom[1], bottom[2], alpha],
    );
    let (x, y) = f.point(lx, ly);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(x, y): zoomto(w * f.sx, h * f.sy): rotationz(-f.rot):
        fadebottom(1.0):
        diffuse(top[0], top[1], top[2], alpha * f.alpha): z(Z)
    ));
}

/// A soft bar of light, added to what is under it: the pack's sheen and the
/// gold card's glint.
#[allow(clippy::too_many_arguments)]
fn soft_bar(
    actors: &mut Vec<Actor>,
    f: &Frame,
    lx: f32,
    ly: f32,
    w: f32,
    h: f32,
    rgb: [f32; 3],
    alpha: f32,
) {
    let (x, y) = f.point(lx, ly);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(x, y): zoomto(w * f.sx, h * f.sy): rotationz(-f.rot):
        fadeleft(0.5): faderight(0.5): blend(add):
        diffuse(rgb[0], rgb[1], rgb[2], alpha * f.alpha): z(Z)
    ));
}

/// A picture centred on a point of its frame.
#[allow(clippy::too_many_arguments)]
fn picture(
    actors: &mut Vec<Actor>,
    f: &Frame,
    texture: &'static str,
    lx: f32,
    ly: f32,
    w: f32,
    h: f32,
    rgba: [f32; 4],
) {
    let (x, y) = f.point(lx, ly);
    actors.push(act!(sprite(texture):
        align(0.5, 0.5): xy(x, y): setsize(w * f.sx, h * f.sy): rotationz(-f.rot):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3] * f.alpha): z(Z)
    ));
}

/// The round glow, added to what is under it.
fn glow(actors: &mut Vec<Actor>, x: f32, y: f32, w: f32, h: f32, rgb: [f32; 3], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    actors.push(act!(sprite(GLOW):
        align(0.5, 0.5): xy(x, y): setsize(w, h): blend(add):
        diffuse(rgb[0], rgb[1], rgb[2], alpha): z(Z)
    ));
}

/// A centred line in its frame, turned with it.
#[allow(clippy::too_many_arguments)]
fn line(
    actors: &mut Vec<Actor>,
    f: &Frame,
    font: &'static str,
    text: impl Into<TextContent>,
    lx: f32,
    ly: f32,
    zoom: f32,
    max_w: f32,
    rgba: [f32; 4],
) {
    let (x, y) = f.point(lx, ly);
    let actor = act!(text:
        font(font): settext(text.into()):
        align(0.5, 0.5): xy(x, y): zoomx(zoom * f.sx): zoomy(zoom * f.sy):
        maxwidth(max_w * f.sx): horizalign(center):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3] * f.alpha): z(Z)
    );
    actors.push(turned(actor, f.rot));
}

/// Text turned about its own centre. The text builder has no turn of its
/// own, so it goes into the actor's local transform, which is laid out in
/// StepMania's downward space: a positive angle is clockwise there too.
fn turned(mut actor: Actor, rot: f32) -> Actor {
    if rot != 0.0
        && let Actor::Text {
            local_transform, ..
        } = &mut actor
    {
        *local_transform = glam::Mat4::from_rotation_z(rot.to_radians());
    }
    actor
}

fn rgba(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], alpha]
}

fn scaled(rgb: [f32; 4], by: f32) -> [f32; 3] {
    [rgb[0] * by, rgb[1] * by, rgb[2] * by]
}

fn content(text: Cow<'static, str>) -> TextContent {
    match text {
        Cow::Borrowed(text) => TextContent::Static(text),
        Cow::Owned(text) => TextContent::Owned(text),
    }
}

// --- the tab ------------------------------------------------------------------

/// The options, the stage, and the picker over both while it is open.
pub(super) fn push_rip(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    w: f32,
    h: f32,
) {
    let accent = accent(state);
    actors.reserve(64 + BOOSTER_SONGS * 24);
    push_options(actors, state, accent);
    push_stage(actors, state, accent, asset_manager);
    if state.rip.picking.is_some() {
        push_picker(actors, state, accent, w, h);
    }
}

/// The footer while the cursor is in the options, in the browser's words.
pub(super) fn footer(state: &State) -> &'static str {
    let rip = &state.rip;
    match rip.picking {
        Some(Pick::Eras) => return "UP/DOWN move   START choose   SELECT clear",
        Some(Pick::Authors) => {
            return "UP/DOWN move   LEFT/RIGHT page   START choose   SELECT clear";
        }
        None => {}
    }
    match Row::ALL[rip.row.min(Row::ALL.len() - 1)] {
        Row::Button if rip.stage == Stage::Choosing => {
            "LEFT/RIGHT keep or re-rip   START choose   UP options"
        }
        Row::Reset => "START reset options   UP options",
        Row::Button if rip::busy(rip) => "UP options",
        Row::Button => "START rip a pack   UP options",
        Row::Typed | Row::Avoid => "START type names   SELECT clear   UP/DOWN options",
        Row::Era => "START choose eras   SELECT any era   UP/DOWN options",
        Row::Author => "START choose authors   SELECT anyone   UP/DOWN options",
        Row::Lo | Row::Hi => "LEFT/RIGHT change   START type it   UP/DOWN options",
        Row::Full => "LEFT/RIGHT change   UP/DOWN options",
    }
}

// --- the options --------------------------------------------------------------

fn push_options(actors: &mut Vec<Actor>, state: &State, accent: [f32; 4]) {
    let rip = &state.rip;
    let in_body = state.zone == Zone::Rip;
    let value_x = lo::RIP_PANEL_X + 10.0;
    for (index, row) in Row::ALL.into_iter().enumerate() {
        // Lit means the cursor is in the options, on this row. On the tab
        // row nothing is.
        let lit = in_body && rip.row == index;
        let y = lo::rip_row_y(index);
        match row {
            Row::Button => push_button(actors, state, accent, y + 10.0, lit),
            Row::Reset => {
                let ink = if lit {
                    accent
                } else if rip.stage == Stage::Filling {
                    [0.32, 0.32, 0.34, 1.0]
                } else {
                    [0.55, 0.55, 0.58, 1.0]
                };
                actors.push(act!(text:
                    font("miso"): settext("RESET OPTIONS"):
                    align(1.0, 0.5): xy(lo::RIP_PANEL_X + lo::RIP_PANEL_W - 4.0, y - 6.0):
                    zoom(0.4): horizalign(right):
                    diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z)
                ));
            }
            _ => {
                let band = if lit {
                    [accent[0], accent[1], accent[2], 0.22]
                } else {
                    [1.0, 1.0, 1.0, 0.04]
                };
                actors.push(act!(quad:
                    align(0.0, 0.5): xy(lo::RIP_PANEL_X, y):
                    zoomto(lo::RIP_PANEL_W, lo::RIP_ROW_H - 4.0):
                    diffuse(band[0], band[1], band[2], band[3]): z(Z)
                ));
                actors.push(act!(text:
                    font("miso"): settext(row.label()):
                    align(0.0, 0.5): xy(value_x, y - 9.0): zoom(0.38): horizalign(left):
                    diffuse(0.6, 0.6, 0.6, 1.0): z(Z)
                ));
                let ink = if rip::row_muted(rip, row) { 0.55 } else { 1.0 };
                actors.push(act!(text:
                    font("miso"): settext(content(rip::row_value(rip, row))):
                    align(0.0, 0.5): xy(value_x, y + 7.0): zoom(0.55):
                    maxwidth(lo::RIP_PANEL_W - 44.0): horizalign(left):
                    diffuse(ink, ink, ink, 1.0): z(Z)
                ));
                // The arrows that change it, on the row the cursor is on.
                if lit && row.cycles() {
                    actors.push(act!(text:
                        font("miso"): settext("&MENULEFT;&MENURIGHT;"):
                        align(1.0, 0.5): xy(lo::RIP_PANEL_X + lo::RIP_PANEL_W - 8.0, y):
                        zoom(0.42): horizalign(right):
                        diffuse(1.0, 1.0, 1.0, 1.0): z(Z)
                    ));
                }
            }
        }
    }

    // Under the button: how many so far, and what a booster is.
    actors.push(act!(text:
        font("miso"): settext(content(rip::info_text(rip))):
        align(0.0, 0.0):
        xy(lo::RIP_PANEL_X + 4.0, lo::rip_row_y(Row::ALL.len() - 1) + 12.0):
        zoom(0.4): wrapwidthpixels((lo::RIP_PANEL_W - 8.0) / 0.4): horizalign(left):
        diffuse(0.6, 0.6, 0.6, 1.0): z(Z)
    ));
}

/// RIP A PACK, or what the rip is busy doing -- and while a pack is on the
/// table, KEEP and RE-RIP side by side.
fn push_button(actors: &mut Vec<Actor>, state: &State, accent: [f32; 4], y: f32, lit: bool) {
    const H: f32 = 34.0;
    let rip = &state.rip;
    let x = lo::RIP_PANEL_X;

    if rip.stage != Stage::Choosing {
        let busy = rip::busy(rip);
        let base = if busy {
            [0.18, 0.18, 0.18, 1.0]
        } else if lit {
            [accent[0], accent[1], accent[2], 1.0]
        } else {
            [accent[0], accent[1], accent[2], 0.32]
        };
        actors.push(act!(quad:
            align(0.0, 0.5): xy(x, y): zoomto(lo::RIP_PANEL_W, H):
            diffuse(base[0], base[1], base[2], base[3]): z(Z)
        ));
        // While a kept pack comes in, the button fills with it.
        if let Some(fraction) = rip::fill_fraction(rip) {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(x, y): zoomto((lo::RIP_PANEL_W * fraction).max(1.0), H):
                diffuse(accent[0], accent[1], accent[2], 0.7): z(Z)
            ));
        }
        // The arrow keeps its own colours whatever the button's.
        let logo_alpha = if busy { 0.45 } else { 1.0 };
        actors.push(act!(sprite(LOGO):
            align(0.5, 0.5): xy(x + 22.0, y): setsize(24.0, 24.0):
            diffuse(1.0, 1.0, 1.0, logo_alpha): z(Z)
        ));
        let ink = if lit && !busy { INK } else { 1.0 };
        actors.push(act!(text:
            font("wendy"): settext(content(rip::button_label(rip))):
            align(0.0, 0.5): xy(x + 40.0, y): zoom(0.42): horizalign(left):
            diffuse(ink, ink, ink, 1.0): z(Z)
        ));
        return;
    }

    let half = (lo::RIP_PANEL_W - 6.0) / 2.0;
    for (n, choice, label) in [(0.0, Choice::Keep, "KEEP"), (1.0, Choice::ReRip, "RE-RIP")] {
        let x0 = x + n * (half + 6.0);
        let on = lit && rip.choice == choice;
        actors.push(act!(quad:
            align(0.0, 0.5): xy(x0, y): zoomto(half, H):
            diffuse(accent[0], accent[1], accent[2], if on { 1.0 } else { 0.32 }): z(Z)
        ));
        let ink = if on { INK } else { 1.0 };
        match choice {
            // The white glyph darkens on a lit button, as the label does.
            Choice::Keep => actors.push(act!(sprite(DOWNLOAD):
                align(0.5, 0.5): xy(x0 + 22.0, y): setsize(22.0, 22.0):
                diffuse(ink, ink, ink, 1.0): z(Z)
            )),
            Choice::ReRip => actors.push(act!(sprite(LOGO):
                align(0.5, 0.5): xy(x0 + 22.0, y): setsize(24.0, 24.0):
                diffuse(1.0, 1.0, 1.0, 1.0): z(Z)
            )),
        }
        actors.push(act!(text:
            font("wendy"): settext(label):
            align(0.0, 0.5): xy(x0 + 40.0, y): zoom(0.42): maxwidth(half - 46.0):
            horizalign(left):
            diffuse(ink, ink, ink, 1.0): z(Z)
        ));
    }
}

// --- the stage ----------------------------------------------------------------

fn push_stage(
    actors: &mut Vec<Actor>,
    state: &State,
    accent: [f32; 4],
    asset_manager: &AssetManager,
) {
    let rip = &state.rip;
    actors.push(act!(text:
        font("wendy"): settext(content(rip::heading(rip))):
        align(0.5, 0.5): xy(lo::RIP_STAGE_X, 54.0): zoom(0.36): maxwidth(500.0):
        horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z)
    ));
    let (under, failed) = rip::under_line(rip);
    let ink = if failed {
        [1.0, 0.5, 0.5, 1.0]
    } else {
        [0.8, 0.8, 0.8, 1.0]
    };
    actors.push(act!(text:
        font("miso"): settext(content(under)):
        align(0.5, 0.5): xy(lo::RIP_STAGE_X, 434.0): zoom(0.42): maxwidth(510.0):
        horizalign(center):
        diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z)
    ));

    let pack = rip::pack_motion(rip);
    // The flash when it tears: a round burst over the pack, not a sheet over
    // the whole stage.
    glow(
        actors,
        lo::RIP_STAGE_X,
        lo::RIP_STAGE_Y,
        440.0,
        400.0,
        [1.0, 1.0, 1.0],
        pack.flash,
    );
    if pack.shown {
        push_pack(actors, rip, accent, &pack);
    }
    if let Some(elapsed) = rip::opened_elapsed(rip) {
        push_cards(actors, state, accent, elapsed as f32, asset_manager);
    }
}

/// The booster pack: its halo, its body and the strip that tears off it.
fn push_pack(actors: &mut Vec<Actor>, rip: &rip::Rip, accent: [f32; 4], motion: &PackMotion) {
    const PW: f32 = lo::RIP_PACK_W;
    const PH: f32 = lo::RIP_PACK_H;
    const EDGE: [f32; 4] = [0.9, 0.9, 0.9, 1.0];
    let pack = Frame::at(motion.x, motion.y, motion.rot);
    let accent_rgb = scaled(accent, 1.0);

    glow(
        actors,
        pack.x,
        pack.y,
        PW + 150.0,
        PH + 150.0,
        accent_rgb,
        motion.glow,
    );

    let body = pack.child(0.0, motion.body_y, 0.0, motion.body_zoom, motion.body_alpha);
    quad(actors, &body, 0.0, 0.0, PW + 4.0, PH + 4.0, EDGE);
    gradient(
        actors,
        &body,
        0.0,
        0.0,
        PW,
        PH,
        accent_rgb,
        scaled(accent, 0.35),
        1.0,
    );
    // The crimp along the bottom, and its teeth.
    quad(
        actors,
        &body,
        0.0,
        PH / 2.0 - 8.0,
        PW,
        16.0,
        [0.0, 0.0, 0.0, 0.35],
    );
    push_teeth(actors, &body, PH / 2.0);
    line(
        actors,
        &body,
        "wendy",
        "CONTENT",
        0.0,
        -PH / 2.0 + 44.0,
        0.42,
        NO_MAX,
        [1.0; 4],
    );
    line(
        actors,
        &body,
        "wendy",
        "BOOSTER",
        0.0,
        -PH / 2.0 + 62.0,
        0.42,
        NO_MAX,
        [1.0; 4],
    );
    picture(actors, &body, LOGO, 0.0, 10.0, 78.0, 78.0, [1.0; 4]);
    // What this one will be made of.
    line(
        actors,
        &body,
        "miso",
        rip::recipe(rip),
        0.0,
        PH / 2.0 - 40.0,
        0.36,
        PW - 14.0,
        [1.0, 1.0, 1.0, 0.9],
    );
    line(
        actors,
        &body,
        "miso",
        "15 SONGS",
        0.0,
        PH / 2.0 - 24.0,
        0.34,
        NO_MAX,
        [1.0, 1.0, 1.0, 0.8],
    );
    if let Some((x, alpha)) = motion.sheen {
        soft_bar(actors, &body, x, 0.0, 26.0, PH - 6.0, [1.0; 3], alpha);
    }

    let strip = pack.child(
        0.0,
        motion.strip_y,
        motion.strip_rot,
        1.0,
        motion.strip_alpha,
    );
    quad(actors, &strip, 0.0, 0.0, PW + 4.0, 22.0, EDGE);
    quad(
        actors,
        &strip,
        0.0,
        0.0,
        PW,
        18.0,
        rgba(scaled(accent, 0.6), 1.0),
    );
    push_teeth(actors, &strip, -11.0);
    line(
        actors,
        &strip,
        "miso",
        "- - - TEAR HERE - - -",
        0.0,
        0.0,
        0.3,
        NO_MAX,
        [1.0, 1.0, 1.0, 0.75],
    );
}

/// Thirteen teeth along a row: small squares turned on their corners.
fn push_teeth(actors: &mut Vec<Actor>, f: &Frame, y: f32) {
    let pitch = (lo::RIP_PACK_W - 12.0) / 12.0;
    for tooth in 0..13 {
        let x = -lo::RIP_PACK_W / 2.0 + 6.0 + tooth as f32 * pitch;
        let (sx, sy) = f.point(x, y);
        actors.push(act!(quad:
            align(0.5, 0.5): xy(sx, sy): zoomto(7.0 * f.sx, 7.0 * f.sy):
            rotationz(-(f.rot + 45.0)):
            diffuse(0.9, 0.9, 0.9, f.alpha): z(Z)
        ));
    }
}

// --- the cards ----------------------------------------------------------------

/// The fifteen cards: every gold card's halo first, behind all of them, then
/// each card back to front in the order dealt.
fn push_cards(
    actors: &mut Vec<Actor>,
    state: &State,
    accent: [f32; 4],
    elapsed: f32,
    asset_manager: &AssetManager,
) {
    let rip = &state.rip;
    let songs = &rip.snapshot.songs;
    let count = rip.cards.len().min(songs.len()).min(BOOSTER_SONGS);
    let gold = rip::table_gold(rip);
    let mut motions = [None::<CardMotion>; BOOSTER_SONGS];
    for (card, motion) in motions.iter_mut().enumerate().take(count) {
        let since = rip
            .landed
            .get(card)
            .map_or(f32::MAX, |landed| (rip.now - landed) as f32);
        *motion = Some(rip::card_motion(card, elapsed, rip::bump(since)));
    }

    // A glow inside its own card would light the cards dealt before it and
    // hide under the ones dealt after, so the halos go down first.
    for (card, motion) in motions.iter().enumerate().take(count) {
        if let Some(motion) = motion
            && motion.shown
            && motion.face_up
            && gold[card]
        {
            glow(
                actors,
                motion.x,
                motion.y,
                lo::RIP_CARD_W + 120.0,
                lo::RIP_CARD_H + 120.0,
                GOLD,
                rip::halo(rip.now, card),
            );
        }
    }

    let fit = rip::fitted_titles(rip, &|text| text_width(asset_manager, "miso", text));
    for (card, motion) in motions.iter().enumerate().take(count) {
        if let Some(motion) = motion
            && motion.shown
        {
            push_card(actors, state, accent, card, motion, gold[card], &fit, songs);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_card(
    actors: &mut Vec<Actor>,
    state: &State,
    accent: [f32; 4],
    card: usize,
    motion: &CardMotion,
    gold: bool,
    fit: &TitleFit,
    songs: &Arc<[RipSong]>,
) {
    const CW: f32 = lo::RIP_CARD_W;
    const CH: f32 = lo::RIP_CARD_H;
    let rip = &state.rip;
    let f = Frame {
        x: motion.x,
        y: motion.y,
        rot: motion.rot,
        sx: motion.sx,
        sy: motion.sy,
        alpha: 1.0,
    };

    // The back: what a card is until it turns, DeadSync's arrow on it as a
    // trading card has its game's logo.
    if !motion.face_up {
        quad(actors, &f, 0.0, 0.0, CW + 4.0, CH + 4.0, accent);
        quad(actors, &f, 0.0, 0.0, CW, CH, [0.07, 0.07, 0.09, 1.0]);
        quad(
            actors,
            &f,
            0.0,
            0.0,
            CW - 10.0,
            CH - 10.0,
            [accent[0], accent[1], accent[2], 0.10],
        );
        picture(actors, &f, LOGO, 0.0, 0.0, 60.0, 60.0, [1.0, 1.0, 1.0, 0.9]);
        return;
    }

    let Some(text) = rip.cards.get(card) else {
        return;
    };
    let song = rip::song_state(rip, card);
    let failed = song == Some(SongState::Failed);
    let now = rip.now;

    // The face: the song. Every card is drawn the same way, and only the
    // pack's hardest chart differently, in gold.
    let border = if failed {
        [0.3, 0.3, 0.3, 1.0]
    } else if gold {
        rgba(GOLD, 1.0)
    } else {
        [accent[0], accent[1], accent[2], 0.55]
    };
    quad(actors, &f, 0.0, 0.0, CW + 4.0, CH + 4.0, border);
    gradient(
        actors,
        &f,
        0.0,
        0.0,
        CW,
        CH,
        [0.11, 0.11, 0.13],
        [0.04, 0.04, 0.05],
        1.0,
    );

    // Under the picture, so a song without one still has colour there; it
    // breathes while the picture is on its way.
    let loaded = text
        .art
        .as_ref()
        .filter(|(_, texture)| deadlib_assets::texture_dims(texture).is_some());
    let coming = text
        .art
        .as_ref()
        .is_some_and(|(key, _)| loaded.is_none() && !banner_is_lost(state, *key));
    let stand_in = if coming {
        rip::stand_in_pulse(now, card)
    } else {
        1.0
    };
    gradient(
        actors,
        &f,
        0.0,
        ART_Y,
        CW - 4.0,
        ART_H,
        scaled(accent, 0.45),
        [0.09, 0.09, 0.105],
        stand_in,
    );
    // The picture fills its window, whatever overhangs cropped evenly from
    // both sides, so every card's is the same shape.
    if let Some((_, texture)) = loaded {
        let (x, y) = f.point(0.0, ART_Y);
        let mut art = act!(sprite(Arc::clone(texture)):
            align(0.5, 0.5): xy(x, y): setsize((CW - 4.0) * f.sx, ART_H * f.sy):
            rotationz(-f.rot): z(Z)
        );
        if let Some(uv) = deadlib_assets::present_dsl::cover_uv(texture, CW - 4.0, ART_H)
            && let Actor::Sprite { uv_rect, .. } = &mut art
        {
            *uv_rect = Some(uv);
        }
        actors.push(art);
    }
    // The picture fades into the card rather than stopping at a line.
    let (fade_x, fade_y) = f.point(0.0, ART_Y + ART_H / 2.0 - 7.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(fade_x, fade_y): zoomto((CW - 4.0) * f.sx, 14.0 * f.sy):
        rotationz(-f.rot): fadetop(1.0):
        diffuse(0.09, 0.09, 0.105, 1.0): z(Z)
    ));

    // The badge: a ring in the card's colour round the Expert meter.
    let ring = if failed {
        [0.3, 0.3, 0.3, 1.0]
    } else if gold {
        rgba(GOLD, 1.0)
    } else {
        accent
    };
    picture(actors, &f, DISC, 0.0, BADGE_Y, 31.0, 31.0, ring);
    picture(
        actors,
        &f,
        DISC,
        0.0,
        BADGE_Y,
        25.0,
        25.0,
        [0.06, 0.06, 0.07, 1.0],
    );
    let meter_ink = if failed {
        [0.5, 0.5, 0.5, 1.0]
    } else if gold {
        rgba(GOLD, 1.0)
    } else {
        [1.0; 4]
    };
    let meter = if failed {
        TextContent::Static("--")
    } else {
        TextContent::Shared(Arc::clone(&text.meter_text))
    };
    line(
        actors,
        &f,
        "wendy",
        meter,
        0.0,
        BADGE_Y + 1.0,
        0.42,
        21.0,
        meter_ink,
    );

    push_card_info(actors, &f, accent, text, card, failed, gold, fit, songs);

    // The pack it came from, along the foot.
    line(
        actors,
        &f,
        "miso",
        &text.pack,
        0.0,
        CH / 2.0 - 8.0,
        0.33,
        CARD_TEXT_W,
        [0.6, 0.6, 0.63, 1.0],
    );

    if gold {
        // What the gold means, on the gold card itself.
        quad(actors, &f, 21.0, -47.5, 50.0, 13.0, rgba(GOLD, 1.0));
        line(
            actors,
            &f,
            "miso",
            "HARDEST",
            21.0,
            -47.5,
            0.38,
            46.0,
            [0.08, 0.06, 0.02, 1.0],
        );
        if let Some((x, alpha)) = rip::shine(now, card) {
            soft_bar(actors, &f, x, 0.0, 18.0, CH - 4.0, [1.0, 0.92, 0.7], alpha);
        }
    }

    // While a kept pack comes in, the cards whose songs are not in yet sit
    // under a shade, and the one on its way turns a wheel.
    let shade = rip::veil(now, rip.stage, song);
    if shade > 0.0 {
        quad(
            actors,
            &f,
            0.0,
            0.0,
            CW + 4.0,
            CH + 4.0,
            [0.0, 0.0, 0.0, shade],
        );
    }
    if rip.stage == Stage::Filling && song == Some(SongState::Downloading) {
        let (x, y) = f.point(0.0, INFO_Y - 4.0);
        actors.push(spinner::accent_actor(x, y, 28.0 * f.sy, Z, accent));
    }
}

/// The year, the title and who charted it, as one block centred on its
/// place: the title takes two lines at most, the year sits over it when the
/// song has one, and the credit under however many lines it took.
#[allow(clippy::too_many_arguments)]
fn push_card_info(
    actors: &mut Vec<Actor>,
    f: &Frame,
    accent: [f32; 4],
    text: &rip::CardText,
    card: usize,
    failed: bool,
    gold: bool,
    fit: &TitleFit,
    songs: &Arc<[RipSong]>,
) {
    let year_h = if text.year.is_some() {
        MISO_LINE * YEAR_ZOOM
    } else {
        0.0
    };
    let year_gap = if year_h > 0.0 { 1.0 } else { 0.0 };
    let fitted = fit.get(songs, card);
    let lines = fitted.map_or(1, |(_, lines)| lines);
    let title_h = lines as f32 * MISO_LINE * rip::TITLE_ZOOM;
    let credit: Option<TextContent> = if failed {
        Some(TextContent::Static("could not be downloaded"))
    } else {
        text.credit
            .as_ref()
            .map(|credit| TextContent::Shared(Arc::clone(credit)))
    };
    let credit_h = if credit.is_some() {
        MISO_LINE * CREDIT_ZOOM
    } else {
        0.0
    };
    let gap = if credit_h > 0.0 { 3.0 } else { 0.0 };
    let total = year_h + year_gap + title_h + gap + credit_h;

    if let Some(year) = text.year.as_ref() {
        let ink = if gold { rgba(GOLD, 1.0) } else { accent };
        line(
            actors,
            f,
            "miso",
            year,
            0.0,
            INFO_Y - total / 2.0 + year_h / 2.0,
            YEAR_ZOOM,
            NO_MAX,
            ink,
        );
    }

    let title_y = INFO_Y - total / 2.0 + year_h + year_gap + title_h / 2.0;
    match fitted {
        Some((title, _)) => line(
            actors,
            f,
            "miso",
            title,
            0.0,
            title_y,
            rip::TITLE_ZOOM,
            CARD_TEXT_W,
            [1.0; 4],
        ),
        // Not fitted yet -- the font is not there to measure with -- so the
        // title wraps itself meanwhile.
        None => {
            let (x, y) = f.point(0.0, title_y);
            actors.push(act!(text:
                font("miso"): settext(Arc::clone(&text.title)):
                align(0.5, 0.5): xy(x, y):
                zoomx(rip::TITLE_ZOOM * f.sx): zoomy(rip::TITLE_ZOOM * f.sy):
                wrapwidthpixels(rip::TITLE_WRAP): horizalign(center):
                diffuse(1.0, 1.0, 1.0, f.alpha): z(Z)
            ));
        }
    }

    if let Some(credit) = credit {
        let ink = if failed {
            [1.0, 0.55, 0.55, 1.0]
        } else {
            [0.7, 0.7, 0.72, 1.0]
        };
        line(
            actors,
            f,
            "miso",
            credit,
            0.0,
            INFO_Y + total / 2.0 - credit_h / 2.0,
            CREDIT_ZOOM,
            CARD_TEXT_W,
            ink,
        );
    }
}

// --- the picker ---------------------------------------------------------------

/// The list open over the tab -- the eras, or every charter -- a screenful
/// at a time. The box is as tall as the list it holds, up to a screenful,
/// and stays centred.
fn push_picker(actors: &mut Vec<Actor>, state: &State, accent: [f32; 4], w: f32, h: f32) {
    const ROW: f32 = lo::RIP_PICK_ROW;
    const BOX_W: f32 = lo::RIP_PICK_W;
    const TALLEST: f32 = 70.0 + lo::RIP_PICK_ROWS as f32 * ROW;
    let rip = &state.rip;
    let Some(pick) = rip.picking else {
        return;
    };
    let count = rip::pick_count(rip);
    let box_h = 70.0 + count.clamp(1, lo::RIP_PICK_ROWS) as f32 * ROW;
    // Laid out from the top edge, so a shorter box moves down by half of
    // what it gave up.
    let cx = w * 0.5;
    let cy = h * 0.5 + (TALLEST - box_h) / 2.0;
    let top = cy - TALLEST / 2.0;

    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(w * 2.0, h * 3.0):
        diffuse(0.0, 0.0, 0.0, 0.7): z(Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.0): xy(cx, top - 2.0): zoomto(BOX_W + 4.0, box_h + 4.0):
        diffuse(1.0, 1.0, 1.0, 0.9): z(Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.0): xy(cx, top): zoomto(BOX_W, box_h):
        diffuse(0.05, 0.05, 0.06, 1.0): z(Z)
    ));
    let title = match pick {
        Pick::Eras => "CHOOSE ERAS",
        Pick::Authors => "CHOOSE AUTHORS",
    };
    actors.push(act!(text:
        font("wendy"): settext(title):
        align(0.0, 0.5): xy(cx - BOX_W / 2.0 + 14.0, top + 20.0): zoom(0.4): horizalign(left):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z)
    ));
    let chosen = rip::pick_chosen(rip);
    let tally = match (chosen, pick) {
        (0, Pick::Eras) => TextContent::Static("any era"),
        (0, Pick::Authors) => TextContent::Static("anyone"),
        (n, _) => TextContent::Owned(format!("{n} chosen")),
    };
    actors.push(act!(text:
        font("miso"): settext(tally):
        align(1.0, 0.5): xy(cx + BOX_W / 2.0 - 14.0, top + 20.0): zoom(0.45): horizalign(right):
        diffuse(0.75, 0.75, 0.78, 1.0): z(Z)
    ));

    let first_y = top + 52.0;
    for slot in 0..lo::RIP_PICK_ROWS {
        let at = rip.pick_top + slot;
        if at >= count {
            break;
        }
        let y = first_y + slot as f32 * ROW;
        if at == rip.pick_at {
            actors.push(act!(quad:
                align(0.5, 0.5): xy(cx, y): zoomto(BOX_W - 28.0, ROW - 2.0):
                diffuse(accent[0], accent[1], accent[2], 0.3): z(Z)
            ));
        }
        let tick_x = cx - BOX_W / 2.0 + 30.0;
        actors.push(act!(quad:
            align(0.5, 0.5): xy(tick_x, y): zoomto(12.0, 12.0):
            diffuse(1.0, 1.0, 1.0, 0.6): z(Z)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5): xy(tick_x, y): zoomto(10.0, 10.0):
            diffuse(0.05, 0.05, 0.06, 1.0): z(Z)
        ));
        if rip::pick_ticked(rip, at) {
            actors.push(act!(quad:
                align(0.5, 0.5): xy(tick_x, y): zoomto(7.0, 7.0):
                diffuse(accent[0], accent[1], accent[2], 1.0): z(Z)
            ));
        }
        let (label, note) = match pick {
            Pick::Eras => (
                TextContent::Static(Era::ALL.get(at).map_or("", |era| era.label())),
                None,
            ),
            Pick::Authors => match rip.author_rows.get(at) {
                Some(row) => (
                    TextContent::Shared(Arc::clone(&row.name)),
                    row.note.as_ref().map(Arc::clone),
                ),
                None => (TextContent::Static(""), None),
            },
        };
        actors.push(act!(text:
            font("miso"): settext(label):
            align(0.0, 0.5): xy(cx - BOX_W / 2.0 + 44.0, y): zoom(0.5): maxwidth(BOX_W - 150.0):
            horizalign(left):
            diffuse(1.0, 1.0, 1.0, 1.0): z(Z)
        ));
        // Beside a charter, how much of theirs is played now.
        if let Some(note) = note {
            actors.push(act!(text:
                font("miso"): settext(note):
                align(1.0, 0.5): xy(cx + BOX_W / 2.0 - 26.0, y): zoom(0.42): horizalign(right):
                diffuse(accent[0], accent[1], accent[2], 1.0): z(Z)
            ));
        }
    }

    // The scrollbar, only when the list runs past a screenful.
    if count > lo::RIP_PICK_ROWS {
        let length = lo::RIP_PICK_ROWS as f32 * ROW;
        let x = cx + BOX_W / 2.0 - 12.0;
        let y = first_y - ROW / 2.0;
        let thumb = (length * lo::RIP_PICK_ROWS as f32 / count as f32)
            .floor()
            .max(14.0);
        let span = length - thumb;
        let offset = rip.pick_top.min(count - lo::RIP_PICK_ROWS) as f32;
        let position = (span * offset / (count - lo::RIP_PICK_ROWS) as f32 + 0.5)
            .floor()
            .clamp(0.0, span);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y): zoomto(lo::SCROLL_W, length):
            diffuse(1.0, 1.0, 1.0, 0.10): z(Z)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y + position): zoomto(lo::SCROLL_W, thumb):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z)
        ));
    }

    actors.push(act!(text:
        font("miso"): settext("&START; choose    &SELECT; clear    &BACK; done"):
        align(0.5, 0.5): xy(cx, top + box_h - 16.0): zoom(0.45): horizalign(center):
        diffuse(0.85, 0.85, 0.88, 1.0): z(Z)
    ));
}

// --- the prompt and the toast -------------------------------------------------

/// The text prompt, as DeadSync's other text entry draws one: the question
/// in a panel, the answer in a field under it with a caret, and the keys
/// that end it.
pub(super) fn push_prompt(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    w: f32,
    h: f32,
) {
    const BORDER: f32 = 2.0;
    const ANSWER_H: f32 = 40.0;
    let Some(prompt) = state.rip.prompt.as_ref() else {
        return;
    };
    let accent = accent(state);
    let box_w = w * 0.75;
    let top_h = h * 0.25;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top_cy = cy - 40.0;
    let answer_cy = cy + 16.0;

    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top_cy): zoomto(box_w, top_h):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_PANEL)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, top_cy):
        zoomto(box_w - 2.0 * BORDER, top_h - 2.0 * BORDER):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MODAL_PANEL + 1)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, answer_cy): zoomto(box_w, ANSWER_H):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_PANEL)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, answer_cy):
        zoomto(box_w - 2.0 * BORDER, ANSWER_H - 2.0 * BORDER):
        diffuse(0.0, 0.0, 0.0, 1.0): z(Z_MODAL_PANEL + 1)
    ));
    actors.push(act!(text:
        font("miso"): settext(prompt.kind.question()):
        align(0.5, 0.5): xy(cx, cy - 64.0): zoom(1.0): maxwidth((box_w - 40.0).min(600.0)):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));
    let max_w = box_w - 40.0;
    actors.push(act!(text:
        font("miso"): settext(prompt.text.clone()):
        align(0.5, 0.5): xy(cx, answer_cy): zoom(1.0): maxwidth(max_w): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));
    // The caret where the answer ends, blinking -- measured in the font, as
    // the search box's is, because miso is proportional.
    if ((prompt.caret * 2.0) as u32).is_multiple_of(2) {
        let typed = if prompt.text.is_empty() {
            0.0
        } else {
            text_width(asset_manager, "miso", &prompt.text).min(max_w)
        };
        actors.push(act!(quad:
            align(0.0, 0.5): xy(cx + typed * 0.5 + 2.0, answer_cy): zoomto(2.0, 24.0):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MODAL_TEXT)
        ));
    }
    actors.push(act!(text:
        font("miso"): settext("type, then ENTER or START to keep it    ESCAPE or BACK to cancel"):
        align(0.5, 0.5): xy(cx, answer_cy + ANSWER_H * 0.5 + 18.0): zoom(0.5):
        horizalign(center):
        diffuse(0.75, 0.75, 0.75, 1.0): z(Z_MODAL_TEXT)
    ));
}

/// A refusal, in the original's toast: over the stage's foot, in the accent,
/// for two seconds and a fade.
pub(super) fn push_toast(actors: &mut Vec<Actor>, state: &State, w: f32) {
    let rip = &state.rip;
    let Some(toast) = rip.toast.as_ref() else {
        return;
    };
    let Some(alpha) = rip::toast_alpha(rip.now - toast.at) else {
        return;
    };
    let accent = accent(state);
    actors.push(act!(text:
        font("miso"): settext(toast.text.clone()):
        align(0.5, 0.5): xy(w * 0.5, lo::CONTENT_BOT - 8.0): zoom(0.6): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], alpha): z(Z_MODAL_TEXT)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::content_browser::state;
    use deadsync_online::rip_packs::{KeepPhase, KeepState, RipPhase, RipSnapshot};

    fn texts(actors: &[Actor]) -> Vec<String> {
        actors
            .iter()
            .filter_map(|actor| match actor {
                Actor::Text { content, .. } => Some(content.as_str().to_owned()),
                _ => None,
            })
            .collect()
    }

    fn rip_tab() -> State {
        let mut state = state::init();
        state.tab_index = state::TABS
            .iter()
            .position(|tab| *tab == state::Tab::Rip)
            .expect("the rip tab");
        state
    }

    /// A frame turns its children about its own centre, clockwise for a
    /// positive angle on a screen whose y runs down.
    #[test]
    fn a_frame_places_its_children_turned_and_scaled() {
        let frame = Frame::at(100.0, 100.0, 90.0);
        let (x, y) = frame.point(10.0, 0.0);
        assert!(
            (x - 100.0).abs() < 1e-4 && (y - 110.0).abs() < 1e-4,
            "right turns down"
        );
        let child = frame.child(0.0, -20.0, 0.0, 0.5, 0.5);
        assert!((child.x - 120.0).abs() < 1e-4 && (child.y - 100.0).abs() < 1e-4);
        assert_eq!((child.sx, child.alpha), (0.5, 0.5));
    }

    /// The unopened stage: the nine rows' words, the pack's words, and the
    /// lines over and under it.
    #[test]
    fn the_idle_stage_draws_the_originals_words() {
        let state = rip_tab();
        let mut actors = Vec::new();
        push_rip(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        let words = texts(&actors);
        for expected in [
            "ERA",
            "Any era  (START to choose)",
            "AUTHOR",
            "OR TYPE AUTHORS",
            "none  (START to type exact names)",
            "AVOID CHARTS BY",
            "nobody  (START to type exact names)",
            "EXPERT FROM",
            "8",
            "EXPERT TO",
            "14",
            "FULL DIFFICULTIES",
            "No",
            "RIP A PACK",
            "RESET OPTIONS",
            "RIP A BOOSTER PACK",
            "CONTENT",
            "BOOSTER",
            "ANY ERA  -  EXPERT 8-14",
            "15 SONGS",
            "- - - TEAR HERE - - -",
        ] {
            assert!(
                words.iter().any(|word| word == expected),
                "missing {expected:?}"
            );
        }
        let glyphs = |words: &[String]| {
            words
                .iter()
                .filter(|word| {
                    word.chars()
                        .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch))
                })
                .count()
        };
        assert_eq!(
            glyphs(&words),
            0,
            "no arrows with the cursor on the tab row"
        );

        // on EXPERT FROM, the arrows that change it
        let mut state = state;
        state.zone = Zone::Rip;
        state.rip.row = 4;
        let mut actors = Vec::new();
        push_rip(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        assert_eq!(glyphs(&texts(&actors)), 1);
    }

    /// The table: fifteen cards, face up once dealt, KEEP and RE-RIP in
    /// place of the button, and the gold card marked HARDEST.
    #[test]
    fn a_pack_on_the_table_deals_its_cards() {
        let mut state = rip_tab();
        state.zone = Zone::Rip;
        state.rip.row = 7;
        let songs: Vec<RipSong> = (0..BOOSTER_SONGS)
            .map(|index| RipSong {
                key: format!("1:{index}"),
                pack_id: 1,
                pack_name: format!("Pack {index}"),
                song_id: index as u64,
                title: format!("Title {index}"),
                artist: String::new(),
                credit: "Zaia".into(),
                meter: if index == 3 { 15 } else { 9 },
                year: None,
                art_url: None,
                folder: String::new(),
                bytes: 1,
            })
            .collect();
        let snapshot = Arc::new(RipSnapshot {
            phase: RipPhase::Ready,
            set: 1,
            songs: Arc::from(songs),
            revision: 1,
            ..RipSnapshot::default()
        });
        let authors = Arc::clone(&state.rip.authors);
        rip::sync(&mut state.rip, snapshot, authors);
        state::update(&mut state, 3.0);

        let mut actors = Vec::new();
        push_rip(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        let words = texts(&actors);
        assert!(words.iter().any(|word| word == "KEEP"));
        assert!(words.iter().any(|word| word == "RE-RIP"));
        assert!(!words.iter().any(|word| word == "RIP A PACK"));
        assert_eq!(
            words.iter().filter(|word| *word == "by Zaia").count(),
            BOOSTER_SONGS
        );
        assert_eq!(words.iter().filter(|word| *word == "HARDEST").count(), 1);
        assert!(words.iter().any(|word| word == "KEEP THIS PACK?"));
        assert!(
            !words.iter().any(|word| word == "CONTENT"),
            "the pack is gone"
        );
        assert_eq!(
            footer(&state),
            "LEFT/RIGHT keep or re-rip   START choose   UP options"
        );

        // and a failed song turns grey and says so, and is never gold
        let mut kept = (*state.rip.snapshot).clone();
        let mut states = vec![SongState::Done; BOOSTER_SONGS];
        states[3] = SongState::Failed;
        kept.keep = Some(Arc::new(KeepState {
            set: 1,
            folder: "Content Booster Pack 1 - 2026-10-17".into(),
            phase: KeepPhase::Done,
            songs: Arc::from(states),
            done_bytes: 15,
            total_bytes: 15,
            got: 14,
            failed: 1,
            group_dir: None,
            message: None,
        }));
        kept.revision = 2;
        let authors = Arc::clone(&state.rip.authors);
        rip::sync(&mut state.rip, Arc::new(kept), authors);
        let mut actors = Vec::new();
        push_rip(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        let words = texts(&actors);
        assert!(words.iter().any(|word| word == "could not be downloaded"));
        assert!(words.iter().any(|word| word == "--"));
        assert_eq!(words.iter().filter(|word| *word == "HARDEST").count(), 0);
    }

    /// The picker shows its title, its tally and its rows, and the footer
    /// says BACK closes it.
    #[test]
    fn the_picker_draws_over_the_tab() {
        let mut state = rip_tab();
        state.zone = Zone::Rip;
        state.rip.eras = vec![Era::Mid];
        assert_eq!(
            rip::press(&mut state, rip::Key::Start, true),
            rip::Heard::Start
        );
        let mut actors = Vec::new();
        push_rip(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        let words = texts(&actors);
        for expected in [
            "CHOOSE ERAS",
            "1 chosen",
            "Modern (2020-present)",
            "Mid-era (2015-2020)",
            "Vintage (before 2015)",
        ] {
            assert!(
                words.iter().any(|word| word == expected),
                "missing {expected:?} in {words:?}"
            );
        }
        // the box's own keys, their glyphs already in place
        assert!(
            words
                .iter()
                .any(|word| word.ends_with(" choose    \u{e013} clear    \u{e010} done"))
        );
        assert_eq!(footer(&state), "UP/DOWN move   START choose   SELECT clear");
    }

    /// The prompt asks its question and shows what has been typed.
    #[test]
    fn the_prompt_asks_its_question() {
        let mut state = rip_tab();
        state.zone = Zone::Rip;
        state.rip.row = 3;
        rip::press(&mut state, rip::Key::Start, true);
        rip::prompt_type(&mut state.rip, "Someone");
        let mut actors = Vec::new();
        push_prompt(&mut actors, &state, &AssetManager::new(), 854.0, 480.0);
        let words = texts(&actors);
        assert!(
            words
                .iter()
                .any(|word| word == "Chart authors to avoid, separated by commas")
        );
        assert!(words.iter().any(|word| word == "Someone"));
    }
}
