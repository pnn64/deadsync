use crate::PulseCircuitTheme;
use crate::scene::{AMBER, MINT, MUTED, PAPER, Scene};
use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use deadlib_present::actors::Actor;
use deadsync_config::{coin::CoinMode, theme::GameFlag};
use deadsync_input::InputEvent;
use deadsync_theme::main_menu::{
    MainMenuFrame, MainMenuItem, MainMenuPhase, MainMenuRuntimeView, MainMenuSelection,
    ThemeMainMenu, main_menu_items,
};
use deadsync_theme::{Screen, ThemeEffect, ThemeInputResult, TransitionState};
use std::sync::Arc;

pub(crate) const ENTRY_SECONDS: f32 = 0.26;
pub(crate) const EXIT_SECONDS: f32 = 0.20;
type Runtime = MainMenuRuntimeView<GameFlag, CoinMode>;

#[derive(Default)]
pub struct MenuState {
    selection: MainMenuSelection,
    runtime: Runtime,
    color_index: i32,
    elapsed: f32,
    idle: f32,
    credits: Arc<str>,
}

fn items(state: &MenuState) -> &'static [MainMenuItem] {
    main_menu_items(
        state.runtime.coin_mode == CoinMode::Home,
        state.runtime.allow_shutdown_host,
    )
}

impl ThemeMainMenu for PulseCircuitTheme {
    type MenuState = MenuState;
    type MenuRuntimeView = Runtime;
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

    fn sync_menu_runtime(state: &mut MenuState, view: Runtime) {
        if state.runtime.credits != view.credits || state.credits.is_empty() {
            state.credits = Arc::from(format!("CREDITS  {:02}", view.credits));
        }
        state.runtime = view;
        let items = items(state);
        state
            .selection
            .sync(items, state.runtime.dedicated_three_key_nav);
    }

    fn update_menu<R>(
        state: &mut MenuState,
        delta: f32,
        phase: MainMenuPhase,
    ) -> ThemeEffect<Screen, R> {
        if delta.is_finite() && delta > 0.0 {
            state.elapsed = (state.elapsed + delta) % 320.0;
            if phase == MainMenuPhase::Active {
                state.idle += delta;
                if state.idle >= 26.0 {
                    state.idle = 0.0;
                    return ThemeEffect::Navigate(Screen::Init);
                }
            }
        }
        ThemeEffect::None
    }

    fn handle_menu_input<R>(state: &mut MenuState, input: &InputEvent) -> ThemeEffect<Screen, R> {
        state.idle = 0.0;
        let items = items(state);
        state.selection.input(input, items)
    }

    fn handle_menu_raw_key<R>(
        state: &mut MenuState,
        input: &RawKeyboardEvent,
    ) -> ThemeInputResult<Screen, R> {
        state.idle = 0.0;
        if input.pressed && !input.repeat && input.code == KeyCode::Escape {
            return ThemeInputResult::consumed(ThemeEffect::Navigate(Screen::Init));
        }
        ThemeInputResult::ignored()
    }

    fn push_menu_actors(
        actors: &mut Vec<Actor>,
        state: &MenuState,
        frame: MainMenuFrame<'_, Screen, ()>,
    ) {
        let reveal = match *frame.transition {
            TransitionState::ActorsFadeIn { elapsed, duration }
            | TransitionState::FadingIn { elapsed, duration } => elapsed / duration.max(0.001),
            TransitionState::ActorsFadeOut {
                elapsed, duration, ..
            }
            | TransitionState::FadingOut {
                elapsed, duration, ..
            } => 1.0 - elapsed / duration.max(0.001),
            TransitionState::Idle => 1.0,
        };
        let scene = Scene::new(frame.alpha_multiplier * reveal.clamp(0.0, 1.0));
        scene.cover(actors);
        scene.motif(actors, [45.0, 168.0], state.elapsed);
        scene.text(actors, [43.0, 230.0], [3.6, 280.0], "PULSE", PAPER);
        scene.text(actors, [44.0, 312.0], [2.6, 266.0], "CIRCUIT", MINT);
        scene.rect(actors, [321.0, 58.0], [1.0, 366.0], MUTED, 0.45, 0);
        let items = items(state);
        let rows_start = 237.0 - 25.0 * (items.len() - 1) as f32;
        for (index, item) in items.iter().enumerate() {
            let focused = index == state.selection.index();
            let y = rows_start + index as f32 * 50.0;
            let [ink, fill] = if focused {
                [MINT, MINT]
            } else {
                [PAPER, MUTED]
            };
            scene.rect(
                actors,
                [349.0, y - 19.0],
                [241.0, 38.0],
                fill,
                if focused { 0.13 } else { 0.065 },
                1,
            );
            if focused {
                scene.rect(actors, [349.0, y - 19.0], [5.0, 38.0], AMBER, 1.0, 2);
            }
            let label = match item {
                MainMenuItem::Play if !state.runtime.can_start => "INSERT CREDIT",
                MainMenuItem::Play => "PLAY",
                MainMenuItem::Options => "OPTIONS",
                MainMenuItem::Content => "CONTENT",
                MainMenuItem::Exit => "EXIT",
                MainMenuItem::Shutdown => "SHUT DOWN",
            };
            scene.text(actors, [368.0, y], [0.93, 210.0], label, ink);
        }
        if state.runtime.coin_mode == CoinMode::Pay {
            scene.text(actors, [45.0, 432.0], [0.62, 245.0], &state.credits, MUTED);
        }
        scene.text(
            actors,
            [349.0, 432.0],
            [0.54, 240.0],
            "START TO SELECT",
            MUTED,
        );
    }
}
