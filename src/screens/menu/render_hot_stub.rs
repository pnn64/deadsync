//! Host-side placeholder for the menu render path, used only under
//! `feature = "hot"`.
//!
//! The real renderer lives in [`render.rs`](super) (`get_actors`). Under the
//! `hot` feature that file is **excluded from the engine rlib** (see
//! `menu/mod.rs`) so that editing it recompiles *only* the small
//! `deadsync-screens` cdylib — a subsecond relink — instead of dirtying the
//! whole engine rlib (~a minute). The host therefore has no compiled-in menu
//! renderer; it dispatches through the hot-loaded cdylib instead.
//!
//! This stub supplies the `get_actors` symbol the engine still references so the
//! host links and runs. It is the fallback shown **before the cdylib has loaded**
//! and **after a quarantined panic / rejected ABI**, where it renders nothing
//! (the screen clear color) until a valid `deadsync_screens` library is swapped
//! in. (Render-cache invalidation now lives host-side in `menu/mod.rs`, shared by
//! both renderers, so it is not duplicated here.)

use crate::engine::present::actors::Actor;
use crate::screens::menu::state::{HostContext, State};

/// Placeholder renderer: emits no actors. The hot-loaded cdylib provides the
/// real `get_actors`; this is only reached before the first successful load or
/// while a panicking generation is quarantined.
pub fn get_actors(_state: &State, _ctx: &HostContext, _alpha: f32) -> Vec<Actor> {
    Vec::new()
}
