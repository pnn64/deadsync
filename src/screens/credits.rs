use crate::act;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_height, screen_width};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ------------------------------ credits ------------------------------ */
// Mirror _fallback ScreenCredits scroller pacing.
const LINE_HEIGHT: f32 = 30.0;
const ITEM_PADDING_START: f32 = 4.0;
const ITEM_PADDING_END: f32 = 15.0;
const SECONDS_PER_ITEM: f32 = 0.5;
const BASE_Y_FROM_BOTTOM: f32 = 64.0;

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
    section("deadsync Team"),
    name("Clean-room ITGmania + Simply Love parity project"),
    name("Built in Rust from scratch"),
    spacer(),
    spacer(),
    section("ITGmania Team"),
    name("Martin Natano (natano)"),
    name("teejusb"),
    spacer(),
    spacer(),
    section("spinal shark collective"),
    name("AJ Kelly (freem)"),
    name("Jonathan Payne (Midiman)"),
    name("Colby Klein (shakesoda)"),
    spacer(),
    spacer(),
    section("sm-ssc Team"),
    name("Jason Felds (wolfman2000)"),
    name("Thai Pangsakulyanont (theDtTvB)"),
    name("Alberto Ramos (Daisuke Master)"),
    name("Jack Walstrom (FSX)"),
    spacer(),
    spacer(),
    section("StepMania Team"),
    name("Chris Danford"),
    name("Glenn Maynard"),
    name("Steve Checkoway"),
    name("and many other contributors"),
    spacer(),
    spacer(),
    section("Other Contributors"),
    subsection("ITGmania"),
    name("Club Fantastic"),
    name("DinsFire64"),
    name("EvocaitArt"),
    name("jenx"),
    name("LightningXCE"),
    subsection("StepMania"),
    name("kyzentun"),
    name("nixtrix"),
    name("Sakurana-Kobato"),
    name("Wallacoloo"),
    name("and more"),
    spacer(),
    spacer(),
    section("Special Thanks"),
    name("Pack creators"),
    name("Chart authors"),
    name("Theme maintainers"),
    name("Cabinet operators"),
    name("The rhythm game community"),
    spacer(),
    spacer(),
    section("Copyright"),
    name("ITGmania is released under the MIT license."),
    name("All game content remains property of its owners."),
];

const TOTAL_SCROLL_ITEMS: f32 = CREDITS.len() as f32 + ITEM_PADDING_START + ITEM_PADDING_END;

pub struct State {
    scroll_items: f32,
}

pub fn init() -> State {
    State { scroll_items: 0.0 }
}

pub fn update(state: &mut State, dt: f32) {
    if dt <= 0.0 {
        return;
    }
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

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back => ScreenAction::Navigate(Screen::Options),
        _ => ScreenAction::None,
    }
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(CREDITS.len() * 2 + 12);
    let screen_w = screen_width();
    let screen_h = screen_height();

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_w, screen_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(0)
    ));

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

            let y = base_y + (idx as f32 + ITEM_PADDING_START) * LINE_HEIGHT - scroll_px + cycle_offset;
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

    // Slight matte masks mimic the fallback screen framing.
    let matte_h = 42.0;
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_w, matte_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(30)
    ));
    actors.push(act!(quad:
        align(0.0, 1.0):
        xy(0.0, screen_h):
        zoomto(screen_w, matte_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(30)
    ));

    actors.push(act!(text:
        font("miso"):
        settext("Press Start or Back to return"):
        align(0.5, 1.0):
        xy(screen_center_x(), screen_h - 12.0):
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
