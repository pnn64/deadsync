use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadlib_present::anim::{self, TweenSeq, TweenState};
use deadsync_config::{coin::CoinMode, theme::GameFlag};
use deadsync_input::InputEvent;
use deadsync_theme::main_menu::{
    MainMenuFrame, MainMenuItem, MainMenuPhase, MainMenuRuntimeView, MainMenuSelection,
    ThemeMainMenu,
};
use deadsync_theme::{Screen, ThemeEffect};

use crate::StepManiaTheme;
use crate::presentation::Layout;
use crate::resources::{NORMAL_FONT, TITLE_FONT};

type RuntimeView = MainMenuRuntimeView<GameFlag, CoinMode>;
const ACTIONS: &[MainMenuItem] = &[
    MainMenuItem::Play,
    MainMenuItem::Options,
    MainMenuItem::Exit,
];
const ROWS: &[(&str, Option<MainMenuItem>)] = &[
    ("GAME START", Some(MainMenuItem::Play)),
    ("SELECT GAME", None),
    ("OPTIONS", Some(MainMenuItem::Options)),
    ("EDIT/SYNC SONGS", None),
    ("EDIT COURSES", None),
    ("JUKEBOX", None),
    ("EXIT", Some(MainMenuItem::Exit)),
];

pub struct MenuState {
    selection: MainMenuSelection,
    color_index: i32,
    runtime: RuntimeView,
    elapsed: f32,
    idle: f32,
    focus_age: f32,
    zooms: [TweenSeq; 7],
}

impl MenuState {
    fn items(&self) -> &'static [MainMenuItem] {
        if self.runtime.coin_mode == CoinMode::Home {
            ACTIONS
        } else {
            &ACTIONS[..1]
        }
    }

    fn selected_row(&self) -> usize {
        [0, 2, 6][self.selection.index()]
    }
}

impl ThemeMainMenu for StepManiaTheme {
    type MenuState = MenuState;
    type MenuRuntimeView = RuntimeView;
    type MenuInput = InputEvent;
    type MenuRawKey = RawKeyboardEvent;
    type MenuVisualPolicy = ();

    fn init_menu(color_index: i32) -> MenuState {
        let mut state = MenuState {
            selection: MainMenuSelection::default(),
            color_index,
            runtime: RuntimeView::default(),
            elapsed: 0.0,
            idle: 0.0,
            focus_age: 0.0,
            zooms: std::array::from_fn(|_| TweenSeq::new(TweenState::default())),
        };
        state.zooms[0].push_step(anim::linear(0.3).zoom(1.3, 1.3).build());
        state
    }

    fn menu_color_index(state: &MenuState) -> i32 {
        state.color_index
    }

    fn set_menu_color_index(state: &mut MenuState, color_index: i32) {
        state.color_index = color_index;
    }

    fn reset_menu(state: &mut MenuState) {
        let runtime = state.runtime.clone();
        *state = Self::init_menu(state.color_index);
        Self::sync_menu_runtime(state, runtime);
    }

    fn sync_menu_runtime(state: &mut MenuState, view: RuntimeView) {
        state.runtime = view;
        state
            .selection
            .sync(state.items(), state.runtime.dedicated_three_key_nav);
    }

    fn update_menu<R>(
        state: &mut MenuState,
        dt: f32,
        phase: MainMenuPhase,
    ) -> ThemeEffect<Screen, R> {
        if !dt.is_finite() || dt <= 0.0 {
            return ThemeEffect::None;
        }
        state.elapsed += dt;
        state.focus_age += dt;
        for zoom in &mut state.zooms {
            zoom.update(dt);
        }
        if phase == MainMenuPhase::Active {
            state.idle += dt;
            if state.idle >= 30.0 {
                state.idle = 0.0;
                return ThemeEffect::Navigate(Screen::Init);
            }
        }
        ThemeEffect::None
    }

    fn handle_menu_input<R>(state: &mut MenuState, input: &InputEvent) -> ThemeEffect<Screen, R> {
        state.idle = 0.0;
        let old = state.selected_row();
        let effect = state.selection.input(input, state.items());
        let selected = state.selected_row();
        if old != selected {
            state.focus_age = 0.0;
            for (row, target) in [(old, 1.0), (selected, 1.3)] {
                state.zooms[row].clear();
                state.zooms[row].push_step(anim::linear(0.3).zoom(target, target).build());
            }
        }
        effect
    }

    fn push_menu_actors(
        actors: &mut Vec<Actor>,
        state: &MenuState,
        frame: MainMenuFrame<'_, Screen, ()>,
    ) {
        let layout = Layout::current();
        let alpha = frame.alpha_multiplier;
        let mut background = layout.sprite(
            "stepmania-395/title-background",
            [320.0, 240.0],
            [640.0, 480.0],
            -100,
        );
        background.alpha(alpha);
        actors.push(background.build(0));
        push_logo(actors, layout, state.runtime.game, state.elapsed, alpha);

        let home = state.runtime.coin_mode == CoinMode::Home;
        let selected = state.selected_row();
        for (index, &(label, action)) in ROWS.iter().enumerate() {
            if !home && index != 0 {
                continue;
            }
            let y = 90.0 + (index as f32 - selected as f32) * 50.0;
            let mut text = layout.text(
                TITLE_FONT,
                TextContent::Static(label),
                [320.0, y],
                state.zooms[index].state().scale[0],
            );
            let color = if index == selected {
                let t = 0.5 - 0.5 * (state.focus_age * std::f32::consts::TAU / 0.5).cos();
                [0.5 - 0.25 * t, 1.0 - 0.5 * t, 0.5 - 0.25 * t, alpha]
            } else if action.is_none() {
                [0.5, 0.5, 0.5, alpha]
            } else {
                [1.0, 1.0, 1.0, alpha]
            };
            text.diffuse(color);
            text.shadowlength(5.0 * layout.scale);
            actors.push(text.build(0));
        }
        if let Some(counts) = TextContent::inline_format(format_args!(
            "{} songs in {} groups, {} courses",
            state.runtime.song_count, state.runtime.pack_count, state.runtime.course_count
        )) {
            let mut text = layout.text(NORMAL_FONT, counts, [20.0, 20.0], 0.5);
            text.align(0.0, 0.5);
            text.horizalign(TextAlign::Left);
            text.diffuse([0.6, 0.6, 0.6, alpha]);
            text.shadowlength(2.0 * layout.scale);
            actors.push(text.build(0));
        }
    }
}

pub(crate) fn push_logo(
    actors: &mut Vec<Actor>,
    layout: Layout,
    game: GameFlag,
    elapsed: f32,
    alpha: f32,
) {
    let (key, dimensions) = match game {
        GameFlag::Dance => ("stepmania-395/title-dance", [512.0, 316.0]),
        GameFlag::Pump => ("stepmania-395/title-pump", [464.0, 206.0]),
    };
    let mut logo = layout.sprite(key, [320.0, 240.0], dimensions, -1);
    logo.zoomy(anim::bounceend_p(((elapsed - 0.5) / 0.5).clamp(0.0, 1.0)));
    logo.alpha(alpha);
    logo.glow([
        1.0,
        1.0,
        1.0,
        alpha * (0.2 - 0.1 * (elapsed * std::f32::consts::TAU / 2.5).cos()),
    ]);
    actors.push(logo.build(0));
}
