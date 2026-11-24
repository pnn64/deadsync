use std::{
    convert::Infallible,
    io,
    path::{Path, PathBuf},
};

use deadlib_assets::TextureDecodeJob;
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::Actor;
use deadlib_present::anim::TweenState;
use deadsync_config::{coin::CoinMode, theme::GameFlag};
use deadsync_input::InputEvent;
use deadsync_noteskin::script::parse_script_number;
use deadsync_theme::main_menu::{
    MainMenuFrame, MainMenuItem, MainMenuPhase, MainMenuRuntimeView, MainMenuSelection,
    ThemeMainMenu, main_menu_items,
};
use deadsync_theme::{Screen, Theme, ThemeEffect, ThemeScreenId, ThemeTransitions, TransitionKind};

use crate::bitmap_text::{BitmapText, TextPlayback};
use crate::scene::{CANVAS, SceneTextures, frame};
use crate::{ActorAnimation, BackgroundPlayback, ResourceCategory, ThemePackage};

const SECTION: &str = "ScreenTitleMenu";
pub const TITLE_FONT: &str = "local-theme-title";
type RuntimeView = MainMenuRuntimeView<GameFlag, CoinMode>;

pub struct PackageTitle;

#[derive(Default)]
pub struct PackageTitleState {
    color_index: i32,
    origin: [f32; 2],
    step: [f32; 2],
    idle_timeout: Option<f32>,
    idle: f32,
    background: Option<(BackgroundPlayback, BackgroundPlayback)>,
    choices: Vec<Choice>,
    visible: Vec<usize>,
    actions: Vec<MainMenuItem>,
    selection: MainMenuSelection,
}

struct Choice {
    action: MainMenuItem,
    text: BitmapText,
    playback: TextPlayback,
}

impl PackageTitle {
    /// Loads the title's declared choices, authored bitmap font, and background.
    pub fn open(
        root: &Path,
        language: Option<&str>,
    ) -> io::Result<(PackageTitleState, PathBuf, Vec<TextureDecodeJob>)> {
        let package = ThemePackage::open([root], language)?;
        let metric = |name: &str| package.metric(SECTION, name).map_err(io::Error::other);
        let (mut choices, mut font_path) = (Vec::new(), None);
        let names = metric("ChoiceNames")?.ok_or_else(|| invalid("missing ChoiceNames"))?;
        for name in names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let Some(action) = metric(&format!("Choice{name}"))?.and_then(host_action) else {
                continue;
            };
            let resource = format!("scroll choice{name}");
            let path = package
                .screen_resource(ResourceCategory::Graphics, SECTION, &resource)
                .map_err(io::Error::other)?
                .ok_or_else(|| invalid(format!("missing {resource}")))?;
            let text = BitmapText::read(path)?;
            let font = package
                .screen_resource(ResourceCategory::Fonts, "", &text.font)
                .map_err(io::Error::other)?
                .ok_or_else(|| invalid(format!("missing font {:?}", text.font)))?;
            if font_path.is_some_and(|loaded| loaded != font) {
                return Err(invalid("title choices must share their bitmap font"));
            }
            font_path = Some(font);
            let playback = text.start();
            choices.push(Choice {
                action,
                text,
                playback,
            });
        }
        if !choices
            .iter()
            .any(|choice| choice.action == MainMenuItem::Play)
        {
            return Err(invalid(
                "title needs a choice with a direct play destination",
            ));
        }
        let mut textures = SceneTextures::new("title");
        let background = textures
            .background(&package, SECTION)?
            .map(|playback| (playback.clone(), playback));
        let mut state = PackageTitleState {
            origin: [
                coordinate("ScrollerX", metric("ScrollerX")?)?,
                coordinate("ScrollerY", metric("ScrollerY")?)?,
            ],
            step: scroller_step(
                metric("ScrollerTransform")?.ok_or_else(|| invalid("missing ScrollerTransform"))?,
            )?,
            idle_timeout: metric("IdleTimeoutSeconds")?
                .and_then(parse_script_number)
                .filter(|seconds| seconds.is_finite() && *seconds > 0.0),
            background,
            choices,
            ..Default::default()
        };
        Self::sync_menu_runtime(&mut state, RuntimeView::default());
        Ok((
            state,
            font_path.expect("play choice has a font").to_owned(),
            textures.into_jobs(),
        ))
    }
}

