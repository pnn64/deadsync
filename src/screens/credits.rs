use crate::act;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_height, screen_width};
use crate::screens::components::heart_bg;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ credits ------------------------------ */
// Mirror _fallback ScreenCredits scroller pacing.
const LINE_HEIGHT: f32 = 30.0;
const ITEM_PADDING_START: f32 = 4.0;
const ITEM_PADDING_END: f32 = 15.0;
const SECONDS_PER_ITEM: f32 = 1.0;
const BASE_Y_FROM_BOTTOM: f32 = 64.0;
const CINEMATIC_ANIM_SECONDS: f32 = 1.8;
const CINEMATIC_BAR_MAX_H: f32 = 54.0;
const CINEMATIC_BAR_MAX_ALPHA: f32 = 1.0;
const CINEMATIC_TINT_RGBA: [f32; 4] = [0.06, 0.1, 0.13, 1.0];
const CINEMATIC_TINT_MAX_ALPHA: f32 = 0.2;

const SECTION_COLOR: [f32; 4] = [0.533, 0.867, 1.0, 1.0];
const SECTION_ACCENT: [f32; 4] = [0.267, 0.4, 0.533, 1.0];
const NAME_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const NAME_ACCENT: [f32; 4] = [0.267, 0.267, 0.267, 1.0];

#[derive(Clone, Copy)]
enum CreditLineKind {
    Section,
    Subsection,
    Name,
    Spacer,
}

#[derive(Clone, Copy)]
struct CreditLine {
    kind: CreditLineKind,
    text: &'static str,
}

const fn section(text: &'static str) -> CreditLine {
    CreditLine {
        kind: CreditLineKind::Section,
        text,
    }
}

const fn subsection(text: &'static str) -> CreditLine {
    CreditLine {
        kind: CreditLineKind::Subsection,
        text,
    }
}

const fn name(text: &'static str) -> CreditLine {
    CreditLine {
        kind: CreditLineKind::Name,
        text,
    }
}

const fn spacer() -> CreditLine {
    CreditLine {
        kind: CreditLineKind::Spacer,
        text: "",
    }
}

const CREDITS: &[CreditLine] = &[
    section("DeadSync Team"),
    name("Patrik Nilsson (PerfectTaste)"),
    name("Lead Developer & Project Maintainer"),
    spacer(),
    spacer(),
    section("DeadSync Contributors"),
    name("Mason Boeman (maboesanman)"),
    name("DolphinChips"),
    name("rehtlaw"),
    spacer(),
    spacer(),
    section("rssp Contributors"),
    name("Celeste Clark (celex3)"),
    spacer(),
    spacer(),
    section("Acknowledgements"),
    name("DeadSync stands on decades of work"),
    name("from the StepMania and ITG communities."),
    spacer(),
    subsection("StepMania"),
    name("To StepMania and its contributors:"),
    name("for creating the engine foundation,"),
    name("and for the many years of continued development."),
    spacer(),
    subsection("ITGmania"),
    name("To ITGmania and its developers:"),
    name("for shaping the modern ITG machine experience,"),
    name("and keeping the torch burning."),
    spacer(),
    subsection("Simply Love"),
    name("To Simply Love, its maintainers, and forks:"),
    name("for defining a huge part of how ITG is played today."),
    name("Special thanks to zmod (Zarzob and Zankoku)."),
    spacer(),
    subsection("Community Projects"),
    name("Stamina Nation"),
    name("ITC (International Timing Collective)"),
    name("Wafles for Arrow Cloud"),
    name("Rafal Florczak (florczakraf) for BoogieStats"),
    name("...and all contributors to the above projects."),
    spacer(),
    name("And to everyone who has created themes, noteskins,"),
    name("tools, charts, and simfiles over the years."),
    spacer(),
    spacer(),
    section("Special Thanks"),
    name("Cabinet operators and tournament organizers"),
    name("Theme and tool authors"),
    name("Pack creators and chart authors"),
    name("The rhythm game community"),
    spacer(),
    spacer(),
    section("Copyright"),
    name("DeadSync is released under the GPL-3.0 license."),
    name("Upstream projects are licensed under their respective terms."),
    name("All game content remains property of its respective owners."),
];

