use std::sync::Arc;

use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::font::FontMap;
use deadsync_core::input::MAX_PLAYERS;

use crate::option_explanation::{Explanation, ExplanationPlacement, fit_zoom};
use crate::presentation::Layout;

use super::{PLAYER_HUES, Page, rgba};

const HELP_Y: f32 = 381.0;
const HELP_WIDTH: f32 = 250.0;
const HELP_HEIGHT: f32 = 86.0;
const BASE_ZOOM: f32 = 0.82;
const WRAP_WIDTH: i32 = (HELP_WIDTH / BASE_ZOOM) as i32;
const ANCHORS: [(f32, TextAlign); MAX_PLAYERS] =
    [(68.0, TextAlign::Left), (568.0, TextAlign::Right)];

#[derive(Default)]
struct SeatHelp {
    lines: Vec<Arc<str>>,
    paragraph: Option<Explanation>,
    zoom: f32,
    started: f64,
}

impl SeatHelp {
    fn replace(&mut self, lines: impl Iterator<Item = Arc<str>>) {
        self.lines.clear();
        self.lines.extend(lines);
        self.paragraph = match self.lines.as_slice() {
            [] => None,
            [one] => (!one.is_empty()).then(|| Explanation::new(one)),
            lines if lines.iter().all(|line| line.is_empty()) => None,
            lines => {
                let length = lines.iter().map(|line| line.len()).sum::<usize>() + lines.len() - 1;
                let mut text = String::with_capacity(length);
                for (index, line) in lines.iter().enumerate() {
                    if index > 0 {
                        text.push('\n');
                    }
                    text.push_str(line);
                }
                Some(Explanation::new(&Arc::from(text)))
            }
        };
    }
}

#[derive(Default)]
pub(super) struct PlayerHelp {
    seats: [SeatHelp; MAX_PLAYERS],
    font_chain: Option<u64>,
}

impl PlayerHelp {
    pub(super) fn update(&mut self, page: &Page<'_>, fonts: &FontMap, new_pane: bool, now: f64) {
        let font_chain = fonts.get("miso").map(|font| font.chain_key);
        let font_changed = self.font_chain != font_chain;
        self.font_chain = font_chain;
        for (player, seat) in self.seats.iter_mut().enumerate() {
            let lines = (0..).map_while(|index| {
                if page.active[player] {
                    (page.help)(player, index)
                } else {
                    None
                }
            });
            let changed = !seat.lines.iter().eq(lines.clone());
            if changed {
                seat.replace(lines.cloned());
            }
            if changed || new_pane {
                seat.started = now;
            }
            if (changed || font_changed)
                && let Some(text) = &seat.paragraph
            {
                seat.zoom = fit_zoom(
                    text.measure(fonts, WRAP_WIDTH),
                    [HELP_WIDTH, HELP_HEIGHT],
                    BASE_ZOOM,
                );
            }
        }
    }

    pub(super) fn push(&self, actors: &mut Vec<Actor>, layout: Layout, alpha: f32, now: f64) {
        actors.push(layout.panel(
            36.0,
            370.0,
            [568.0, 1.0],
            rgba([0.32, 0.40, 0.48], alpha),
            -17,
        ));
        for (player, seat) in self.seats.iter().enumerate() {
            if let Some(text) = &seat.paragraph {
                let (x, align) = ANCHORS[player];
                actors.push(text.actor(
                    layout,
                    ExplanationPlacement {
                        position: [x, HELP_Y],
                        align,
                        zoom: seat.zoom,
                        wrap_width: WRAP_WIDTH,
                    },
                    rgba(PLAYER_HUES[player], alpha),
                    now - seat.started,
                ));
            }
        }
    }
}