impl PackageTitleState {
    fn focus_row(&mut self, row: Option<usize>, focused: bool) {
        if let Some(choice) = row.map(|index| &mut self.choices[index]) {
            choice.text.focus(&mut choice.playback, focused);
        }
    }

    fn focused_row(&self) -> Option<usize> {
        self.visible.get(self.selection.index()).copied()
    }
}

impl Theme for PackageTitle {
    type Screen = Screen;
    type RuntimeRequest = Infallible;

    fn screen_id(screen: Screen) -> ThemeScreenId {
        ThemeScreenId::new(screen.current_screen_file_name())
    }
}

impl ThemeTransitions for PackageTitle {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(_from: Screen, _to: Screen) -> TransitionKind {
        TransitionKind::Direct
    }
    fn actor_entry_duration(_screen: Screen) -> Option<f32> {
        None
    }
    fn actor_exit_duration(_screen: Screen) -> f32 {
        0.0
    }
    fn skip_global_entry_fade(_from: Screen, _to: Screen) -> bool {
        false
    }
    fn out_transition(_from: Screen, _to: Screen) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.0)
    }
    fn in_transition(_screen: Screen, (): Self::EntryContext<'_>) -> (Vec<Actor>, f32) {
        (Vec::new(), 0.0)
    }
}

impl ThemeMainMenu for PackageTitle {
    type MenuState = PackageTitleState;
    type MenuRuntimeView = RuntimeView;
    type MenuInput = InputEvent;
    type MenuRawKey = RawKeyboardEvent;
    type MenuVisualPolicy = ();

    fn init_menu(color_index: i32) -> PackageTitleState {
        PackageTitleState {
            color_index,
            ..Default::default()
        }
    }

    fn menu_color_index(state: &PackageTitleState) -> i32 {
        state.color_index
    }
    fn set_menu_color_index(state: &mut PackageTitleState, color_index: i32) {
        state.color_index = color_index;
    }

    fn reset_menu(state: &mut PackageTitleState) {
        state.idle = 0.0;
        state.selection = MainMenuSelection::default();
        if let Some((initial, current)) = &mut state.background {
            *current = initial.clone();
        }
        for choice in &mut state.choices {
            choice.playback = choice.text.start();
        }
        state.focus_row(state.focused_row(), true);
    }

    fn sync_menu_runtime(state: &mut PackageTitleState, view: RuntimeView) {
        let old = state.focused_row();
        let allowed = main_menu_items(view.coin_mode == CoinMode::Home, view.allow_shutdown_host);
        state.visible.clear();
        state.actions.clear();
        for (index, choice) in state.choices.iter().enumerate() {
            if allowed.contains(&choice.action) {
                state.visible.push(index);
                state.actions.push(choice.action);
            }
        }
        state
            .selection
            .sync(&state.actions, view.dedicated_three_key_nav);
        let focus = state.focused_row();
        if old != focus {
            state.focus_row(old, false);
            state.focus_row(focus, true);
        }
    }

    fn update_menu<R>(
        state: &mut PackageTitleState,
        delta: f32,
        phase: MainMenuPhase,
    ) -> ThemeEffect<Screen, R> {
        if !delta.is_finite() || delta <= 0.0 {
            return ThemeEffect::None;
        }
        if let Some((_, background)) = &mut state.background {
            background.advance(delta);
        }
        for choice in &mut state.choices {
            choice.playback.advance(delta);
        }
        if phase == MainMenuPhase::Active
            && let Some(timeout) = state.idle_timeout
        {
            state.idle += delta;
            if state.idle >= timeout {
                state.idle = 0.0;
                return ThemeEffect::Navigate(Screen::Init);
            }
        }
        ThemeEffect::None
    }

