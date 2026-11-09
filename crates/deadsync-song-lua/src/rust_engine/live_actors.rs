//! The live actor phase; the scene coordinator owns layer scheduling and clocks.
use super::*;

impl RustSongRuntime {
    /// Advance the production screen/player/song/transition job order. Call after
    /// publishing the live frame. `before_enter` is an optional host callback
    /// receiving each actor before its incoming delta is selected; the scene
    /// coordinator uses it to activate scheduled foregrounds in traversal order.
    /// Handles passed to that callback are traced throughout the current plan,
    /// including when a callback invalidates the plan for the following frame.
    /// This phase does not advance clocks, schedule backgrounds, publish health,
    /// dispatch input, draw, or drain live output.
    pub fn advance_song_live_actors(
        &mut self,
        screen: ActorScene,
        delta_seconds: f64,
        before_enter: Option<Value>,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        self.actors.advance_live(
            &mut self.vm,
            screen,
            delta_seconds,
            before_enter,
            self.statistics.clone(),
            self.catalog,
        )
    }
}
