//! Song background media and chart-authored visual transitions.

use std::cell::Cell;
use std::sync::{Arc, OnceLock};

use crate::gameplay_stage::ActiveGameplayStage;
use deadlib_assets::present_dsl::cover_sprite;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadlib_render_core::BlendMode;
use deadsync_chart::SongBackgroundChange;
use deadsync_gameplay::song_backgrounds::SongLayer2Flash;

use super::GameplayBackgroundMedia;

macro_rules! act {
    (quad: $($tail:tt)+) => {
        deadlib_present::__act_from_builder!(
            ($($tail)+) deadlib_present::dsl::SpriteBuilder::solid()
        )
    };
}

pub struct GameplayBackground {
    pub media: GameplayBackgroundMedia,
    song_background_key: Option<Arc<str>>,
    previous_background_texture_key: Option<Arc<str>>,
    background_transition: Option<BackgroundTransition>,
    background_transition_expired: Cell<bool>,
    background_transition_start_time: f32,
}

impl GameplayBackground {
    pub fn new(stage: &ActiveGameplayStage) -> Self {
        Self {
            media: Default::default(),
            song_background_key: stage
                .song()
                .background_path
                .as_deref()
                .map(deadlib_assets::media_path_key),
            previous_background_texture_key: None,
            background_transition: None,
            background_transition_expired: Cell::new(false),
            background_transition_start_time: stage.current_music_time_display(),
        }
    }