    fn handle_menu_input<R>(
        state: &mut PackageTitleState,
        input: &InputEvent,
    ) -> ThemeEffect<Screen, R> {
        state.idle = 0.0;
        let old = state.focused_row();
        let effect = state.selection.input(input, &state.actions);
        if old != state.focused_row() {
            state.focus_row(old, false);
            state.focus_row(state.focused_row(), true);
        }
        effect
    }

    fn push_menu_actors(
        actors: &mut Vec<Actor>,
        state: &PackageTitleState,
        view: MainMenuFrame<'_, Screen, ()>,
    ) {
        let mut children = Vec::new();
        if let Some((_, background)) = &state.background {
            background.push_actors(&mut children, i16::MIN);
        }
        for (item, &row) in state.visible.iter().enumerate() {
            let choice = &state.choices[row];
            let offset = item as f32 - state.selection.index() as f32;
            let [x, y] = state.origin;
            let at = [
                state.step[0].mul_add(offset, x),
                state.step[1].mul_add(offset, y),
            ];
            children.extend(choice.text.draw(
                &choice.playback,
                TITLE_FONT,
                at,
                view.alpha_multiplier,
            ));
        }
        actors.push(frame(children));
    }
}

fn host_action(command: &str) -> Option<MainMenuItem> {
    // Authored screen routes map to shared host actions. Computed destinations
    // require a script runtime and cannot be selected by this presentation.
    let (verb, destination) = command.trim().split_once(',')?;
    if !verb.eq_ignore_ascii_case("screen") {
        return None;
    }
    Some(match destination.trim() {
        "ScreenCaution" | "ScreenSelectStyle" | "ScreenSelectProfile" => MainMenuItem::Play,
        "ScreenOptionsMenu" | "ScreenOptions" => MainMenuItem::Options,
        "ScreenContentBrowser" => MainMenuItem::Content,
        "ScreenExit" => MainMenuItem::Exit,
        "ScreenShutdown" => MainMenuItem::Shutdown,
        _ => return None,
    })
}

fn coordinate(key: &str, value: Option<&str>) -> io::Result<f32> {
    let value = value.ok_or_else(|| invalid(format!("missing {key}")))?;
    let animation =
        ActorAnimation::parse(&format!("x,{value}"), CANVAS).map_err(io::Error::other)?;
    Ok(animation.start(TweenState::default()).state().x)
}

fn scroller_step(transform: &str) -> io::Result<[f32; 2]> {
    // Support a declarative axis and pitch from a legacy scroller transform.
    let unsupported =
        || invalid("ScrollerTransform must position x or y by a constant times offset");
    let (_, body) = transform
        .trim()
        .strip_prefix("function(")
        .and_then(|text| text.split_once(')'))
        .ok_or_else(unsupported)?;
    let body = body
        .trim()
        .strip_suffix("end")
        .ok_or_else(unsupported)?
        .trim();
    let (axis, value) = body
        .strip_prefix("self:")
        .and_then(|text| text.split_once('('))
        .ok_or_else(unsupported)?;
    let (left, right) = value
        .strip_suffix(')')
        .and_then(|text| text.split_once('*'))
        .ok_or_else(unsupported)?;
    let factor = match (left.trim(), right.trim()) {
        ("offset", factor) | (factor, "offset") => factor,
        _ => return Err(unsupported()),
    };
    let pitch = parse_script_number(factor)
        .filter(|pitch| pitch.is_finite())
        .ok_or_else(unsupported)?;
    match axis {
        "x" => Ok([pitch, 0.0]),
        "y" => Ok([0.0, pitch]),
        _ => Err(unsupported()),
    }
}

fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("[{SECTION}] {message}"))
}
