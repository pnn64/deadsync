use crate::presentation::{ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, quad};
use crate::{Effect, OpenItgRuntimeRequest, OpenItgScreen, OpenItgTheme};
use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_config::coin::CoinMode;
use deadsync_config::theme::GameFlag;
use deadsync_input::{InputEvent, MenuLrChord, VirtualAction};
use deadsync_theme::main_menu::{
    MainMenuFrame, MainMenuItem, MainMenuPhase, MainMenuRuntimeView, ThemeMainMenu, main_menu_items,
};
use deadsync_theme::{ThemeInputResult, TransitionState, UiCue};

type RuntimeView = MainMenuRuntimeView<GameFlag, CoinMode>;
const FIRST_ROW_Y: f32 = 112.0;
const ROW_PITCH: f32 = 36.0;
const FOCUS_SECONDS: f32 = 0.5;
const ATTRACT_SECONDS: f32 = 30.0;
pub(super) const ENTRY_SECONDS: f32 = 0.3;
pub(super) const EXIT_SECONDS: f32 = 0.3;

/// Retained selection and animation state for the title screen.
#[derive(Default)]
pub struct MenuState {
    selected: usize,
    color_index: i32,
    view: RuntimeView,
    entry_elapsed: f32,
    focus_elapsed: f32,
    idle_elapsed: f32,
    nav_chords: [MenuLrChord; 2],
}

impl MenuState {
    fn items(&self) -> &'static [MainMenuItem] {
        main_menu_items(
            self.view.coin_mode == CoinMode::Home,
            self.view.allow_shutdown_host,
        )
    }

    fn move_selection(&mut self, delta: isize) -> Effect {
        let count = self.items().len();
        let next = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
        if self.selected == next {
            return Effect::None;
        }
        self.selected = next;
        self.focus_elapsed = 0.0;
        Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change))
    }

    fn activate(&self) -> Effect {
        let destination = match self.items()[self.selected] {
            // The host admits arcade Start before calling the menu. In Pay mode
            // the updated view can already report zero remaining credits.
            MainMenuItem::Play => Effect::Navigate(OpenItgScreen::Play),
            MainMenuItem::Options => Effect::Navigate(OpenItgScreen::Options),
            MainMenuItem::Content => Effect::Navigate(OpenItgScreen::Content),
            MainMenuItem::Exit => Effect::Exit,
            MainMenuItem::Shutdown => Effect::Shutdown,
        };
        Effect::sequence(
            Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Confirm)),
            destination,
        )
    }

    fn cancel_chord(&mut self, input: &InputEvent) -> bool {
        if !self.view.dedicated_three_key_nav {
            return false;
        }
        let side = match input.action {
            VirtualAction::p1_menu_left | VirtualAction::p1_menu_right => 0,
            VirtualAction::p2_menu_left | VirtualAction::p2_menu_right => 1,
            _ => return false,
        };
        self.nav_chords[side].update(input)
    }
}

impl ThemeMainMenu for OpenItgTheme {
    type MenuState = MenuState;
    type MenuRuntimeView = RuntimeView;
    type MenuInput = InputEvent;
    type MenuRawKey = RawKeyboardEvent;
    type MenuVisualPolicy = ();

    fn init_menu(color_index: i32) -> MenuState {
        MenuState {
            color_index,
            ..MenuState::default()
        }
    }

    fn menu_color_index(state: &MenuState) -> i32 {
        state.color_index
    }

    fn set_menu_color_index(state: &mut MenuState, color_index: i32) {
        state.color_index = color_index;
    }

    fn sync_menu_runtime(state: &mut MenuState, view: RuntimeView) {
        if view.dedicated_three_key_nav != state.view.dedicated_three_key_nav {
            state.nav_chords = [MenuLrChord::default(); 2];
        }
        state.view = view;
        state.selected = state.selected.min(state.items().len() - 1);
    }

    fn update_menu(state: &mut MenuState, delta_time: f32, phase: MainMenuPhase) -> Effect {
        if delta_time <= 0.0 || !delta_time.is_finite() {
            return Effect::None;
        }
        state.entry_elapsed = (state.entry_elapsed + delta_time).min(ENTRY_SECONDS);
        state.focus_elapsed = (state.focus_elapsed + delta_time) % FOCUS_SECONDS;
        if phase == MainMenuPhase::Active {
            state.idle_elapsed += delta_time;
            if state.idle_elapsed >= ATTRACT_SECONDS {
                state.idle_elapsed = 0.0;
                return Effect::Navigate(OpenItgScreen::Attract);
            }
        }
        Effect::None
    }