    pub fn begin_transition(
        &mut self,
        previous_texture_key: Arc<str>,
        transition_name: &str,
        start_time: f32,
    ) {
        let transition = if &*previous_texture_key == "__black" {
            None
        } else {
            BackgroundTransition::from_name(transition_name)
        };
        self.previous_background_texture_key = transition.map(|_| previous_texture_key);
        self.background_transition = transition;
        self.background_transition_expired.set(false);
        self.background_transition_start_time = start_time;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BackgroundTransition {
    CrossFade(f32),
    FadeCenterHorizontal,
    FadeCenterVertical,
    FadeDown,
    FadeLeft,
    FadeRight,
    FadeUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    SlideUp,
}

impl BackgroundTransition {
    const fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("CrossFade_Fastest") {
            Some(Self::CrossFade(0.5))
        } else if name.eq_ignore_ascii_case("CrossFade_Faster") {
            Some(Self::CrossFade(0.75))
        } else if name.eq_ignore_ascii_case("CrossFade") {
            Some(Self::CrossFade(1.0))
        } else if name.eq_ignore_ascii_case("FadeCenterHorizontal") {
            Some(Self::FadeCenterHorizontal)
        } else if name.eq_ignore_ascii_case("FadeCenterVertical") {
            Some(Self::FadeCenterVertical)
        } else if name.eq_ignore_ascii_case("FadeDown") {
            Some(Self::FadeDown)
        } else if name.eq_ignore_ascii_case("FadeLeft") {
            Some(Self::FadeLeft)
        } else if name.eq_ignore_ascii_case("FadeRight") {
            Some(Self::FadeRight)
        } else if name.eq_ignore_ascii_case("FadeUp") {
            Some(Self::FadeUp)
        } else if name.eq_ignore_ascii_case("SlideDown") {
            Some(Self::SlideDown)
        } else if name.eq_ignore_ascii_case("SlideLeft") {
            Some(Self::SlideLeft)
        } else if name.eq_ignore_ascii_case("SlideRight") {
            Some(Self::SlideRight)
        } else if name.eq_ignore_ascii_case("SlideUp") {
            Some(Self::SlideUp)
        } else {
            None
        }
    }

    const fn duration(self) -> f32 {
        match self {
            Self::CrossFade(duration) => duration,
            _ => 1.0,
        }
    }
}

#[inline(always)]
fn white_texture_key() -> Arc<str> {
    static WHITE_TEXTURE_KEY: OnceLock<Arc<str>> = OnceLock::new();
    Arc::clone(WHITE_TEXTURE_KEY.get_or_init(|| Arc::from("__white")))
}

pub fn push_background(
    actors: &mut Vec<Actor>,
    state: &ActiveGameplayStage,
    background: &GameplayBackground,
    fallback: Option<fn(&mut Vec<Actor>, f32)>,
    bg_brightness: f32,
    base_color: deadlib_present::color::Color,
) {
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let bg_brightness = bg_brightness.clamp(0.0, 1.0);

    // Solid base fill behind everything. This is what shows when the song has no
    // background image, and what the song background is dimmed toward as
    // BGBrightness drops on the default path.
    let mut base = cover_sprite(white_texture_key(), cx, cy, sw, sh, 1.0, -101);
    if let Actor::Sprite { tint, .. } = &mut base {
        *tint = base_color.to_rgba();
    }
    actors.push(base);

    push_current_bgchange_media(
        actors,
        state,
        background,
        fallback,
        bg_brightness,
        cx,
        cy,
        sw,
        sh,
    );
    push_bgchange_transition(actors, state, background, bg_brightness, cx, cy, sw, sh);
    // A non-default GameplayBgColor mirrors Chris's Simply Love underlay quad:
    // it covers song art but stays behind the notefield, filters, and HUD.
    push_custom_gameplay_backdrop(actors, base_color);
    push_layer2_bganimations(actors, state);
}

fn bgchange_tint(change: Option<&SongBackgroundChange>, brightness: f32) -> [f32; 4] {
    let color = change.and_then(|change| change.color1).unwrap_or([1.0; 4]);
    [color[0], color[1], color[2], color[3] * brightness]
}

fn bgchange_movie_viz_tint(change: Option<&SongBackgroundChange>, brightness: f32) -> [f32; 4] {
    let color = change.and_then(|change| change.color2).unwrap_or([1.0; 4]);
    [color[0], color[1], color[2], color[3] * brightness]
}

fn background_media_sprite(
    key: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Actor {
    let mut actor = cover_sprite(key, x, y, w, h, 1.0, -100);
    if let Actor::Sprite {
        tint: actor_tint,
        blend: actor_blend,
        ..
    } = &mut actor
    {
        *actor_tint = tint;
        *actor_blend = blend;
    }
    actor
}

fn push_current_bgchange_media(
    actors: &mut Vec<Actor>,
    state: &ActiveGameplayStage,
    background: &GameplayBackground,
    fallback: Option<fn(&mut Vec<Actor>, f32)>,
    bg_brightness: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let change = state.song_backgrounds().active_change();
    if &*background.media.background_texture_key == "__black" {
        if let Some(fallback) = fallback
            && background.song_background_key.is_none()
            && change.is_none()
            && bg_brightness > 0.0
        {
            fallback(actors, bg_brightness);
        }
        return;
    }
    if change.is_some_and(|change| change.effect_is("SongBgWithMovieViz")) {
        if let Some(key) = background.song_background_key.as_ref() {
            actors.push(background_media_sprite(
                Arc::clone(key),
                bgchange_tint(change, bg_brightness),
                BlendMode::Alpha,
                x,
                y,
                w,
                h,
            ));
        }
        actors.push(background_media_sprite(
            background.media.background_texture_key.clone(),
            bgchange_movie_viz_tint(change, bg_brightness),
            BlendMode::Add,
            x,
            y,
            w,
            h,
        ));
    } else {
        actors.push(background_media_sprite(
            background.media.background_texture_key.clone(),
            bgchange_tint(change, bg_brightness),
            BlendMode::Alpha,
            x,
            y,
            w,
            h,
        ));
    }
}

fn push_bgchange_transition(
    actors: &mut Vec<Actor>,
    state: &ActiveGameplayStage,
    background: &GameplayBackground,
    bg_brightness: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let Some(key) = background.previous_background_texture_key.as_ref() else {
        return;
    };
    if &**key == "__black" {
        return;
    }
    let now = state.current_music_time_display();
    let start_time = background.background_transition_start_time;
    let Some((transition, progress)) = background_transition_frame(
        background.background_transition,
        &background.background_transition_expired,
        start_time,
        now,
    ) else {
        return;
    };
    let mut actor = background_media_sprite(
        key.clone(),
        [1.0, 1.0, 1.0, bg_brightness],
        BlendMode::Alpha,
        x,
        y,
        w,
        h,
    );
    apply_bgchange_transition(&mut actor, transition, progress, w, h);
    actors.push(actor);
}

fn background_transition_frame(
    transition: Option<BackgroundTransition>,
    expired: &Cell<bool>,
    start_time: f32,
    now: f32,
) -> Option<(BackgroundTransition, f32)> {
    let current = transition?;
    if expired.get() && now >= start_time + current.duration() {
        return None;
    }
    let progress = ((now - start_time) / current.duration()).clamp(0.0, 1.0);
    if progress >= 1.0 {
        expired.set(true);
        return None;
    }
    expired.set(false);
    Some((current, progress))
}

fn apply_bgchange_transition(
    actor: &mut Actor,
    transition: BackgroundTransition,
    progress: f32,
    screen_w: f32,
    screen_h: f32,
) {
    let Actor::Sprite {
        offset,
        tint,
        cropleft,
        cropright,
        croptop,
        cropbottom,
        fadeleft,
        faderight,
        fadetop,
        fadebottom,
        ..
    } = actor
    else {
        return;
    };
    match transition {
        BackgroundTransition::CrossFade(_) => tint[3] *= 1.0 - progress,
        BackgroundTransition::SlideLeft => {
            offset[0] = screen_w.mul_add(-progress, offset[0]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideRight => {
            offset[0] = screen_w.mul_add(progress, offset[0]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideUp => {
            offset[1] = screen_h.mul_add(-progress, offset[1]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideDown => {
            offset[1] = screen_h.mul_add(progress, offset[1]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::FadeUp => {
            *cropbottom = 1.6f32.mul_add(progress, -0.3);
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeDown => {
            *croptop = 1.6f32.mul_add(progress, -0.3);
            *fadetop = 0.3;
        }
        BackgroundTransition::FadeRight => {
            *cropleft = 1.6f32.mul_add(progress, -0.3);
            *fadeleft = 0.3;
        }
        BackgroundTransition::FadeLeft => {
            *cropright = 1.6f32.mul_add(progress, -0.3);
            *faderight = 0.3;
        }
        BackgroundTransition::FadeCenterHorizontal => {
            *croptop = 0.8f32.mul_add(progress, -0.3);
            *cropbottom = 0.8f32.mul_add(progress, -0.3);
            *fadetop = 0.3;
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeCenterVertical => {
            *cropleft = 0.8f32.mul_add(progress, -0.3);
            *cropright = 0.8f32.mul_add(progress, -0.3);
            *fadeleft = 0.3;
            *faderight = 0.3;
        }
    }
}

const LAYER2_FLASH_SECONDS: f32 = 0.6;

fn layer2_flash_color(flash: SongLayer2Flash, elapsed: f32) -> Option<[f32; 4]> {
    if !(0.0..=LAYER2_FLASH_SECONDS).contains(&elapsed) {
        return None;
    }
    let progress = (elapsed / LAYER2_FLASH_SECONDS).clamp(0.0, 1.0);
    let blue = match flash {
        SongLayer2Flash::White => 1.0,
        SongLayer2Flash::Yellow => 160.0 / 255.0,
    };
    Some([1.0, 1.0, blue, 1.0 - progress * progress])
}

fn push_layer2_bganimations(actors: &mut Vec<Actor>, state: &ActiveGameplayStage) {
    let Some(color) = state
        .song_backgrounds()
        .active_layer2_flash(state.current_music_time_display())
        .and_then(|(flash, elapsed)| layer2_flash_color(flash, elapsed))
    else {
        return;
    };
    actors.push(act!(quad:
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        setsize(screen_width() * 2.0, screen_height() * 2.0):
        diffuse(color[0], color[1], color[2], color[3]):
        z(-98)
    ));
}

pub fn custom_gameplay_backdrop_enabled(color: deadlib_present::color::Color) -> bool {
    color != deadlib_present::color::Color::BLACK
}

pub fn push_custom_gameplay_backdrop(
    actors: &mut Vec<Actor>,
    color: deadlib_present::color::Color,
) {
    if !custom_gameplay_backdrop_enabled(color) {
        return;
    }
    let rgba = color.to_rgba();
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        setsize(screen_width(), screen_height()):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(-99)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer2_flash_keeps_its_tint_and_fade() {
        assert_eq!(
            layer2_flash_color(SongLayer2Flash::White, 0.0),
            Some([1.0; 4])
        );
        assert_eq!(
            layer2_flash_color(SongLayer2Flash::Yellow, LAYER2_FLASH_SECONDS * 0.5),
            Some([1.0, 1.0, 160.0 / 255.0, 0.75]),
        );
        assert_eq!(layer2_flash_color(SongLayer2Flash::White, 1.0), None);
    }
}
