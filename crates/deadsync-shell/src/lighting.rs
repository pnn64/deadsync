//! Shell policy and adapters for cabinet and StepManiaX lighting.

use std::sync::Arc;

use crate::smx_config::smx_options_light_preview_active;
use deadsync_config::options::SmxPackName;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_input::VirtualAction;
use deadsync_lights::cabinet_chart::{
    CabinetLightEvent, CabinetLightPlan, GameplayLightChartKey, cabinet_light_chart_from_loaded,
};
use deadsync_lights::{ButtonLight, HideFlags, Player, ScreenLightContext};
use deadsync_profile::compat as profile;
use deadsync_profile::{
    HideLightType, PlayStyle, PlayerSide, physical_player_slot_for_chart_pad, player_side_index,
};
use deadsync_score::Grade;
use deadsync_simfile::app_runtime as song_loading;
use deadsync_smx::gameplay_driver as smx_driver;
use deadsync_smx::gifs::FullPadAnim;
use deadsync_smx::panel_fx::JudgementGifs;
use deadsync_smx::panels::{Clock, PADS};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::views::ScoreInfo;

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

pub fn smx_background_role(
    screen: Screen,
    enabled: bool,
    assignment_preview: bool,
) -> Option<&'static str> {
    if enabled && !assignment_preview {
        deadsync_lights::screen_smx_background_role(screen_light_context(screen))
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmxResultContext {
    pub grade: Option<Grade>,
    pub difficulty: Option<&'static str>,
}

impl SmxResultContext {
    #[inline(always)]
    pub fn grade_sprite_state(self) -> Option<u32> {
        self.grade.map(|grade| grade.to_sprite_state())
    }
}

pub fn smx_result_context(screen: Screen, score_info: &[Option<ScoreInfo>; 2]) -> SmxResultContext {
    if !matches!(
        screen,
        Screen::Evaluation | Screen::EvaluationSummary | Screen::Initials
    ) {
        return SmxResultContext::default();
    }
    score_info
        .iter()
        .flatten()
        .map(|score| SmxResultContext {
            grade: Some(score.grade),
            difficulty: Some(deadlib_present::color::difficulty_gif_tag(
                &score.chart.difficulty,
            )),
        })
        .min_by_key(|context| context.grade_sprite_state())
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayLightSyncTarget {
    Gameplay,
    Practice,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightingFramePlan {
    pub screen_mode: Option<ScreenLightContext>,
    pub smx_panels_enabled: bool,
    pub smx_select_music_beat: bool,
    pub gameplay_target: GameplayLightSyncTarget,
}

pub const fn lighting_frame_plan(
    screen: Screen,
    smx_input: bool,
    smx_panel_lights: bool,
) -> LightingFramePlan {
    let smx_panels_enabled = smx_input && smx_panel_lights;
    LightingFramePlan {
        screen_mode: lighting_screen_mode(screen),
        smx_panels_enabled,
        smx_select_music_beat: matches!(screen, Screen::SelectMusic) && smx_panels_enabled,
        gameplay_target: gameplay_light_sync_target(screen),
    }
}

pub const fn lighting_screen_mode(screen: Screen) -> Option<ScreenLightContext> {
    if matches!(screen, Screen::TestLights) {
        None
    } else {
        Some(screen_light_context(screen))
    }
}

pub const fn gameplay_light_sync_target(screen: Screen) -> GameplayLightSyncTarget {
    match screen {
        Screen::Gameplay => GameplayLightSyncTarget::Gameplay,
        Screen::Practice => GameplayLightSyncTarget::Practice,
        _ => GameplayLightSyncTarget::Clear,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxPadGifFramePlan {
    pub assignment_preview: bool,
    pub role: Option<&'static str>,
    pub current_song_needed: bool,
    pub result_context: SmxResultContext,
    pub beat_locked: bool,
}

pub fn smx_pad_gif_frame_plan(
    screen: Screen,
    enabled: bool,
    smx_input: bool,
    smx_config_view: bool,
    score_info: &[Option<ScoreInfo>; 2],
) -> SmxPadGifFramePlan {
    let assignment_preview = smx_options_light_preview_active(screen, smx_input, smx_config_view);
    let role = smx_background_role(screen, enabled, assignment_preview);
    SmxPadGifFramePlan {
        assignment_preview,
        role,
        current_song_needed: role.is_some(),
        result_context: smx_result_context(screen, score_info),
        beat_locked: enabled && matches!(screen, Screen::SelectMusic),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorMenuButtonRoute {
    Ignore,
    ConsumeLocked,
    NavigateOptions,
}

pub const fn operator_menu_button_route(
    screen: Screen,
    pressed: bool,
    action: VirtualAction,
) -> OperatorMenuButtonRoute {
    if !pressed || !deadsync_lights::operator_menu_action(action) {
        return OperatorMenuButtonRoute::Ignore;
    }
    if deadsync_lights::screen_allows_operator_menu_button(screen_light_context(screen)) {
        OperatorMenuButtonRoute::NavigateOptions
    } else {
        OperatorMenuButtonRoute::ConsumeLocked
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightInputRoute {
    Ignore,
    Pad {
        player: Player,
        button: ButtonLight,
        pressed: bool,
    },
    Menu {
        player: Player,
        button: ButtonLight,
        pressed: bool,
    },
}

pub const fn light_input_route(action: VirtualAction, pressed: bool) -> LightInputRoute {
    match deadsync_lights::button_source_from_action(action) {
        Some(deadsync_lights::ButtonSource::Pad(player, button)) => LightInputRoute::Pad {
            player,
            button,
            pressed,
        },
        Some(deadsync_lights::ButtonSource::Menu(player, button)) => LightInputRoute::Menu {
            player,
            button,
            pressed,
        },
        None => LightInputRoute::Ignore,
    }
}

pub const fn smx_pad_blackout(
    screen: Screen,
    enabled: bool,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [bool; 2] {
    let in_game = enabled
        && !matches!(
            screen,
            Screen::Menu
                | Screen::Init
                | Screen::SmxAssignPads
                | Screen::TestLights
                | Screen::ManageLocalProfiles
                | Screen::Credits
                | Screen::OverscanAdjustment
                | Screen::Mappings
                | Screen::Options
                | Screen::PlayerOptions
                | Screen::ConfigurePads
                | Screen::Input
        );
    if in_game && matches!(play_style, PlayStyle::Single) {
        let used = player_side_index(player_side);
        [used != 0, used != 1]
    } else {
        [false; 2]
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

    #[test]
    fn smx_role_respects_feature_and_assignment_preview() {
        assert_eq!(
            smx_background_role(Screen::SelectMusic, true, false),
            Some("song_select")
        );
        assert_eq!(smx_background_role(Screen::SelectMusic, false, false), None);
        assert_eq!(smx_background_role(Screen::Options, true, true), None);
    }

    #[test]
    fn lighting_frame_plan_keeps_test_lights_in_control() {
        let gameplay = lighting_frame_plan(Screen::Gameplay, true, true);
        assert_eq!(gameplay.screen_mode, Some(ScreenLightContext::Gameplay));
        assert_eq!(gameplay.gameplay_target, GameplayLightSyncTarget::Gameplay);
        assert!(gameplay.smx_panels_enabled);
        assert!(!gameplay.smx_select_music_beat);

        let test_lights = lighting_frame_plan(Screen::TestLights, true, true);
        assert_eq!(test_lights.screen_mode, None);
        assert_eq!(test_lights.gameplay_target, GameplayLightSyncTarget::Clear);
        assert!(test_lights.smx_panels_enabled);
    }

    #[test]
    fn smx_pad_gif_plan_suppresses_assignment_preview_backgrounds() {
        let score_info = [None, None];
        let select = smx_pad_gif_frame_plan(Screen::SelectMusic, true, true, false, &score_info);
        assert_eq!(select.role, Some("song_select"));
        assert!(select.current_song_needed);
        assert!(select.beat_locked);

        let options = smx_pad_gif_frame_plan(Screen::Options, true, true, true, &score_info);
        assert!(options.assignment_preview);
        assert_eq!(options.role, None);
        assert!(!options.current_song_needed);
        assert!(!options.beat_locked);
    }

    #[test]
    fn operator_menu_button_policy_consumes_locked_screens() {
        assert_eq!(
            operator_menu_button_route(Screen::Menu, true, VirtualAction::p1_operator),
            OperatorMenuButtonRoute::NavigateOptions,
        );
        assert_eq!(
            operator_menu_button_route(Screen::Mappings, true, VirtualAction::p1_operator),
            OperatorMenuButtonRoute::ConsumeLocked,
        );
        assert_eq!(
            operator_menu_button_route(Screen::Menu, false, VirtualAction::p1_operator),
            OperatorMenuButtonRoute::Ignore,
        );
        assert_eq!(
            operator_menu_button_route(Screen::Menu, true, VirtualAction::p1_start),
            OperatorMenuButtonRoute::Ignore,
        );
    }

    #[test]
    fn light_input_route_preserves_source_and_pressed_state() {
        assert_eq!(
            light_input_route(VirtualAction::p1_left, true),
            LightInputRoute::Pad {
                player: Player::P1,
                button: ButtonLight::Left,
                pressed: true,
            },
        );
        assert_eq!(
            light_input_route(VirtualAction::p2_menu_right, false),
            LightInputRoute::Menu {
                player: Player::P2,
                button: ButtonLight::Right,
                pressed: false,
            },
        );
        assert_eq!(
            light_input_route(VirtualAction::system_fast_forward, true),
            LightInputRoute::Ignore,
        );
    }

    #[test]
    fn single_play_blacks_out_only_the_unused_game_slot() {
        assert_eq!(
            smx_pad_blackout(Screen::Gameplay, true, PlayStyle::Single, PlayerSide::P1),
            [false, true],
        );
        assert_eq!(
            smx_pad_blackout(Screen::SelectMusic, true, PlayStyle::Single, PlayerSide::P2),
            [true, false],
        );
        assert_eq!(
            smx_pad_blackout(Screen::Gameplay, true, PlayStyle::Double, PlayerSide::P1),
            [false, false],
        );
        assert_eq!(
            smx_pad_blackout(Screen::Options, true, PlayStyle::Single, PlayerSide::P1),
            [false, false],
        );
    }

    #[test]
    fn empty_result_context_has_no_grade_or_difficulty() {
        assert_eq!(
            smx_result_context(Screen::Evaluation, &[None, None]),
            SmxResultContext::default(),
        );
        assert_eq!(
            smx_result_context(Screen::Gameplay, &[None, None]),
            SmxResultContext::default(),
        );
    }
}