    fn handle_menu_input(state: &mut MenuState, input: &InputEvent) -> Effect {
        state.idle_elapsed = 0.0;
        if state.cancel_chord(input) {
            return Effect::Navigate(OpenItgScreen::Attract);
        }
        if !input.pressed {
            return Effect::None;
        }

        use VirtualAction::*;
        match input.action {
            p1_start | p2_start => state.activate(),
            p1_back | p2_back => Effect::Navigate(OpenItgScreen::Attract),
            p1_menu_left | p1_menu_up | p2_menu_left | p2_menu_up => state.move_selection(-1),
            p1_menu_right | p1_menu_down | p2_menu_right | p2_menu_down => state.move_selection(1),
            p1_left | p1_up | p2_left | p2_up if !state.view.dedicated_three_key_nav => {
                state.move_selection(-1)
            }
            p1_right | p1_down | p2_right | p2_down if !state.view.dedicated_three_key_nav => {
                state.move_selection(1)
            }
            _ => Effect::None,
        }
    }

    fn handle_menu_raw_key(
        state: &mut MenuState,
        key: &RawKeyboardEvent,
    ) -> ThemeInputResult<OpenItgScreen, OpenItgRuntimeRequest> {
        state.idle_elapsed = 0.0;
        if !key.pressed || key.repeat {
            return ThemeInputResult::ignored();
        }
        match key.code {
            KeyCode::Escape => ThemeInputResult::consumed(Effect::Navigate(OpenItgScreen::Attract)),
            KeyCode::F4 => ThemeInputResult::consumed(Effect::Navigate(OpenItgScreen::Sandbox)),
            _ => ThemeInputResult::ignored(),
        }
    }

    fn push_menu_actors(
        actors: &mut Vec<Actor>,
        state: &MenuState,
        frame: MainMenuFrame<'_, OpenItgScreen, ()>,
    ) {
        push_title(actors, state, frame);
    }
}

fn transition_alpha(state: &MenuState, frame: &MainMenuFrame<'_, OpenItgScreen, ()>) -> (f32, f32) {
    let (opacity, flash) = match *frame.transition {
        TransitionState::FadingIn { elapsed, .. }
        | TransitionState::ActorsFadeIn { elapsed, .. } => (1.0, 1.0 - elapsed / ENTRY_SECONDS),
        TransitionState::FadingOut {
            elapsed, duration, ..
        }
        | TransitionState::ActorsFadeOut {
            elapsed, duration, ..
        } => (1.0 - elapsed / duration.max(f32::EPSILON), 0.0),
        TransitionState::Idle => (1.0, 1.0 - state.entry_elapsed / ENTRY_SECONDS),
    };
    (
        frame.alpha_multiplier * opacity.clamp(0.0, 1.0),
        frame.alpha_multiplier * flash.clamp(0.0, 1.0),
    )
}

fn item_label(item: MainMenuItem) -> &'static str {
    match item {
        MainMenuItem::Play => "Play",
        MainMenuItem::Options => "Options",
        MainMenuItem::Content => "Find Content",
        MainMenuItem::Exit => "Exit",
        MainMenuItem::Shutdown => "Shut Down",
    }
}

