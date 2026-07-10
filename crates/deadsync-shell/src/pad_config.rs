use deadsync_input::fsr::BackendKind;
use deadsync_input::fsr::PadDeviceId;
use deadsync_input_fsr::Monitor;
use deadsync_profile::pad_config::{self, PadConfigProfile};
use deadsync_profile::pad_config_sync::PadConfigSync;
use deadsync_screens::Screen;
use deadsync_screens::pad_config::{PadCommand, ProfileListEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadConfigFsrTarget {
    Screen,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadConfigFsrPlan {
    pub target: Option<PadConfigFsrTarget>,
    pub monitor_active: Option<bool>,
    pub managed_active: bool,
}

pub fn pad_config_fsr_plan(
    screen: Screen,
    use_fsrs: bool,
    overlay_visible: bool,
    fsr_pads_active: bool,
    smx_manages_pad_config: bool,
) -> Option<PadConfigFsrPlan> {
    let target = if screen == Screen::ConfigurePads && use_fsrs {
        Some(PadConfigFsrTarget::Screen)
    } else if screen == Screen::SelectMusic && overlay_visible && use_fsrs {
        Some(PadConfigFsrTarget::Overlay)
    } else {
        None
    };

    if let Some(target) = target {
        return Some(PadConfigFsrPlan {
            target: Some(target),
            monitor_active: (!fsr_pads_active).then_some(true),
            managed_active: smx_manages_pad_config,
        });
    }

    fsr_pads_active.then_some(PadConfigFsrPlan {
        target: None,
        monitor_active: Some(false),
        managed_active: smx_manages_pad_config,
    })
}

pub const fn pad_config_profile_cursor(
    target: PadConfigFsrTarget,
    selected_device: Option<PadDeviceId>,
) -> Option<PadDeviceId> {
    match (target, selected_device) {
        (PadConfigFsrTarget::Overlay, Some(dev)) if matches!(dev.backend, BackendKind::Smx) => {
            Some(dev)
        }
        _ => None,
    }
}

pub fn pad_config_profile_entries(
    configs: &[PadConfigProfile],
    active_name: Option<&str>,
    serial: Option<&str>,
) -> Vec<ProfileListEntry> {
    configs
        .iter()
        .map(|config| ProfileListEntry {
            is_active: active_name == Some(config.name.as_str()),
            is_default: serial.is_some_and(|serial| pad_config::is_default_for(config, serial)),
            name: config.name.clone(),
        })
        .collect()
}

pub fn apply_pad_commands(
    monitor: &mut Monitor,
    sync: &mut PadConfigSync,
    commands: Vec<PadCommand>,
) {
    for command in commands {
        let device = command.device();
        match command {
            PadCommand::Threshold {
                device,
                button,
                sensor,
                value,
            } => {
                let _ = monitor.set_threshold(device, button, sensor, value);
            }
            PadCommand::ThresholdPair {
                device,
                button,
                press,
                release,
            } => {
                let _ = monitor.set_threshold_pair(device, button, press, release);
            }
            PadCommand::SensorEnabled {
                device,
                button,
                sensor,
                enabled,
            } => {
                let _ = monitor.set_sensor_enabled(device, button, sensor, enabled);
            }
            PadCommand::AutoRecalibration { device, enabled } => {
                let _ = monitor.set_auto_recalibration(device, enabled);
            }
            PadCommand::Debounce { device, micros } => {
                let _ = monitor.set_debounce_micros(device, micros);
            }
        }
        if device.backend == BackendKind::Smx {
            sync.mark_diverged(device.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> PadConfigProfile {
        PadConfigProfile {
            name: name.to_owned(),
            backend: deadsync_smx::BACKEND_ID.to_owned(),
            pad_type: None,
            serial: None,
            default_for_serials: Vec::new(),
            global_default: false,
            settings: Vec::new(),
        }
    }

    #[test]
    fn fsr_plan_skips_when_unrelated_and_inactive() {
        assert_eq!(
            pad_config_fsr_plan(Screen::Gameplay, true, false, false, true),
            None
        );
    }

    #[test]
    fn fsr_plan_activates_for_config_screen() {
        assert_eq!(
            pad_config_fsr_plan(Screen::ConfigurePads, true, false, false, true),
            Some(PadConfigFsrPlan {
                target: Some(PadConfigFsrTarget::Screen),
                monitor_active: Some(true),
                managed_active: true,
            })
        );
    }

    #[test]
    fn fsr_plan_drives_select_music_overlay_without_reactivating() {
        assert_eq!(
            pad_config_fsr_plan(Screen::SelectMusic, true, true, true, false),
            Some(PadConfigFsrPlan {
                target: Some(PadConfigFsrTarget::Overlay),
                monitor_active: None,
                managed_active: false,
            })
        );
    }

    #[test]
    fn fsr_plan_deactivates_when_leaving_target() {
        assert_eq!(
            pad_config_fsr_plan(Screen::Menu, true, false, true, true),
            Some(PadConfigFsrPlan {
                target: None,
                monitor_active: Some(false),
                managed_active: true,
            })
        );
    }

    #[test]
    fn profile_cursor_is_overlay_smx_only() {
        let smx = PadDeviceId {
            backend: BackendKind::Smx,
            index: 1,
        };
        let other = PadDeviceId {
            backend: BackendKind::Fsrio,
            index: 0,
        };

        assert_eq!(
            pad_config_profile_cursor(PadConfigFsrTarget::Overlay, Some(smx)),
            Some(smx)
        );
        assert_eq!(
            pad_config_profile_cursor(PadConfigFsrTarget::Screen, Some(smx)),
            None
        );
        assert_eq!(
            pad_config_profile_cursor(PadConfigFsrTarget::Overlay, Some(other)),
            None
        );
    }

    #[test]
    fn profile_entries_mark_active_and_default() {
        let mut a = profile("soft");
        a.default_for_serials.push("abc".to_owned());
        let b = profile("firm");

        assert_eq!(
            pad_config_profile_entries(&[a, b], Some("firm"), Some("abc")),
            vec![
                ProfileListEntry {
                    name: "soft".to_owned(),
                    is_active: false,
                    is_default: true,
                },
                ProfileListEntry {
                    name: "firm".to_owned(),
                    is_active: true,
                    is_default: false,
                },
            ]
        );
    }
}
