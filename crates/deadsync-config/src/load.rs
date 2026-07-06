use crate::audio::{
    AudioOptions, AudioRuntimeOptions, load_audio_options, load_audio_runtime_options,
};
use crate::ini::SimpleIni;
use crate::null_or_die::{NullOrDieOptions, load_null_or_die_options};
use crate::options::{
    DisplayLoadOptions, RuntimeIoLoadOptions, RuntimeOptions, SelectMusicOptions,
    SystemInputHardwareLoadOptions, SystemOptions, load_display_options, load_gameplay_bg_color,
    load_runtime_io_options, load_runtime_options, load_select_music_options,
    load_system_input_hardware_options, load_system_options,
};
use crate::theme::{
    MachineFlowOptions, ThemePresentationOptions, ThemeShortcutOptions, load_machine_flow_options,
    load_theme_presentation_options, load_theme_shortcut_options,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigLoadDefaults<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub display: DisplayLoadOptions<F, P, V>,
    pub input_hardware: SystemInputHardwareLoadOptions<W, S>,
    pub audio_runtime: AudioRuntimeOptions<L, M>,
    pub runtime_io: RuntimeIoLoadOptions<D, G, R>,
    pub gameplay_bg_color: C,
    pub shortcuts: ThemeShortcutOptions<K>,
}

#[derive(Clone, Copy)]
pub struct ConfigLoadParsers<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub parse_fullscreen_type: fn(&str) -> Option<F>,
    pub parse_present_mode_policy: fn(&str) -> Option<P>,
    pub legacy_balanced_policy: P,
    pub legacy_unhinged_policy: P,
    pub parse_video_renderer: fn(&str) -> Option<V>,
    pub parse_gamepad_backend: fn(&str) -> Option<W>,
    pub parse_smx_pad_config: fn(&str) -> Option<S>,
    pub parse_linux_backend: fn(&str) -> Option<L>,
    pub parse_audio_output_mode: fn(&str) -> Option<M>,
    pub parse_input_debounce_seconds: fn(&str) -> Option<f32>,
    pub parse_lights_driver: fn(&str, D) -> D,
    pub parse_gameplay_pad_lights: fn(&str, G) -> G,
    pub parse_lights_com_port: fn(&str, R) -> R,
    pub parse_color: fn(&str) -> Option<C>,
    pub parse_key: fn(&str) -> Option<K>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadedConfigSections<F, P, V, W, S, L, M, D, G, R, C, K> {
    pub display: DisplayLoadOptions<F, P, V>,
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareLoadOptions<W, S>,
    pub gameplay_bg_color: C,
    pub null_or_die: NullOrDieOptions,
    pub audio_runtime: AudioRuntimeOptions<L, M>,
    pub audio: AudioOptions,
    pub select_music: SelectMusicOptions,
    pub runtime: RuntimeOptions,
    pub runtime_io: RuntimeIoLoadOptions<D, G, R>,
    pub theme_presentation: ThemePresentationOptions,
    pub machine_flow: MachineFlowOptions,
    pub shortcuts: ThemeShortcutOptions<K>,
}

pub fn load_config_sections<F, P, V, W, S, L, M, D, G, R, C, K>(
    conf: &SimpleIni,
    default: ConfigLoadDefaults<F, P, V, W, S, L, M, D, G, R, C, K>,
    parsers: ConfigLoadParsers<F, P, V, W, S, L, M, D, G, R, C, K>,
) -> LoadedConfigSections<F, P, V, W, S, L, M, D, G, R, C, K>
where
    F: Copy,
    P: Copy,
    V: Copy,
    W: Copy,
    S: Copy,
    L: Copy,
    M: Copy,
    D: Copy,
    G: Copy,
    R: Copy,
    C: Copy,
    K: Copy,
{
    LoadedConfigSections {
        display: load_display_options(
            conf,
            default.display,
            parsers.parse_fullscreen_type,
            parsers.parse_present_mode_policy,
            parsers.legacy_balanced_policy,
            parsers.legacy_unhinged_policy,
            parsers.parse_video_renderer,
        ),
        system: load_system_options(conf, SystemOptions::default()),
        input_hardware: load_system_input_hardware_options(
            conf,
            default.input_hardware,
            parsers.parse_gamepad_backend,
            parsers.parse_smx_pad_config,
        ),
        gameplay_bg_color: load_gameplay_bg_color(
            conf,
            default.gameplay_bg_color,
            parsers.parse_color,
        ),
        null_or_die: load_null_or_die_options(conf, NullOrDieOptions::default()),
        audio_runtime: load_audio_runtime_options(
            conf,
            default.audio_runtime,
            parsers.parse_linux_backend,
            parsers.parse_audio_output_mode,
        ),
        audio: load_audio_options(conf, AudioOptions::default()),
        select_music: load_select_music_options(conf, SelectMusicOptions::default()),
        runtime: load_runtime_options(conf, RuntimeOptions::default()),
        runtime_io: load_runtime_io_options(
            conf,
            default.runtime_io,
            parsers.parse_input_debounce_seconds,
            parsers.parse_lights_driver,
            parsers.parse_gameplay_pad_lights,
            parsers.parse_lights_com_port,
        ),
        theme_presentation: load_theme_presentation_options(
            conf,
            ThemePresentationOptions::default(),
        ),
        machine_flow: load_machine_flow_options(conf, MachineFlowOptions::default()),
        shortcuts: load_theme_shortcut_options(conf, default.shortcuts, parsers.parse_key),
    }
}
