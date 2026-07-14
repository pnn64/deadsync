use deadsync_config::prelude::SmxPadPreset;
use deadsync_profile::pad_config;
use deadsync_profile::pad_config_sync::AppliedPadConfig;
use deadsync_smx::SmxInfo;
use deadsync_theme::views::{SmxAssignmentPadView, SmxAssignmentView, SmxGifCatalogView};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmxAssignmentSource {
    DistinctJumpers,
    SingleP1,
    SingleP2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmxAssignmentPlan {
    pub p1_serial: Option<String>,
    pub p2_serial: Option<String>,
    pub source: SmxAssignmentSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmxAutopromptPlan {
    pub latch: bool,
    pub unlatch: bool,
    pub navigate_to_assign: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxLightBrightnessPlan {
    pub resolved: [u8; 2],
    pub apply: bool,
}

pub fn smx_assignment_view() -> SmxAssignmentView {
    let manager = deadsync_smx::manager();
    let (saved_p1, saved_p2) = deadsync_config::prelude::smx_pad_assignment();
    SmxAssignmentView {
        pads: std::array::from_fn(|slot| {
            let info = deadsync_smx::get_info(slot);
            let label = if info.connected && !info.serial.is_empty() {
                format!("SMX[{}]", deadsync_smx::serial_prefix(info.serial.as_str()))
            } else {
                String::new()
            };
            SmxAssignmentPadView {
                connected: info.connected,
                serial: info.serial,
                label,
                input_state: manager.map_or(0, |manager| manager.get_input_state(slot)),
                backend_id: deadsync_smx::BACKEND_ID.to_owned(),
                pad_type: deadsync_smx::pad_sensor_type(slot)
                    .map(|pad_type| pad_type.as_str().to_owned()),
            }
        }),
        can_swap: saved_p1.is_some() && saved_p2.is_some(),
        conflict_warning: deadsync_smx::conflict_warning_active(),
        conflict_rgb: deadsync_smx::CONFLICT_WARNING_RGB,
        player_rgb: [deadsync_smx::PLAYER1_LIGHT, deadsync_smx::PLAYER2_LIGHT],
    }
}

pub fn smx_gif_catalog_view() -> SmxGifCatalogView {
    let root = deadlib_platform::dirs::app_dirs().resolve_asset_path("assets");
    SmxGifCatalogView {
        background_packs: deadsync_smx::gifs::discover_packs(&root.join("smx-pad-lights")),
        judgment_packs: deadsync_smx::gifs::discover_packs(&root.join("smx-judge-lights")),
    }
}

pub fn apply_smx_underglow() {
    let lone_pad = deadsync_smx::get_info(0).connected ^ deadsync_smx::get_info(1).connected;
    if let Some(plan) = deadsync_config::runtime_state::smx_underglow_colors_from_config(
        &deadsync_config::prelude::get(),
        lone_pad,
    ) {
        deadsync_smx::set_platform_lights_grb(plan.grb);
        deadsync_smx::set_platform_lights_solid(plan.colors);
    }
}

pub fn set_smx_underglow_theme(enabled: bool) {
    if deadsync_config::prelude::update_smx_underglow_theme(enabled) && enabled {
        apply_smx_underglow();
    }
}

pub fn set_smx_underglow_grb(grb: bool) {
    if !deadsync_config::prelude::update_smx_underglow_grb(grb) {
        return;
    }
    deadsync_smx::set_platform_lights_grb(grb);
    apply_smx_underglow();
}

pub fn set_theme_color(index: i32) {
    if deadsync_config::prelude::update_simply_love_color(index) {
        apply_smx_underglow();
    }
}

pub fn set_smx_assignment(p1_serial: Option<String>, p2_serial: Option<String>) {
    if deadsync_config::prelude::update_smx_pad_assignment(p1_serial.clone(), p2_serial.clone()) {
        deadsync_smx::set_player_assignment(p1_serial, p2_serial);
    }
}

pub fn swap_smx_assignment() -> bool {
    let [slot0, slot1] = deadsync_smx::connected_serials();
    let (Some(p1), Some(p2)) = (slot0, slot1) else {
        return false;
    };
    set_smx_assignment(Some(p2), Some(p1));
    true
}

pub fn apply_smx_pad_preset(pad: usize, name: &str) -> bool {
    if pad >= 2 || !deadsync_smx::get_info(pad).connected {
        return false;
    }
    name.parse::<SmxPadPreset>()
        .is_ok_and(|preset| deadsync_smx::apply_preset(pad, preset))
}

pub fn apply_smx_saved_pad_config(pad: usize, profile_id: &str, name: &str) -> bool {
    if pad >= 2 || !deadsync_smx::get_info(pad).connected {
        return false;
    }
    let configs = deadsync_profile::compat::load_pad_configs(profile_id);
    configs
        .iter()
        .find(|config| config.name == name)
        .and_then(|config| deadsync_smx::PadConfigData::from_settings(&config.settings))
        .is_some_and(|data| deadsync_smx::apply_config_data(pad, &data))
}

pub fn capture_smx_pad_config(pad: usize, profile_id: &str, name: &str, set_default: bool) -> bool {
    if pad >= 2 {
        return false;
    }
    let info = deadsync_smx::get_info(pad);
    if !info.connected {
        return false;
    }
    let Some(data) = deadsync_smx::capture_config(pad) else {
        return false;
    };
    let pad_type = deadsync_smx::pad_sensor_type(pad).map(|kind| kind.as_str().to_owned());
    deadsync_profile::compat::upsert_pad_config(
        profile_id,
        name,
        deadsync_smx::BACKEND_ID,
        pad_type,
        Some(info.serial),
        set_default,
        data.to_settings(),
    );
    true
}

pub fn smx_assignment_plan(
    current_p1: Option<&str>,
    current_p2: Option<&str>,
    slot0: &SmxInfo,
    slot1: &SmxInfo,
) -> Option<SmxAssignmentPlan> {
    if current_p1.is_some() || current_p2.is_some() {
        return None;
    }
    if let Some((p1, p2)) = deadsync_smx::jumper_derived_pair(slot0, slot1) {
        return Some(SmxAssignmentPlan {
            p1_serial: Some(p1),
            p2_serial: Some(p2),
            source: SmxAssignmentSource::DistinctJumpers,
        });
    }

    let single = match (slot0.connected, slot1.connected) {
        (true, false) if slot0.has_serial_number && !slot0.serial.is_empty() => Some(slot0),
        (false, true) if slot1.has_serial_number && !slot1.serial.is_empty() => Some(slot1),
        _ => None,
    }?;
    let serial = Some(single.serial.clone());
    if single.is_player2 {
        Some(SmxAssignmentPlan {
            p1_serial: None,
            p2_serial: serial,
            source: SmxAssignmentSource::SingleP2,
        })
    } else {
        Some(SmxAssignmentPlan {
            p1_serial: serial,
            p2_serial: None,
            source: SmxAssignmentSource::SingleP1,
        })
    }
}

pub fn smx_runtime_assignment_plan(
    screen: Screen,
    smx_input: bool,
    current_p1: Option<&str>,
    current_p2: Option<&str>,
    slot0: &SmxInfo,
    slot1: &SmxInfo,
) -> Option<SmxAssignmentPlan> {
    if matches!(screen, Screen::Gameplay | Screen::Practice) || !smx_input {
        return None;
    }
    smx_assignment_plan(current_p1, current_p2, slot0, slot1)
}

pub const fn smx_autoprompt_plan(
    screen: Screen,
    transition_idle: bool,
    smx_input: bool,
    conflict_active: bool,
    latched: bool,
) -> SmxAutopromptPlan {
    if !matches!(screen, Screen::Menu) || !transition_idle {
        return SmxAutopromptPlan {
            latch: false,
            unlatch: false,
            navigate_to_assign: false,
        };
    }
    if !(smx_input && conflict_active) {
        return SmxAutopromptPlan {
            latch: false,
            unlatch: true,
            navigate_to_assign: false,
        };
    }
    if latched {
        return SmxAutopromptPlan {
            latch: false,
            unlatch: false,
            navigate_to_assign: false,
        };
    }
    SmxAutopromptPlan {
        latch: true,
        unlatch: false,
        navigate_to_assign: true,
    }
}

#[inline(always)]
pub const fn smx_options_light_preview_active(
    screen: Screen,
    smx_input: bool,
    smx_config_view: bool,
) -> bool {
    matches!(screen, Screen::Options) && smx_input && smx_config_view
}

#[inline(always)]
pub const fn smx_player_options_light_preview_allowed(screen: Screen, smx_input: bool) -> bool {
    matches!(screen, Screen::PlayerOptions) && smx_input
}

#[inline(always)]
pub const fn smx_light_preview_restore_auto(screen: Screen) -> bool {
    !matches!(screen, Screen::Gameplay | Screen::Practice)
}

pub const fn smx_light_brightness_plan(
    screen: Screen,
    smx_input: bool,
    current: [u8; 2],
    profile_resolved: [u8; 2],
) -> Option<SmxLightBrightnessPlan> {
    if matches!(screen, Screen::Gameplay | Screen::Practice) {
        return None;
    }
    let resolved = if smx_input {
        profile_resolved
    } else {
        [100, 100]
    };
    Some(SmxLightBrightnessPlan {
        resolved,
        apply: resolved[0] != current[0] || resolved[1] != current[1],
    })
}

pub fn resolve_smx_pad_config(
    pad: usize,
    profile_id: Option<&str>,
    pad_type: Option<&str>,
    serial: &str,
    preset: SmxPadPreset,
) -> (bool, AppliedPadConfig) {
    let preset_label = AppliedPadConfig {
        preset: true,
        name: preset.as_str().to_owned(),
    };
    let Some(profile_id) = profile_id else {
        return (deadsync_smx::apply_preset(pad, preset), preset_label);
    };
    let configs = deadsync_profile::compat::load_pad_configs(profile_id);
    match pad_config::resolve(&configs, deadsync_smx::BACKEND_ID, pad_type, serial).and_then(
        |config| {
            deadsync_smx::PadConfigData::from_settings(&config.settings)
                .map(|data| (config.name.clone(), data))
        },
    ) {
        Some((name, data)) => (
            deadsync_smx::apply_config_data(pad, &data),
            AppliedPadConfig {
                preset: false,
                name,
            },
        ),
        None => (deadsync_smx::apply_preset(pad, preset), preset_label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(connected: bool, is_player2: bool, has_serial: bool, serial: &str) -> SmxInfo {
        SmxInfo {
            connected,
            is_player2,
            has_serial_number: has_serial,
            serial: serial.to_owned(),
            ..SmxInfo::default()
        }
    }

    #[test]
    fn distinct_jumpers_assign_both_slots() {
        let plan = smx_assignment_plan(
            None,
            None,
            &info(true, false, true, "P1"),
            &info(true, true, true, "P2"),
        )
        .unwrap();
        assert_eq!(plan.p1_serial.as_deref(), Some("P1"));
        assert_eq!(plan.p2_serial.as_deref(), Some("P2"));
        assert_eq!(plan.source, SmxAssignmentSource::DistinctJumpers);
    }

    #[test]
    fn single_pad_follows_its_jumper_side() {
        let disconnected = info(false, false, false, "");
        let p1 = smx_assignment_plan(None, None, &info(true, false, true, "ONLY"), &disconnected)
            .unwrap();
        assert_eq!(p1.p1_serial.as_deref(), Some("ONLY"));
        assert_eq!(p1.p2_serial, None);
        assert_eq!(p1.source, SmxAssignmentSource::SingleP1);

        let p2 = smx_assignment_plan(None, None, &disconnected, &info(true, true, true, "ONLY"))
            .unwrap();
        assert_eq!(p2.p1_serial, None);
        assert_eq!(p2.p2_serial.as_deref(), Some("ONLY"));
        assert_eq!(p2.source, SmxAssignmentSource::SingleP2);
    }

    #[test]
    fn existing_or_ambiguous_assignments_are_left_unchanged() {
        let p1 = info(true, false, true, "A");
        let p2 = info(true, false, true, "B");
        assert!(smx_assignment_plan(Some("saved"), None, &p1, &p2).is_none());
        assert!(smx_assignment_plan(None, None, &p1, &p2).is_none());
        assert!(
            smx_assignment_plan(
                None,
                None,
                &info(true, false, false, ""),
                &SmxInfo::default(),
            )
            .is_none()
        );
    }

    #[test]
    fn runtime_assignment_skips_gameplay_and_disabled_smx() {
        let disconnected = SmxInfo::default();
        assert!(
            smx_runtime_assignment_plan(
                Screen::Gameplay,
                true,
                None,
                None,
                &info(true, false, true, "P1"),
                &disconnected,
            )
            .is_none()
        );
        assert!(
            smx_runtime_assignment_plan(
                Screen::Menu,
                false,
                None,
                None,
                &info(true, false, true, "P1"),
                &disconnected,
            )
            .is_none()
        );
        assert!(
            smx_runtime_assignment_plan(
                Screen::Menu,
                true,
                Some("saved"),
                None,
                &info(true, false, true, "P1"),
                &disconnected,
            )
            .is_none()
        );
        assert!(
            smx_runtime_assignment_plan(
                Screen::Menu,
                true,
                None,
                None,
                &info(true, false, true, "P1"),
                &disconnected,
            )
            .is_some()
        );
    }

    #[test]
    fn autoprompt_latches_only_unresolved_menu_conflicts() {
        assert_eq!(
            smx_autoprompt_plan(Screen::Menu, true, true, true, false),
            SmxAutopromptPlan {
                latch: true,
                unlatch: false,
                navigate_to_assign: true,
            }
        );
        assert_eq!(
            smx_autoprompt_plan(Screen::Menu, true, true, false, true),
            SmxAutopromptPlan {
                latch: false,
                unlatch: true,
                navigate_to_assign: false,
            }
        );
        assert_eq!(
            smx_autoprompt_plan(Screen::Gameplay, true, true, true, false),
            SmxAutopromptPlan::default()
        );
        assert_eq!(
            smx_autoprompt_plan(Screen::Menu, false, true, true, false),
            SmxAutopromptPlan::default()
        );
    }

    #[test]
    fn smx_preview_gates_follow_their_own_screens() {
        assert!(smx_options_light_preview_active(
            Screen::Options,
            true,
            true
        ));
        assert!(!smx_options_light_preview_active(
            Screen::Options,
            false,
            true
        ));
        assert!(smx_player_options_light_preview_allowed(
            Screen::PlayerOptions,
            true
        ));
        assert!(!smx_player_options_light_preview_allowed(
            Screen::Options,
            true
        ));
        assert!(!smx_light_preview_restore_auto(Screen::Gameplay));
        assert!(smx_light_preview_restore_auto(Screen::Options));
    }

    #[test]
    fn brightness_plan_skips_gameplay_and_applies_only_changes() {
        assert_eq!(
            smx_light_brightness_plan(Screen::Gameplay, true, [100, 100], [50, 75]),
            None
        );
        assert_eq!(
            smx_light_brightness_plan(Screen::Menu, true, [100, 100], [50, 75]),
            Some(SmxLightBrightnessPlan {
                resolved: [50, 75],
                apply: true,
            })
        );
        assert_eq!(
            smx_light_brightness_plan(Screen::Menu, false, [50, 75], [10, 20]),
            Some(SmxLightBrightnessPlan {
                resolved: [100, 100],
                apply: true,
            })
        );
        assert_eq!(
            smx_light_brightness_plan(Screen::Menu, false, [100, 100], [10, 20]),
            Some(SmxLightBrightnessPlan {
                resolved: [100, 100],
                apply: false,
            })
        );
    }
}
