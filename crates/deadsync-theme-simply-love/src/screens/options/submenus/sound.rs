use super::super::*;

pub(in crate::screens::options) const SOUND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SoundDevice,
        label: lookup_key("OptionsSound", "SoundDevice"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::AudioOutputMode,
        label: lookup_key("OptionsSound", "AudioOutputMode"),
        choices: &[
            localized_choice("OptionsSound", "OutputModeAuto"),
            localized_choice("OptionsSound", "OutputModeShared"),
        ],
        inline: false,
    },
    #[cfg(target_os = "linux")]
    SubRow {
        id: SubRowId::LinuxAudioBackend,
        label: lookup_key("OptionsSound", "LinuxAudioBackend"),
        choices: SOUND_LINUX_BACKEND_CHOICES,
        inline: false,
    },
    #[cfg(target_os = "linux")]
    SubRow {
        id: SubRowId::AlsaExclusive,
        label: lookup_key("OptionsSound", "AlsaExclusive"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AudioSampleRate,
        label: lookup_key("OptionsSound", "AudioSampleRate"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MasterVolume,
        label: lookup_key("OptionsSound", "MasterVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SfxVolume,
        label: lookup_key("OptionsSound", "SFXVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::AssistTickVolume,
        label: lookup_key("OptionsSound", "AssistTickVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MusicVolume,
        label: lookup_key("OptionsSound", "MusicVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MineSounds,
        label: lookup_key("OptionsSound", "MineSounds"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GlobalOffset,
        label: lookup_key("OptionsSound", "GlobalOffset"),
        choices: &[literal_choice("0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::RateModPreservesPitch,
        label: lookup_key("OptionsSound", "RateModPreservesPitch"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ReplayGain,
        label: lookup_key("OptionsSound", "ReplayGain"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const SOUND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SndDevice,
        name: lookup_key("OptionsSound", "SoundDevice"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "SoundDeviceHelp",
        ))],
    },
    Item {
        id: ItemId::SndOutputMode,
        name: lookup_key("OptionsSound", "AudioOutputMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AudioOutputModeHelp",
        ))],
    },
    #[cfg(target_os = "linux")]
    Item {
        id: ItemId::SndLinuxBackend,
        name: lookup_key("OptionsSound", "LinuxAudioBackend"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "LinuxAudioBackendHelp",
        ))],
    },
    #[cfg(target_os = "linux")]
    Item {
        id: ItemId::SndAlsaExclusive,
        name: lookup_key("OptionsSound", "AlsaExclusive"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AlsaExclusiveHelp",
        ))],
    },
    Item {
        id: ItemId::SndSampleRate,
        name: lookup_key("OptionsSound", "AudioSampleRate"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AudioSampleRateHelp",
        ))],
    },
    Item {
        id: ItemId::SndMasterVolume,
        name: lookup_key("OptionsSound", "MasterVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MasterVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndSfxVolume,
        name: lookup_key("OptionsSound", "SFXVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "SfxVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndAssistTickVolume,
        name: lookup_key("OptionsSound", "AssistTickVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AssistTickVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndMusicVolume,
        name: lookup_key("OptionsSound", "MusicVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MusicVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndMineSounds,
        name: lookup_key("OptionsSound", "MineSounds"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MineSoundsHelp",
        ))],
    },
    Item {
        id: ItemId::SndGlobalOffset,
        name: lookup_key("OptionsSound", "GlobalOffset"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "GlobalOffsetHelp",
        ))],
    },
    Item {
        id: ItemId::SndRateModPitch,
        name: lookup_key("OptionsSound", "RateModPreservesPitch"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "RateModPreservesPitchHelp",
        ))],
    },
    Item {
        id: ItemId::SndReplayGain,
        name: lookup_key("OptionsSound", "ReplayGain"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "ReplayGainHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

#[derive(Clone, Debug)]
pub(in crate::screens::options) struct SoundDeviceOption {
    pub(in crate::screens::options) label: String,
    pub(in crate::screens::options) config_index: Option<u16>,
    pub(in crate::screens::options) sample_rates_hz: Vec<u32>,
}

#[cfg(target_os = "linux")]
pub(in crate::screens::options) const SOUND_LINUX_BACKEND_CHOICES: &[Choice] =
    &[localized_choice("Common", "Auto")];

pub(in crate::screens::options) fn build_sound_device_options(
    audio_options: &AudioOptionsView,
) -> Vec<SoundDeviceOption> {
    let default_rates = audio_options
        .output_devices
        .iter()
        .find(|dev| dev.is_default)
        .map(|dev| dev.sample_rates_hz.clone())
        .unwrap_or_default();
    let mut options = Vec::with_capacity(audio_options.output_devices.len() + 1);
    options.push(SoundDeviceOption {
        label: tr("Common", "Auto").to_string(),
        config_index: None,
        sample_rates_hz: default_rates,
    });
    for (idx, dev) in audio_options.output_devices.iter().enumerate() {
        let mut label = dev.name.clone();
        if dev.is_default {
            label.push_str(&tr("OptionsSound", "DefaultSuffix"));
        }
        options.push(SoundDeviceOption {
            label,
            config_index: Some(idx as u16),
            sample_rates_hz: dev.sample_rates_hz.clone(),
        });
    }
    options
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn linux_backend_name_label(name: &str) -> std::sync::Arc<str> {
    if name.eq_ignore_ascii_case("auto") {
        tr("Common", "Auto")
    } else {
        std::sync::Arc::from(name)
    }
}

#[cfg(target_os = "linux")]
pub(in crate::screens::options) fn build_linux_backend_choices(
    audio_options: &AudioOptionsView,
) -> Vec<String> {
    audio_options
        .available_backend_names
        .iter()
        .map(|backend| linux_backend_name_label(backend).to_string())
        .collect()
}

pub(in crate::screens::options) fn sound_device_choice_index(
    options: &[SoundDeviceOption],
    config_index: Option<u16>,
) -> usize {
    let Some(target) = config_index else {
        return 0;
    };
    options
        .iter()
        .position(|opt| opt.config_index == Some(target))
        .unwrap_or(0)
}

pub(in crate::screens::options) fn sound_row_index(id: SubRowId) -> Option<usize> {
    SOUND_OPTIONS_ROWS.iter().position(|row| row.id == id)
}

pub(in crate::screens::options) fn selected_sound_device_choice(state: &State) -> usize {
    sound_row_index(SubRowId::SoundDevice)
        .and_then(|idx| {
            state.sub[SubmenuKind::Sound]
                .choice_indices
                .get(idx)
                .copied()
        })
        .unwrap_or(0)
}

pub(in crate::screens::options) fn sound_sample_rate_choices(state: &State) -> Vec<Option<u32>> {
    let device_idx =
        selected_sound_device_choice(state).min(state.sound_device_options.len().saturating_sub(1));
    if let Some(option) = state.sound_device_options.get(device_idx) {
        return audio_sample_rate_choices(&option.sample_rates_hz);
    }
    audio_sample_rate_choices(&[])
}

fn audio_sample_rate_choices(sample_rates_hz: &[u32]) -> Vec<Option<u32>> {
    let mut choices = Vec::with_capacity(sample_rates_hz.len() + 1);
    choices.push(None);
    for &hz in sample_rates_hz {
        let rate = Some(hz);
        if !choices.contains(&rate) {
            choices.push(rate);
        }
    }
    if choices.len() == 1 {
        choices.extend([Some(44_100), Some(48_000)]);
    }
    choices
}

pub(in crate::screens::options) fn sound_device_from_choice(
    state: &State,
    idx: usize,
) -> Option<u16> {
    state
        .sound_device_options
        .get(idx)
        .and_then(|opt| opt.config_index)
}

#[cfg(target_os = "linux")]
#[inline(always)]
pub(in crate::screens::options) fn selected_audio_output_mode(
    state: &State,
) -> AudioOutputModeChoice {
    sound_row_index(SubRowId::AudioOutputMode)
        .and_then(|idx| {
            state.sub[SubmenuKind::Sound]
                .choice_indices
                .get(idx)
                .copied()
        })
        .map(AudioOutputModeChoice::from_choice)
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub(in crate::screens::options) fn linux_audio_backend_choice_index(
    state: &State,
    backend_name: &str,
) -> usize {
    state
        .audio_options
        .available_backend_names
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(backend_name))
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
pub(in crate::screens::options) fn linux_audio_backend_name_from_choice(
    state: &State,
    idx: usize,
) -> &str {
    state
        .audio_options
        .available_backend_names
        .get(idx)
        .map(String::as_str)
        .unwrap_or("Auto")
}

#[cfg(target_os = "linux")]
#[inline(always)]
pub(in crate::screens::options) fn selected_linux_audio_backend_name(state: &State) -> &str {
    let index = sound_row_index(SubRowId::LinuxAudioBackend)
        .and_then(|idx| {
            state.sub[SubmenuKind::Sound]
                .choice_indices
                .get(idx)
                .copied()
        })
        .unwrap_or(0);
    linux_audio_backend_name_from_choice(state, index)
}

#[cfg(target_os = "linux")]
#[inline(always)]
pub(in crate::screens::options) fn sound_show_alsa_exclusive(state: &State) -> bool {
    selected_linux_audio_backend_name(state).eq_ignore_ascii_case("ALSA")
}

#[cfg(target_os = "linux")]
pub(in crate::screens::options) fn sound_parent_row(actual_idx: usize) -> Option<usize> {
    let child_idx = sound_row_index(SubRowId::AlsaExclusive)?;
    if actual_idx != child_idx {
        return None;
    }
    sound_row_index(SubRowId::LinuxAudioBackend)
}

pub(in crate::screens::options) fn set_sound_choice_index(
    state: &mut State,
    id: SubRowId,
    idx: usize,
) {
    let Some(row_idx) = sound_row_index(id) else {
        return;
    };
    if let Some(slot) = state.sub[SubmenuKind::Sound]
        .choice_indices
        .get_mut(row_idx)
    {
        *slot = idx;
    }
    if let Some(slot) = state.sub[SubmenuKind::Sound]
        .cursor_indices
        .get_mut(row_idx)
    {
        *slot = idx;
    }
}

pub(in crate::screens::options) fn sample_rate_choice_index(
    state: &State,
    rate: Option<u32>,
) -> usize {
    sound_sample_rate_choices(state)
        .iter()
        .position(|&value| value == rate)
        .unwrap_or(0)
}

pub(in crate::screens::options) fn sample_rate_from_choice(
    state: &State,
    idx: usize,
) -> Option<u32> {
    sound_sample_rate_choices(state).get(idx).copied().flatten()
}
