use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, machine_font_key};
use crate::screens::components::shared::screen_bar::{
    ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::{screen_bar, select_flow_footer, visual_style_bg};
use crate::screens::select_style_flow::{
    self as style_flow, Choice, InputEffect, State as StyleFlow,
};
use crate::screens::{Screen, ThemeEffect};
use crate::views::SelectFlowRuntimeView;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, widescale};
use deadsync_input::InputEvent;
use deadsync_theme::AudioRequest;

/* ------------------------------ layout ------------------------------- */
// Simply Love: ScreenSelectStyle underlay/choice.lua
const CHOICE_NOT_CHOSEN_FADE_DELAY: f32 = 0.1;
const CHOICE_NOT_CHOSEN_FADE_DURATION: f32 = 0.2;
const PAD_TILE_NATIVE_SIZE: f32 = 64.0;
const PAD_TILE_ZOOM_4_3: f32 = 0.435;
const PAD_TILE_ZOOM_16_9: f32 = 0.525;
const PAD_DUAL_OFFSET_4_3: f32 = 42.0;
const PAD_DUAL_OFFSET_16_9: f32 = 51.0;
const CHOICE_X_OFFSET_4_3: f32 = 160.0;
const CHOICE_X_OFFSET_16_9: f32 = 214.0;
const CHOICE_Y_OFFSET_4_3: f32 = 0.0;
const CHOICE_Y_OFFSET_16_9: f32 = 10.0;

const PAD_UNUSED_RGBA: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
const DANCE_PAD_LAYOUT: [bool; 9] = [false, true, false, true, false, true, false, true, false];

#[inline(always)]
fn choice_label(choice: Choice) -> String {
    let key = match choice {
        Choice::Single => "SinglePlayer",
        Choice::Versus => "TwoPlayers",
        Choice::Double => "Double",
    };
    tr("SelectStyle", key).to_string()
}

pub struct State {
    pub active_color_index: i32,
    flow: StyleFlow,
    bg: visual_style_bg::State,
    runtime: SelectFlowRuntimeView,
}

pub fn init(runtime: SelectFlowRuntimeView) -> State {
    let mut flow = StyleFlow::default();
    flow.set_selected_index(usize::from(
        runtime.players.iter().all(|player| player.joined),
    ));
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        flow,
        bg: visual_style_bg::State::new(),
        runtime,
    }
}

#[inline(always)]
pub fn set_selected_index(state: &mut State, index: usize) {
    state.flow.set_selected_index(index);
}

pub fn sync_runtime_view(state: &mut State, runtime: SelectFlowRuntimeView) {
    state.runtime = runtime;
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    // Simply Love handles transitions via per-actor OffCommands and a sleep in out.lua.
    (vec![], 0.0)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    // Simply Love handles transitions via per-actor OffCommands and a sleep in out.lua.
    (vec![], 0.0)
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    let exit_elapsed = if state.flow.exit_chosen_anim() {
        exit_anim_t(true)
    } else {
        0.0
    };
    style_flow::update(&mut state.flow, dt, exit_elapsed).map(ThemeEffect::Navigate)
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    match style_flow::handle_input(&mut state.flow, ev) {
        InputEffect::None => ThemeEffect::None,
        InputEffect::Move => ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::PlaySfx("assets/sounds/change.ogg".to_owned()),
        )),
        InputEffect::Confirm(play_style) => {
            let _ = exit_anim_t(true);
            state.runtime.play_style = play_style;
            crate::effects::sfx_then(
                "assets/sounds/start.ogg",
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                    crate::SimplyLoveProfileRequest::SetPlayStyle(play_style),
                )),
            )
        }
        InputEffect::Back => ThemeEffect::Navigate(Screen::Menu),
    }
}

#[inline(always)]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

#[inline(always)]
fn not_chosen_alpha(exit_t: f32) -> f32 {
    if exit_t <= CHOICE_NOT_CHOSEN_FADE_DELAY {
        return 1.0;
    }
    let t = (exit_t - CHOICE_NOT_CHOSEN_FADE_DELAY) / CHOICE_NOT_CHOSEN_FADE_DURATION;
    1.0 - smoothstep(t)
}

#[inline(always)]
fn exit_anim_t(exiting: bool) -> f32 {
    static STEPS: std::sync::OnceLock<Vec<deadlib_present::anim::Step>> =
        std::sync::OnceLock::new();
    crate::screens::components::shared::transitions::linear_elapsed(
        exiting,
        style_flow::CONFIRM_EXIT_SECONDS,
        &STEPS,
        0x5353544C45584954u64, // "SSTLEXIT"
    )
}

