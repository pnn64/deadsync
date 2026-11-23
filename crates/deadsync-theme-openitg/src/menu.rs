use crate::intro::{LOGO_ENTRY_SECONDS, LOGO_MOVIE, LOGO_POSTER, push_logo_layers};
use crate::presentation::{HEIGHT, Layout, italic, quad, sprite};
use crate::resources::{TITLE_CHROME, TITLE_FONT, TITLE_GLINT, TITLE_STREAK};
use crate::OpenItgTheme;
use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::render::BlendMode;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_config::coin::CoinMode;
use deadsync_config::theme::GameFlag;
use deadsync_input::{InputEvent, MenuLrChord, VirtualAction};
use deadsync_theme::main_menu::{
    MainMenuFrame, MainMenuItem, MainMenuPhase, MainMenuRuntimeView, MenuBackgroundMovie,
    ThemeMainMenu, main_menu_items,
};
use deadsync_theme::{Screen, ThemeEffect, ThemeInputResult, TransitionState, UiCue};
use std::sync::Arc;

type Effect<R> = ThemeEffect<Screen, R>;
type RuntimeView = MainMenuRuntimeView<GameFlag, CoinMode>;
const FIRST_ROW_Y: f32 = 112.0;
const ROW_PITCH: f32 = 36.0;
const FOCUS_SECONDS: f32 = 0.5;
const ATTRACT_SECONDS: f32 = 30.0;
const FRAME_REVEAL_SECONDS: f32 = 1.0;
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

    fn move_selection<R>(&mut self, delta: isize) -> Effect<R> {
        let count = self.items().len();
        let next = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
        if self.selected == next {
            return Effect::None;
        }
        self.selected = next;
        self.focus_elapsed = 0.0;
        Effect::UiCue(UiCue::Change)
    }

    fn activate<R>(&self) -> Effect<R> {
        let destination = match self.items()[self.selected] {
            // The host admits arcade Start before calling the menu. In Pay mode
            // the updated view can already report zero remaining credits.
            MainMenuItem::Play => Effect::Navigate(Screen::SelectProfile),
            MainMenuItem::Options => Effect::Navigate(Screen::Options),
            MainMenuItem::Content => Effect::Navigate(Screen::ContentBrowser),
            MainMenuItem::Exit => Effect::Exit,
            MainMenuItem::Shutdown => Effect::Shutdown,
        };
        Effect::sequence(
            Effect::UiCue(UiCue::Confirm),
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

    const MENU_BACKGROUND_MOVIE: Option<MenuBackgroundMovie> = Some(MenuBackgroundMovie {
        path: LOGO_MOVIE,
        rate: 2.0,
    });

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

    fn update_menu<R>(state: &mut MenuState, delta_time: f32, phase: MainMenuPhase) -> Effect<R> {
        if delta_time <= 0.0 || !delta_time.is_finite() {
            return Effect::None;
        }
        state.entry_elapsed = (state.entry_elapsed + delta_time).min(LOGO_ENTRY_SECONDS);
        state.focus_elapsed = (state.focus_elapsed + delta_time) % FOCUS_SECONDS;
        if phase == MainMenuPhase::Active {
            state.idle_elapsed += delta_time;
            if state.idle_elapsed >= ATTRACT_SECONDS {
                state.idle_elapsed = 0.0;
                return Effect::Navigate(Screen::Init);
            }
        }
        Effect::None
    }

    fn handle_menu_input<R>(state: &mut MenuState, input: &InputEvent) -> Effect<R> {
        state.idle_elapsed = 0.0;
        if state.cancel_chord(input) {
            return Effect::Navigate(Screen::Init);
        }
        if !input.pressed {
            return Effect::None;
        }

        use VirtualAction::*;
        match input.action {
            p1_start | p2_start => state.activate(),
            p1_back | p2_back => Effect::Navigate(Screen::Init),
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

    fn handle_menu_raw_key<R>(
        state: &mut MenuState,
        key: &RawKeyboardEvent,
    ) -> ThemeInputResult<Screen, R> {
        state.idle_elapsed = 0.0;
        if !key.pressed || key.repeat {
            return ThemeInputResult::ignored();
        }
        match key.code {
            KeyCode::Escape => ThemeInputResult::consumed(Effect::Navigate(Screen::Init)),
            KeyCode::F4 => ThemeInputResult::consumed(Effect::Navigate(Screen::Sandbox)),
            _ => ThemeInputResult::ignored(),
        }
    }

    fn push_menu_actors(
        actors: &mut Vec<Actor>,
        state: &MenuState,
        frame: MainMenuFrame<'_, Screen, ()>,
    ) {
        push_title(actors, state, frame);
    }
}

fn transition_alpha(state: &MenuState, frame: &MainMenuFrame<'_, Screen, ()>) -> (f32, f32) {
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

fn title_text(
    layout: Layout,
    position: [f32; 2],
    alignment: f32,
    content: TextContent,
    zoom: f32,
    ink: [f32; 4],
    z: i16,
) -> Actor {
    let mut text = layout.text(position, alignment, content, zoom, ink);
    text.font(TITLE_FONT);
    text.shadowlength(layout.size([2.0, 0.0])[0]);
    text.z(z);
    text.build(0)
}

fn title_image(
    key: &'static str,
    [x, y]: [f32; 2],
    [width, height]: [f32; 2],
    alpha: f32,
    z: i16,
) -> SpriteBuilder {
    sprite(key, [x, y], [width, height], [1.0, 1.0, 1.0, alpha], z)
}

fn push_title(
    actors: &mut Vec<Actor>,
    state: &MenuState,
    frame: MainMenuFrame<'_, Screen, ()>,
) {
    let width = screen_width();
    let height = screen_height();
    let center = [width * 0.5, height * 0.5];
    let layout = Layout::centered(width, height);
    let side_inset = layout.position(0.0, 0.0)[0];
    let left = layout.translated_x(-side_inset);
    let right = layout.translated_x(side_inset);
    let (alpha, flash) = transition_alpha(state, &frame);
    let white = [1.0, 1.0, 1.0, alpha];

    let mut backdrop = frame.background_key.map_or_else(
        || SpriteBuilder::static_texture(LOGO_POSTER),
        |key| SpriteBuilder::texture(Arc::clone(key)),
    );
    backdrop.align(0.5, 0.5);
    backdrop.xy(center[0], center[1]);
    backdrop.size(width, height);
    backdrop.alpha(alpha * 0.7);
    backdrop.z(-32);
    actors.push(backdrop.build(0));
    push_logo_layers(actors, layout, state.entry_elapsed, alpha, -30);
    let reveal = (state.entry_elapsed / FRAME_REVEAL_SECONDS).clamp(0.0, 1.0);
    let vertical_scale = height / HEIGHT;
    let mut chrome = title_image(
        TITLE_CHROME,
        center,
        [width, 346.0 * vertical_scale],
        alpha,
        -23,
    );
    chrome.cropright(1.0 - reveal);
    actors.push(chrome.build(0));
    for y in [42.0, 446.0] {
        let mut streak = title_image(
            TITLE_STREAK,
            [center[0], y * vertical_scale],
            [width, 34.0 * vertical_scale],
            alpha,
            -23,
        );
        streak.cropright(1.0 - reveal);
        actors.push(streak.build(0));
    }

    if reveal < 1.0 {
        let x = reveal.mul_add(width + 128.0, -64.0);
        for (y, direction) in [(64.0, 1.0), (416.0, -1.0)] {
            let mut glint = title_image(
                TITLE_GLINT,
                [x, y * vertical_scale],
                [128.0 * vertical_scale; 2],
                alpha * 0.65,
                -22,
            );
            glint.blend(BlendMode::Add);
            glint.rotationz(direction * 360.0 * reveal);
            actors.push(glint.build(0));
        }
    }
    actors.push(italic(
        title_text(
            left,
            [36.0, 39.0],
            0.0,
            "MAIN MENU".into(),
            1.13,
            white,
            16,
        ),
        -0.22,
    ));

    let pulse = 0.5 - 0.5 * (state.focus_elapsed * (std::f32::consts::TAU / FOCUS_SECONDS)).cos();
    for (index, item) in state.items().iter().copied().enumerate() {
        let y = FIRST_ROW_Y + index as f32 * ROW_PITCH;
        let (zoom, color) = if index == state.selected {
            let shade = 0.5 + 0.5 * pulse;
            (1.1, [shade, 0.5 * shade, 0.5 * shade, alpha])
        } else {
            (1.0, white)
        };
        actors.push(title_text(
            layout,
            [120.0, y],
            0.0,
            item_label(item).into(),
            zoom,
            color,
            12,
        ));
    }

    let prompt = if state.view.can_start {
        "PRESS START"
    } else {
        "INSERT COIN"
    };
    let footer_y = HEIGHT - 16.0;
    for (edge, x, alignment) in [(left, 36.0, 0.0), (right, 604.0, 1.0)] {
        actors.push(title_text(
            edge,
            [x, footer_y],
            alignment,
            prompt.into(),
            0.63,
            white,
            16,
        ));
    }
    if state.view.coin_mode == CoinMode::Home {
        let controls = if state.view.dedicated_three_key_nav {
            "Press &MENULEFT;&MENURIGHT; to choose, then press &START;"
        } else {
            "Press &MENUUP;&MENUDOWN; to choose, then press &START;"
        };
        actors.push(title_text(
            layout,
            [320.0, 445.0],
            0.5,
            controls.into(),
            0.70,
            white,
            16,
        ));
    }
    let status = if state.view.coin_mode == CoinMode::Pay {
        TextContent::inline_format(format_args!("CREDITS {}", state.view.credits))
    } else {
        state.view.event_mode.then(|| "EVENT MODE".into())
    };
    if let Some(status) = status {
        actors.push(title_text(
            layout,
            [320.0, footer_y],
            0.5,
            status,
            0.59,
            white,
            16,
        ));
    }
    if frame.update_banner_tag.is_some() {
        actors.push(title_text(
            right,
            [604.0, 39.0],
            1.0,
            "UPDATE AVAILABLE".into(),
            0.59,
            [1.0, 0.80, 0.50, alpha],
            16,
        ));
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
    use deadlib_present::actors::SpriteSource;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_core::input::InputSource;
    use deadsync_theme::ThemeFlowEvent;
    use std::time::Instant;

    fn press(state: &mut MenuState, action: VirtualAction) -> Effect<()> {
        let now = Instant::now();
        let event = InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now);
        OpenItgTheme::handle_menu_input(state, &event)
    }

    fn assert_activation(effect: Effect<()>, destination: ThemeFlowEvent<Screen>) {
        let Effect::Batch(effects) = effect else {
            panic!("expected confirming cue and destination")
        };
        assert!(matches!(
            &effects[0],
            Effect::UiCue(UiCue::Confirm)
        ));
        assert_eq!(effects[1].flow_event(), Some(destination));
    }

    #[test]
    fn home_menu_can_select_options_and_exit() {
        let mut state = OpenItgTheme::init_menu(0);
        assert!(matches!(
            press(&mut state, VirtualAction::p1_menu_right),
            Effect::UiCue(UiCue::Change)
        ));
        assert_activation(
            press(&mut state, VirtualAction::p1_start),
            ThemeFlowEvent::Navigate(Screen::Options),
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
            ThemeFlowEvent::Navigate(Screen::SelectProfile),
        );
    }

    #[test]
    fn first_choice_keeps_native_coordinates_inside_a_wide_viewport() {
        let mut state = OpenItgTheme::init_menu(0);
        OpenItgTheme::update_menu::<()>(&mut state, ENTRY_SECONDS, MainMenuPhase::Entering);
        let movie_key = Arc::from("fixture/logo.mpg");
        for (width, x, background_key) in [
            (640.0, 120.0, None),
            (854.0, 227.0, Some(&movie_key)),
        ] {
            with_current_metrics(Metrics::centered(width, HEIGHT), || {
                let mut actors = Vec::new();
                OpenItgTheme::push_menu_actors(
                    &mut actors,
                    &state,
                    MainMenuFrame {
                        update_banner_tag: None,
                        background_key,
                        alpha_multiplier: 1.0,
                        transition: &TransitionState::Idle,
                        visual_policy: (),
                    },
                );
                let Actor::Sprite { source, tint, z, .. } = &actors[0] else {
                    panic!("the badge and menu should draw over a background sprite");
                };
                match (background_key, source) {
                    (None, SpriteSource::TextureStatic(LOGO_POSTER)) => {}
                    (Some(movie), SpriteSource::Texture(key)) if movie == key => {}
                    _ => panic!("unexpected title background {source:?}"),
                }
                assert_eq!(*tint, [1.0, 1.0, 1.0, 0.7]);
                assert_eq!(*z, -32);
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content: TextContent::Static("Play"), font: TITLE_FONT, offset, scale, .. }
                        if *offset == [x, FIRST_ROW_Y] && *scale == [1.1; 2]
                )));
                assert!(actors.iter().any(|actor| matches!(
                    actor,
                    Actor::Text {
                        content: TextContent::Static("MAIN MENU"),
                        font: TITLE_FONT,
                        offset,
                        ..
                    } if *offset == [36.0, 39.0]
                )));
            });
        }
    }
}
