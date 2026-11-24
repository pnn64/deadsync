//! Presentation for library loading and the default intro.

use crate::app_requests::AppEffect;

pub trait StartupPresentation<Actor, Progress, Policy> {
    fn reset_intro(&mut self, color_index: i32);
    fn advance_intro(&mut self, delta_time: f32) -> AppEffect;
    fn push_loading(&self, actors: &mut Vec<Actor>, progress: Option<&Progress>, policy: &Policy);
    fn push_intro(&self, actors: &mut Vec<Actor>, policy: &Policy);
}
