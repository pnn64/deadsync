use deadlib_present::{
    actors::Actor,
    anim,
    dsl::{SpriteBuilder, TweenSteps},
    runtime,
    space::{screen_height, screen_width},
};
use deadsync_theme::{Screen, ThemeTransitions, TransitionKind};

use crate::{StepManiaTheme, presentation::Layout};

impl ThemeTransitions for StepManiaTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(from: Screen, to: Screen) -> TransitionKind {
        if (from, to) == (Screen::Init, Screen::Menu) {
            TransitionKind::Direct
        } else {
            TransitionKind::Global
        }
    }

    fn actor_entry_duration(screen: Screen) -> Option<f32> {
        (screen == Screen::Menu).then_some(0.3)
    }

    fn actor_exit_duration(screen: Screen) -> f32 {
        match screen {
            Screen::Menu => 0.3,
            Screen::SelectMusic | Screen::SelectCourse => 1.5,
            Screen::Evaluation => 1.3,
            _ => 0.5,
        }
    }

    fn skip_global_entry_fade(_from: Screen, _to: Screen) -> bool {
        false
    }

    fn in_transition(screen: Screen, (): ()) -> (Vec<Actor>, f32) {
        match screen {
            Screen::Menu => (vec![black_fade(true)], 0.3),
            Screen::SelectPlayMode | Screen::SelectMusic | Screen::SelectCourse => {
                (vec![move_on(true, true)], 0.5)
            }
            Screen::EvaluationSummary => (vec![move_on(true, false)], 1.3),
            Screen::Initials => (vec![black_fade(true)], 0.3),
            Screen::GameOver => (Vec::new(), 0.0),
            Screen::Evaluation => (Vec::new(), 1.3),
            Screen::Init => (Vec::new(), 0.0),
            _ => (Vec::new(), 0.5),
        }
    }

    fn out_transition(from: Screen, _to: Screen) -> (Vec<Actor>, f32) {
        match from {
            Screen::Menu => (vec![black_fade(false)], 0.3),
            Screen::SelectStyle | Screen::SelectPlayMode => (vec![move_on(false, true)], 0.5),
            Screen::EvaluationSummary => (vec![fade_after(false, 1.0)], 1.3),
            Screen::Initials | Screen::GameOver => (vec![black_fade(false)], 0.3),
            Screen::SelectMusic | Screen::SelectCourse => (vec![black_fade(false)], 1.5),
            Screen::Evaluation => (Vec::new(), 1.3),
            Screen::Init => (Vec::new(), 0.0),
            _ => (Vec::new(), 0.5),
        }
    }
}

pub(crate) fn black_fade(arriving: bool) -> Actor {
    fade_after(arriving, 0.0)
}

fn fade_after(arriving: bool, delay: f32) -> Actor {
    const SITE: u64 = runtime::site_base(file!(), line!(), column!());
    let mut cover = SpriteBuilder::solid();
    cover.tweensalt(u64::from(arriving) | (u64::from(delay.to_bits()) << 1));
    cover.align(0.0, 0.0);
    cover.size(screen_width(), screen_height());
    cover.diffuse([0.0, 0.0, 0.0, if arriving { 1.0 } else { 0.0 }]);
    cover.z(1200);
    cover.build_tweened(SITE, || {
        let mut steps = TweenSteps::new();
        steps.push(anim::sleep(delay));
        steps.push(
            anim::linear(0.3)
                .alpha(if arriving { 0.0 } else { 1.0 })
                .build(),
        );
        steps
    })
}

fn move_on(arriving: bool, fade_rgb: bool) -> Actor {
    const SITE: u64 = runtime::site_base(file!(), line!(), column!());
    let mut sign = Layout::current().sprite(
        "stepmania-395/evaluation-move-on",
        [320.0, 240.0],
        [368.0, 176.0],
        1200,
    );
    sign.tweensalt(u64::from(arriving) | (u64::from(fade_rgb) << 1));
    sign.zoomy(if arriving { 1.0 } else { 0.0 });
    let start = if arriving { 1.0 } else { 0.0 };
    sign.diffuse([start; 4]);
    sign.build_tweened(SITE, || {
        let mut steps = TweenSteps::new();
        let end = if arriving { 0.0 } else { 1.0 };
        steps.push(
            anim::linear(0.5)
                .zoomy(if arriving { 0.0 } else { 1.0 })
                .diffuse(
                    if fade_rgb { end } else { 1.0 },
                    if fade_rgb { end } else { 1.0 },
                    if fade_rgb { end } else { 1.0 },
                    end,
                )
                .build(),
        );
        steps
    })
}
