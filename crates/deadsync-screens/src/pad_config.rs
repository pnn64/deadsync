use deadsync_input::fsr::PadDeviceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadCommand {
    Threshold {
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    },
    ThresholdPair {
        device: PadDeviceId,
        button: usize,
        press: u16,
        release: u16,
    },
    SensorEnabled {
        device: PadDeviceId,
        button: usize,
        sensor: usize,
        enabled: bool,
    },
    AutoRecalibration {
        device: PadDeviceId,
        enabled: bool,
    },
    Debounce {
        device: PadDeviceId,
        micros: u16,
    },
}

impl PadCommand {
    #[inline(always)]
    pub const fn device(self) -> PadDeviceId {
        match self {
            Self::Threshold { device, .. }
            | Self::ThresholdPair { device, .. }
            | Self::SensorEnabled { device, .. }
            | Self::AutoRecalibration { device, .. }
            | Self::Debounce { device, .. } => device,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditResult {
    Handled,
    ExitToParent,
    SaveRequested,
    ApplyProfile,
    SetDefaultProfile,
}

#[derive(Clone, Debug, Default)]
pub struct SaveDraft {
    pub name: String,
    pub set_default: bool,
    pub rename_of: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileListEntry {
    pub name: String,
    pub is_default: bool,
    pub is_active: bool,
}

#[derive(Clone, Copy, Default)]
pub enum PadFilter {
    #[default]
    All,
    Sides {
        p1: bool,
        p2: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_input::fsr::BackendKind;

    #[test]
    fn every_command_preserves_its_target_device() {
        let device = PadDeviceId {
            backend: BackendKind::Smx,
            index: 1,
        };
        let commands = [
            PadCommand::Threshold {
                device,
                button: 0,
                sensor: None,
                value: 30,
            },
            PadCommand::ThresholdPair {
                device,
                button: 1,
                press: 80,
                release: 70,
            },
            PadCommand::SensorEnabled {
                device,
                button: 2,
                sensor: 3,
                enabled: false,
            },
            PadCommand::AutoRecalibration {
                device,
                enabled: true,
            },
            PadCommand::Debounce {
                device,
                micros: 4_000,
            },
        ];
        assert!(
            commands
                .into_iter()
                .all(|command| command.device() == device)
        );
    }
}
