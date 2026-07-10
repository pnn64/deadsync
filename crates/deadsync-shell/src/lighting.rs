//! Shell policy and adapters for cabinet and StepManiaX lighting.

use std::sync::Arc;

use deadsync_config::options::SmxPackName;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_lights::cabinet_chart::{
    CabinetLightEvent, CabinetLightPlan, GameplayLightChartKey, cabinet_light_chart_from_loaded,
};
use deadsync_lights::{HideFlags, ScreenLightContext};
use deadsync_profile::compat as profile;
use deadsync_profile::{HideLightType, physical_player_slot_for_chart_pad};
use deadsync_screens::Screen;
use deadsync_simfile::app_runtime as song_loading;
use deadsync_smx::gameplay_driver as smx_driver;
use deadsync_smx::gifs::FullPadAnim;
use deadsync_smx::panel_fx::JudgementGifs;
use deadsync_smx::panels::{Clock, PADS};

/// Load and compile the cabinet-light chart requested by a gameplay plan.
pub fn load_cabinet_light_chart(
    song: &deadsync_chart::SongData,
    plan: &CabinetLightPlan,
    global_offset_seconds: f32,
    pack_sync_offset_seconds: f32,
) -> Result<(GameplayLightChartKey, Vec<CabinetLightEvent>), String> {
    let charts =
        song_loading::load_gameplay_charts(song, &plan.request_chart_ixs(), global_offset_seconds)?;
    Ok(cabinet_light_chart_from_loaded(
        song,
        plan,
        &charts,
        global_offset_seconds,
        pack_sync_offset_seconds,
    ))
}

/// Translate screen identity into the lighting mode understood by the hardware layer.
pub const fn screen_light_context(screen: Screen) -> ScreenLightContext {
    match screen {
        Screen::Init => ScreenLightContext::Init,
        Screen::Gameplay | Screen::Practice => ScreenLightContext::Gameplay,
        Screen::TestLights => ScreenLightContext::TestLights,
        Screen::OverscanAdjustment => ScreenLightContext::OverscanAdjustment,
        Screen::Evaluation | Screen::EvaluationSummary | Screen::Initials => {
            ScreenLightContext::Results
        }
        Screen::GameOver => ScreenLightContext::GameOver,
        Screen::Options => ScreenLightContext::Options,
        Screen::Mappings | Screen::Input => ScreenLightContext::OperatorLocked,
        Screen::SmxAssignPads => ScreenLightContext::SmxAssignPads,
        Screen::SelectMusic | Screen::SelectCourse => ScreenLightContext::SongSelect,
        Screen::Menu
        | Screen::Credits
        | Screen::ManageLocalProfiles
        | Screen::SelectProfile
        | Screen::ArrowCloudLogin
        | Screen::GrooveStatsLogin
        | Screen::SelectColor
        | Screen::SelectStyle
        | Screen::SelectPlayMode
        | Screen::ProfileLoad
        | Screen::Sandbox
        | Screen::PlayerOptions
        | Screen::ConfigurePads => ScreenLightContext::Menu,
    }
}

/// Map both gameplay profiles' hide-light choices into hardware flags.
pub const fn hide_flags_for_profiles(hide: [HideLightType; 2]) -> [HideFlags; 2] {
    [
        hide_flags_from_profile(hide[0]),
        hide_flags_from_profile(hide[1]),
    ]
}

pub const fn hide_flags_from_profile(hide: HideLightType) -> HideFlags {
    match hide {
        HideLightType::NoHideLights => HideFlags {
            all: false,
            marquee: false,
            bass: false,
        },
        HideLightType::HideAllLights => HideFlags {
            all: true,
            marquee: true,
            bass: true,
        },
        HideLightType::HideMarqueeLights => HideFlags {
            all: false,
            marquee: true,
            bass: false,
        },
        HideLightType::HideBassLights => HideFlags {
            all: false,
            marquee: false,
            bass: true,
        },
    }
}

/// Dedup key for SMX background and judgment-animation synchronization.
#[derive(Clone, Copy, PartialEq)]
pub struct SmxAnimationSyncKey {
    enabled: bool,
    role: Option<&'static str>,
    bg_packs: [SmxPackName; 2],
    judge_packs: [SmxPackName; 2],
    song_id: Option<usize>,
    eval_grade: Option<u32>,
    eval_difficulty: Option<&'static str>,
}

impl SmxAnimationSyncKey {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        enabled: bool,
        role: Option<&'static str>,
        bg_packs: [SmxPackName; 2],
        judge_packs: [SmxPackName; 2],
        song_id: Option<usize>,
        eval_grade: Option<u32>,
        eval_difficulty: Option<&'static str>,
    ) -> Self {
        Self {
            enabled,
            role,
            bg_packs,
            judge_packs,
            song_id,
            eval_grade,
            eval_difficulty,
        }
    }

    /// Whether animation registries must be refreshed rather than only role/song selection.
    pub fn packs_changed(self, next: Self) -> bool {
        self.enabled != next.enabled
            || self.bg_packs != next.bg_packs
            || self.judge_packs != next.judge_packs
    }
}

/// App-facing adapter around the reusable SMX gameplay panel-light driver.
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
    pub fn update<Profile, OverlayActor, CapturedActor, StateDelta>(
        &mut self,
        state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    ) where
        Profile: GameplayProfileData,
    {
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

fn smx_slot_for_gameplay<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
) -> [usize; PADS]
where
    Profile: GameplayProfileData,
{
    let play_style = profile::get_session_play_style();
    let session_side = profile::get_session_player_side();
    let doubles = state.cols_per_player() >= 8 && state.num_players() == 1;
    std::array::from_fn(|pad| {
        physical_player_slot_for_chart_pad(play_style, session_side, doubles, pad)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_policy_covers_gameplay_results_and_operator_modes() {
        assert_eq!(
            screen_light_context(Screen::Gameplay),
            ScreenLightContext::Gameplay
        );
        assert_eq!(
            screen_light_context(Screen::Practice),
            ScreenLightContext::Gameplay
        );
        assert_eq!(
            screen_light_context(Screen::Evaluation),
            ScreenLightContext::Results
        );
        assert_eq!(
            screen_light_context(Screen::Mappings),
            ScreenLightContext::OperatorLocked
        );
        assert_eq!(
            screen_light_context(Screen::SelectMusic),
            ScreenLightContext::SongSelect
        );
        assert_eq!(screen_light_context(Screen::Menu), ScreenLightContext::Menu);
    }

    #[test]
    fn profile_hide_choices_map_to_independent_flags() {
        let [p1, p2] = hide_flags_for_profiles([
            HideLightType::HideMarqueeLights,
            HideLightType::HideBassLights,
        ]);
        assert!(!p1.all && p1.marquee && !p1.bass);
        assert!(!p2.all && !p2.marquee && p2.bass);
        assert!(hide_flags_from_profile(HideLightType::HideAllLights).all);
        assert!(!hide_flags_from_profile(HideLightType::NoHideLights).all);
    }

    #[test]
    fn sync_key_separates_pack_refreshes_from_role_changes() {
        let packs = [SmxPackName::parse("common"), SmxPackName::default()];
        let base = SmxAnimationSyncKey::new(true, Some("menu"), packs, packs, None, None, None);
        let role = SmxAnimationSyncKey::new(true, Some("results"), packs, packs, None, None, None);
        assert!(!base.packs_changed(role));

        let changed = SmxAnimationSyncKey::new(
            true,
            Some("menu"),
            [SmxPackName::parse("other"), SmxPackName::default()],
            packs,
            None,
            None,
            None,
        );
        assert!(base.packs_changed(changed));
    }
}
