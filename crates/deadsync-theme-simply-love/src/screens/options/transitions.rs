use super::*;

use crate::screens::components::shared::transitions as shared_transitions;

pub fn in_transition() -> (Vec<Actor>, f32) {
    shared_transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    shared_transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}