const TOTAL_SCROLL_ITEMS: f32 = CREDITS.len() as f32 + ITEM_PADDING_START + ITEM_PADDING_END;

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    enter_elapsed: f32,
    scroll_items: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        enter_elapsed: 0.0,
        scroll_items: 0.0,
    }
}

pub fn update(state: &mut State, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    state.enter_elapsed += dt;
    let scroll_speed = 1.0 / SECONDS_PER_ITEM;
    state.scroll_items = (state.scroll_items + dt * scroll_speed) % TOTAL_SCROLL_ITEMS;
}

#[inline(always)]
const fn line_zoom(kind: CreditLineKind) -> f32 {
    match kind {
        CreditLineKind::Section => 1.0,
        CreditLineKind::Subsection => 0.92,
        CreditLineKind::Name | CreditLineKind::Spacer => 0.875,
    }
}

#[inline(always)]
const fn line_colors(kind: CreditLineKind) -> ([f32; 4], [f32; 4]) {
    match kind {
        CreditLineKind::Section | CreditLineKind::Subsection => (SECTION_COLOR, SECTION_ACCENT),
        CreditLineKind::Name | CreditLineKind::Spacer => (NAME_COLOR, NAME_ACCENT),
    }
}

#[inline(always)]
fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back => ScreenAction::NavigateNoFade(Screen::Options),
        _ => ScreenAction::None,
    }
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(CREDITS.len() * 2 + 12);
    let screen_w = screen_width();
    let screen_h = screen_height();

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    let cinematic_t =
        ease_out_cubic((state.enter_elapsed / CINEMATIC_ANIM_SECONDS).clamp(0.0, 1.0));
    let tint_alpha = CINEMATIC_TINT_MAX_ALPHA * cinematic_t;
    if tint_alpha > 0.0 {
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_w, screen_h):
            diffuse(
                CINEMATIC_TINT_RGBA[0],
                CINEMATIC_TINT_RGBA[1],
                CINEMATIC_TINT_RGBA[2],
                CINEMATIC_TINT_RGBA[3] * tint_alpha
            ):
            z(5)
        ));
    }

    let scroll_px = state.scroll_items * LINE_HEIGHT;
    let total_scroll_px = TOTAL_SCROLL_ITEMS * LINE_HEIGHT;
    let base_y = screen_h - BASE_Y_FROM_BOTTOM;
    let max_width = screen_w * 0.8;

    for cycle in 0..2 {
        let cycle_offset = cycle as f32 * total_scroll_px;
        for (idx, line) in CREDITS.iter().enumerate() {
            if matches!(line.kind, CreditLineKind::Spacer) {
                continue;
            }

            let y =
                base_y + (idx as f32 + ITEM_PADDING_START) * LINE_HEIGHT - scroll_px + cycle_offset;
            if y < -LINE_HEIGHT || y > screen_h + LINE_HEIGHT {
                continue;
            }

            let (diffuse, accent) = line_colors(line.kind);
            actors.push(act!(text:
                font("miso"):
                settext(line.text):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                zoom(line_zoom(line.kind)):
                maxwidth(max_width):
                horizalign(center):
                diffuse(diffuse[0], diffuse[1], diffuse[2], diffuse[3]):
                strokecolor(accent[0], accent[1], accent[2], accent[3]):
                shadowcolor(accent[0], accent[1], accent[2], accent[3]):
                shadowlength(1.0):
                z(20)
            ));
        }
    }

    // Cinematic letterbox bars easing in from the edges.
    let matte_h = CINEMATIC_BAR_MAX_H * cinematic_t;
    if matte_h > 0.0 {
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_w, matte_h):
            diffuse(0.0, 0.0, 0.0, CINEMATIC_BAR_MAX_ALPHA):
            z(30)
        ));
        actors.push(act!(quad:
            align(0.0, 1.0):
            xy(0.0, screen_h):
            zoomto(screen_w, matte_h):
            diffuse(0.0, 0.0, 0.0, CINEMATIC_BAR_MAX_ALPHA):
            z(30)
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext("Press &START; and &BACK; to return"):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h - CINEMATIC_BAR_MAX_H * 0.5):
        zoom(0.7):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.8):
        z(40)
    ));

    actors
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}