fn push_title(
    actors: &mut Vec<Actor>,
    state: &MenuState,
    frame: MainMenuFrame<'_, OpenItgScreen, ()>,
) {
    let width = screen_width();
    let height = screen_height();
    let layout = Layout::centered(width, height);
    let (alpha, flash) = transition_alpha(state, &frame);
    let ink = [INK[0], INK[1], INK[2], alpha];
    let muted = [MUTED[0], MUTED[1], MUTED[2], alpha];
    let accent = [ACCENT[0], ACCENT[1], ACCENT[2], alpha];

    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.035, 0.048, 0.072, alpha],
        -20,
    ));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.063, 0.087, 0.126, alpha], -19));
    actors.push(layout.panel(36.0, 70.0, [568.0, 1.0], [0.32, 0.40, 0.48, alpha], -17));
    actors.push(layout.panel(36.0, 426.0, [568.0, 1.0], [0.32, 0.40, 0.48, alpha], -17));
    actors.push(layout.label([36.0, 38.0], 0.0, "DEADSYNC".into(), 2.2, ink));

    // Stepped bars form an original, static rhythm motif beside the menu.
    for (index, length) in [70.0, 122.0, 98.0, 140.0, 76.0].into_iter().enumerate() {
        let x = 564.0 - length;
        let y = 127.0 + 34.0 * index as f32;
        let bar = if index == 2 {
            [0.38, 0.25, 0.13, alpha]
        } else {
            [0.12, 0.21, 0.29, alpha]
        };
        actors.push(layout.panel(x, y, [length, 19.0], bar, -18));
        actors.push(layout.panel(564.0, y, [5.0, 19.0], [0.31, 0.48, 0.60, alpha], -18));
    }

    let pulse = 1.0 - (2.0 * state.focus_elapsed / FOCUS_SECONDS - 1.0).abs();
    for (index, item) in state.items().iter().copied().enumerate() {
        let y = FIRST_ROW_Y + index as f32 * ROW_PITCH;
        let selected = index == state.selected;
        if selected {
            actors.push(layout.panel(94.0, y - 14.0, [250.0, 28.0], [0.16, 0.24, 0.30, alpha], 0));
            actors.push(layout.panel(
                94.0,
                y - 14.0,
                [3.0, 28.0],
                [
                    ACCENT[0],
                    ACCENT[1],
                    ACCENT[2],
                    alpha * (0.66 + 0.34 * pulse),
                ],
                1,
            ));
        }
        let (zoom, color) = if selected { (1.1, ink) } else { (1.0, muted) };
        actors.push(layout.label([120.0, y], 0.0, item_label(item).into(), zoom, color));
    }

    let prompt = if state.view.can_start {
        "PRESS START"
    } else {
        "INSERT COIN"
    };
    let footer_y = HEIGHT - 33.0;
    actors.push(layout.label([36.0, footer_y], 0.0, prompt.into(), 1.0, accent));
    if state.view.coin_mode == CoinMode::Pay {
        actors.push(layout.label([460.0, footer_y], 1.0, "CREDITS".into(), 0.9, muted));
        actors.push(layout.label(
            [604.0, footer_y],
            1.0,
            TextContent::inline_u32(state.view.credits),
            0.9,
            ink,
        ));
    } else if state.view.event_mode {
        actors.push(layout.label([604.0, footer_y], 1.0, "EVENT MODE".into(), 0.9, muted));
    }
    if frame.update_banner_tag.is_some() {
        actors.push(layout.label([604.0, 38.0], 1.0, "UPDATE AVAILABLE".into(), 0.75, accent));
    }
    if flash > 0.0 {
        actors.push(quad(
            [0.0, 0.0],
            [width, height],
            [1.0, 1.0, 1.0, flash],
            1000,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_core::input::InputSource;
    use deadsync_theme::ThemeFlowEvent;
    use std::time::Instant;

    fn press(state: &mut MenuState, action: VirtualAction) -> Effect {
        let now = Instant::now();
        let event = InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now);
        OpenItgTheme::handle_menu_input(state, &event)
    }

    fn assert_activation(effect: Effect, destination: ThemeFlowEvent<OpenItgScreen>) {
        let Effect::Batch(effects) = effect else {
            panic!("expected confirming cue and destination")
        };
        assert!(matches!(
            &effects[0],
            Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Confirm))
        ));
        assert_eq!(effects[1].flow_event(), Some(destination));
    }

    #[test]
    fn home_menu_can_select_options_and_exit() {
        let mut state = OpenItgTheme::init_menu(0);
        assert!(matches!(
            press(&mut state, VirtualAction::p1_menu_right),
            Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change))
        ));
        assert_activation(
            press(&mut state, VirtualAction::p1_start),
            ThemeFlowEvent::Navigate(OpenItgScreen::Options),
        );
        press(&mut state, VirtualAction::p1_menu_left);
        press(&mut state, VirtualAction::p1_menu_left);
        assert_activation(
            press(&mut state, VirtualAction::p1_start),
            ThemeFlowEvent::Exit,
        );
    }

    #[test]
    fn arcade_start_dispatches_play_after_the_host_consumes_the_last_credit() {
        let mut state = OpenItgTheme::init_menu(0);
        OpenItgTheme::sync_menu_runtime(
            &mut state,
            RuntimeView {
                coin_mode: CoinMode::Pay,
                credits: 0,
                can_start: false,
                allow_shutdown_host: true,
                ..RuntimeView::default()
            },
        );
        assert!(matches!(
            press(&mut state, VirtualAction::p2_menu_left),
            Effect::None
        ));
        assert_activation(
            press(&mut state, VirtualAction::p2_start),
            ThemeFlowEvent::Navigate(OpenItgScreen::Play),
        );
    }

    #[test]
    fn first_choice_keeps_native_coordinates_inside_a_wide_viewport() {
        let mut state = OpenItgTheme::init_menu(0);
        OpenItgTheme::update_menu(&mut state, ENTRY_SECONDS, MainMenuPhase::Entering);
        for (width, x) in [(640.0, 120.0), (854.0, 227.0)] {
            with_current_metrics(Metrics::centered(width, HEIGHT), || {
                let mut actors = Vec::new();
                OpenItgTheme::push_menu_actors(
                    &mut actors,
                    &state,
                    MainMenuFrame {
                        update_banner_tag: None,
                        alpha_multiplier: 1.0,
                        transition: &TransitionState::Idle,
                        visual_policy: (),
                    },
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content: TextContent::Static("Play"), offset, scale, .. }
                        if *offset == [x, FIRST_ROW_Y] && *scale == [1.1; 2]
                )));
            });
        }
    }
}
