use crate::act;
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_height, screen_width};

const SLEEP_SECONDS: f32 = 0.6;
const FADE_SECONDS: f32 = 0.5;
pub(crate) const TOTAL_SECONDS: f32 = SLEEP_SECONDS + FADE_SECONDS;

pub(crate) fn push(actors: &mut Vec<Actor>, selection_animation_timer: f32) {
    if selection_animation_timer > TOTAL_SECONDS {
        return;
    }
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(-98):
        sleep(SLEEP_SECONDS):
        linear(FADE_SECONDS): alpha(0.0):
        linear(0.0): visible(false)
    ));
}
