//! App adapter for the reusable SMX gameplay panel-light driver.
//!
//! The SMX crate owns the per-column gameplay diff and worker state. The app
//! keeps session/profile lookup here so the crate does not need to know about
//! active profile mutexes.

use std::sync::Arc;

use deadsync_profile::compat as profile;
use deadsync_profile::physical_player_slot_for_chart_pad;
use deadsync_smx::gameplay_driver as smx_driver;
use deadsync_smx::gifs::FullPadAnim;
use deadsync_smx::panel_fx::JudgementGifs;
use deadsync_smx::panels::{Clock, PADS};

use crate::GameplayCoreState;

pub struct SmxPanelDriver {
    inner: smx_driver::SmxPanelDriver,
}

impl Default for SmxPanelDriver {
    fn default() -> Self {
        Self {
            inner: smx_driver::SmxPanelDriver::default(),
        }
    }
}

impl SmxPanelDriver {
    pub fn update(&mut self, state: &GameplayCoreState) {
        self.inner.update(state, smx_slot_for_gameplay(state));
    }

    pub fn deactivate(&mut self) {
        self.inner.deactivate();
    }

    pub fn set_background_for_pad(
        &mut self,
        pad: usize,
        background: Option<(Arc<FullPadAnim>, Clock)>,
    ) {
        self.inner.set_background_for_pad(pad, background);
    }

    pub fn set_judgement_gifs_for_pad(&mut self, pad: usize, gifs: JudgementGifs) {
        self.inner.set_judgement_gifs_for_pad(pad, gifs);
    }

    pub fn set_pad_blackout(&self, pad: usize, on: bool) {
        self.inner.set_pad_blackout(pad, on);
    }

    pub fn on_raw_panel(&self, pad: usize, panel: usize, pressed: bool) {
        self.inner.on_raw_panel(pad, panel, pressed);
    }

    pub fn set_beat(&self, beat: f32) {
        self.inner.set_beat(beat);
    }
}

fn smx_slot_for_gameplay(state: &GameplayCoreState) -> [usize; PADS] {
    let play_style = profile::get_session_play_style();
    let session_side = profile::get_session_player_side();
    let doubles = state.cols_per_player() >= 8 && state.num_players() == 1;
    std::array::from_fn(|pad| {
        physical_player_slot_for_chart_pad(play_style, session_side, doubles, pad)
    })
}