fn push_pad_tiles(
    out: &mut Vec<Actor>,
    base_x: f32,
    base_y: f32,
    zoom: f32,
    alpha_mul: f32,
    used_rgba: [f32; 4],
    unused_rgba: [f32; 4],
) {
    let tile_zoom = widescale(PAD_TILE_ZOOM_4_3, PAD_TILE_ZOOM_16_9) * zoom;
    let tile_step = PAD_TILE_NATIVE_SIZE * tile_zoom;

    for row in 0..3 {
        for col in 0..3 {
            let idx = row * 3 + col;
            let mut tint = if DANCE_PAD_LAYOUT[idx] {
                used_rgba
            } else {
                unused_rgba
            };
            tint[3] *= alpha_mul;

            let x = tile_step.mul_add(col as f32 - 1.0, base_x);
            let y = tile_step.mul_add(row as f32 - 2.0, base_y);

            out.push(act!(sprite("rounded-square.png"):
                xy(x, y):
                setsize(PAD_TILE_NATIVE_SIZE, PAD_TILE_NATIVE_SIZE):
                zoom(tile_zoom):
                diffuse(tint[0], tint[1], tint[2], tint[3])
            ));
        }
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(128);
    let exit_chosen_anim = state.flow.exit_chosen_anim();
    let exit_t = exit_anim_t(exit_chosen_anim);
    let (chosen_p, other_alpha) = if exit_chosen_anim {
        (
            deadlib_present::anim::bouncebegin_p(exit_t / style_flow::CONFIRM_EXIT_SECONDS),
            not_chosen_alpha(exit_t),
        )
    } else {
        (0.0, 1.0)
    };

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    let select_style = tr("ScreenTitles", "SelectStyle");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &select_style,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        visual_policy,
    }));

    select_flow_footer::push(actors, &state.runtime.players, visual_policy);

    let cx = screen_center_x();
    let cy = screen_center_y() + widescale(CHOICE_Y_OFFSET_4_3, CHOICE_Y_OFFSET_16_9);
    let choice_x_off = widescale(CHOICE_X_OFFSET_4_3, CHOICE_X_OFFSET_16_9);
    let dual_pad_off = widescale(PAD_DUAL_OFFSET_4_3, PAD_DUAL_OFFSET_16_9);

    for i in 0..style_flow::CHOICE_COUNT {
        let choice = Choice::from_index(i);
        let x = match choice {
            Choice::Single => cx - choice_x_off,
            Choice::Versus => cx,
            Choice::Double => cx + choice_x_off,
        };
        let (zoom, alpha) = if exit_chosen_anim {
            if i == state.flow.selected_index() {
                (style_flow::CHOICE_ZOOM_FOCUSED * (1.0 - chosen_p), 1.0)
            } else {
                (style_flow::CHOICE_ZOOM_UNFOCUSED, other_alpha)
            }
        } else {
            (state.flow.choice_zoom(i), 1.0)
        };

        match choice {
            Choice::Single => {
                let used = color::decorative_rgba(state.active_color_index);
                push_pad_tiles(actors, x, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
            }
            Choice::Versus => {
                let left = color::decorative_rgba(state.active_color_index - 1);
                let right = color::decorative_rgba(state.active_color_index + 2);
                let off = dual_pad_off * zoom;
                push_pad_tiles(actors, x - off, cy, zoom, alpha, left, PAD_UNUSED_RGBA);
                push_pad_tiles(actors, x + off, cy, zoom, alpha, right, PAD_UNUSED_RGBA);
            }
            Choice::Double => {
                let used = color::decorative_rgba(state.active_color_index + 1);
                let off = dual_pad_off * zoom;
                push_pad_tiles(actors, x - off, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
                push_pad_tiles(actors, x + off, cy, zoom, alpha, used, PAD_UNUSED_RGBA);
            }
        }

        let label_y = 37.0f32.mul_add(zoom, cy);
        actors.push(act!(text:
            align(0.5, 0.0):
            xy(x, label_y):
            zoom(0.5 * zoom):
            z(1):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, alpha):
            font(machine_font_key(visual_policy.machine_font, FontRole::Header)): settext(choice_label(choice)): horizalign(center)
        ));
    }
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(128);
    push_actors(&mut actors, state, Default::default());
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use deadsync_input::VirtualAction;
    use std::time::Instant;

    #[test]
    fn confirm_requests_audio_before_profile_update() {
        let mut state = init(SelectFlowRuntimeView::default());
        let now = Instant::now();
        let effect = handle_input(
            &mut state,
            &InputEvent {
                action: VirtualAction::p1_start,
                input_slot: 0,
                pressed: true,
                source: InputSource::Keyboard,
                timestamp: now,
                timestamp_host_nanos: 0,
                stored_at: now,
                emitted_at: now,
            },
        );
        let ThemeEffect::Batch(effects) = effect else {
            panic!("expected batched confirm effect");
        };
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            effects[1],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::SetPlayStyle(_)
            ))
        ));
    }
}
