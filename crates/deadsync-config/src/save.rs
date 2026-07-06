use crate::audio::{
    AudioDeviceOptions, AudioOptions, push_audio_device_option_lines,
    push_audio_music_option_lines, push_audio_playback_prefix_lines, push_audio_tail_option_lines,
    push_audio_write_current_screen_option_lines,
};
use crate::cache::push_never_cache_list_option_line;
use crate::folders::{AdditionalSongFolder, push_additional_song_folder_option_lines};
use crate::machine::push_default_noteskin_option_line;
use crate::null_or_die::{NullOrDieOptions, push_null_or_die_option_lines};
use crate::options::{
    DisplayOptions, RuntimeIoOptions, RuntimeOptions, SelectMusicSaveOptions, StatsOverlayOptions,
    SystemInputHardwareOptions, SystemOptions, push_display_frame_timing_option_lines,
    push_display_fullscreen_option_lines, push_display_monitor_option_lines,
    push_display_size_option_lines, push_display_video_tail_option_lines,
    push_gameplay_bg_color_option_line, push_runtime_audio_backend_option_lines,
    push_runtime_cache_option_lines, push_runtime_fastload_option_lines,
    push_runtime_input_debounce_option_lines, push_runtime_lights_driver_option_lines,
    push_runtime_lights_option_lines, push_runtime_lights_port_option_lines,
    push_runtime_menu_option_lines, push_runtime_navigation_option_lines,
    push_runtime_worker_theme_option_lines, push_select_music_option_lines,
    push_stats_overlay_option_lines, push_system_banner_cache_option_lines,
    push_system_bg_brightness_option_lines, push_system_cdtitle_center_option_lines,
    push_system_course_option_lines, push_system_diagnostics_option_lines,
    push_system_download_option_lines, push_system_input_hardware_option_lines,
    push_system_mine_hit_sound_option_lines, push_system_online_option_lines,
    push_system_translation_option_lines,
};
use crate::runtime_state::{
    RuntimeStateIdTokens, push_pad_order_option_lines, push_runtime_state_id_option_lines,
};
use crate::theme::{
    MachineFlowOptions, ThemePresentationOptions, ThemeShortcutTokens, push_theme_option_lines,
};
use crate::writer::push_section;
use std::fmt::Display;

pub struct SavedOptionSection<'a, P> {
    pub audio: AudioOptions,
    pub audio_device: AudioDeviceOptions<'a>,
    pub additional_song_folders: &'a [AdditionalSongFolder],
    pub never_cache_list: &'a [String],
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareOptions<'a>,
    pub display: DisplayOptions<'a>,
    pub runtime_io: RuntimeIoOptions<'a>,
    pub runtime: RuntimeOptions,
    pub stats_overlay: StatsOverlayOptions<'a>,
    pub select_music: SelectMusicSaveOptions,
    pub null_or_die: NullOrDieOptions,
    pub gameplay_bg_color: &'a str,
    pub default_noteskin: &'a str,
    pub runtime_state_ids: RuntimeStateIdTokens<'a>,
    pub pad_order_lines: P,
}

pub struct DefaultOptionSection<'a, P> {
    pub audio: AudioOptions,
    pub audio_device: AudioDeviceOptions<'a>,
    pub additional_song_folders: &'a [AdditionalSongFolder],
    pub never_cache_list: &'a [String],
    pub system: SystemOptions,
    pub input_hardware: SystemInputHardwareOptions<'a>,
    pub display: DisplayOptions<'a>,
    pub runtime_io: RuntimeIoOptions<'a>,
    pub runtime: RuntimeOptions,
    pub stats_overlay: StatsOverlayOptions<'a>,
    pub select_music: SelectMusicSaveOptions,
    pub gameplay_bg_color: &'a str,
    pub default_noteskin: &'a str,
    pub runtime_state_ids: RuntimeStateIdTokens<'a>,
    pub pad_order_lines: P,
}

pub struct ThemeSection<'a> {
    pub presentation: ThemePresentationOptions,
    pub machine: MachineFlowOptions,
    pub shortcuts: ThemeShortcutTokens<'a>,
    pub null_or_die: Option<NullOrDieOptions>,
}

pub struct SavedConfigFile<'a, P, K> {
    pub options: SavedOptionSection<'a, P>,
    pub keymap: K,
    pub theme: ThemeSection<'a>,
}

pub struct DefaultConfigFile<'a, P, K> {
    pub options: DefaultOptionSection<'a, P>,
    pub keymap: K,
    pub theme: ThemeSection<'a>,
}

pub fn build_saved_config_file<P, V, K>(
    file: SavedConfigFile<'_, P, K>,
    push_keymap: impl FnOnce(&mut String, K),
) -> String
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    let mut content = String::with_capacity(4096);
    push_saved_option_section(&mut content, file.options);
    push_keymap(&mut content, file.keymap);
    push_theme_section(&mut content, file.theme);
    content
}

pub fn build_default_config_file<P, V, K>(
    file: DefaultConfigFile<'_, P, K>,
    push_keymap: impl FnOnce(&mut String, K),
) -> String
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    let mut content = String::with_capacity(4096);
    push_default_option_section(&mut content, file.options);
    push_keymap(&mut content, file.keymap);
    push_theme_section(&mut content, file.theme);
    content
}

pub fn push_saved_option_section<P, V>(content: &mut String, options: SavedOptionSection<'_, P>)
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    push_section(content, "[Options]");
    push_audio_device_option_lines(content, options.audio_device);
    push_additional_song_folder_option_lines(content, options.additional_song_folders);
    push_system_download_option_lines(content, options.system);
    push_system_bg_brightness_option_lines(content, options.system);
    push_gameplay_bg_color_option_line(content, options.gameplay_bg_color);
    push_system_banner_cache_option_lines(content, options.system);
    push_runtime_cache_option_lines(content, options.runtime);
    push_never_cache_list_option_line(content, options.never_cache_list);
    push_system_cdtitle_center_option_lines(content, options.system);
    push_system_course_option_lines(content, options.system);
    push_null_or_die_option_lines(content, options.null_or_die);
    push_default_noteskin_option_line(content, options.default_noteskin);
    push_display_size_option_lines(content, options.display);
    push_system_online_option_lines(content, options.system);
    push_runtime_fastload_option_lines(content, options.runtime);
    push_display_fullscreen_option_lines(content, options.display);
    push_system_input_hardware_option_lines(content, options.input_hardware);
    push_runtime_state_id_option_lines(content, options.runtime_state_ids);
    push_pad_order_option_lines(content, options.pad_order_lines);
    push_system_diagnostics_option_lines(content, options.system);
    push_runtime_audio_backend_option_lines(content, options.runtime_io);
    push_display_frame_timing_option_lines(content, options.display);
    push_audio_playback_prefix_lines(content, options.audio);
    push_system_mine_hit_sound_option_lines(content, options.system);
    push_audio_music_option_lines(content, options.audio);
    push_select_music_option_lines(content, options.select_music);
    push_stats_overlay_option_lines(content, options.stats_overlay);
    push_runtime_input_debounce_option_lines(content, options.runtime_io);
    push_runtime_navigation_option_lines(content, options.runtime);
    push_runtime_lights_driver_option_lines(content, options.runtime_io);
    push_runtime_lights_option_lines(content, options.runtime);
    push_runtime_lights_port_option_lines(content, options.runtime_io);
    push_runtime_menu_option_lines(content, options.runtime);
    push_display_monitor_option_lines(content, options.display);
    push_runtime_worker_theme_option_lines(content, options.runtime);
    push_audio_tail_option_lines(content, options.audio);
    push_system_translation_option_lines(content, options.system);
    push_display_video_tail_option_lines(content, options.display);
    push_audio_write_current_screen_option_lines(content, options.audio);
    content.push('\n');
}

pub fn push_default_option_section<P, V>(content: &mut String, options: DefaultOptionSection<'_, P>)
where
    P: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    push_section(content, "[Options]");
    push_audio_device_option_lines(content, options.audio_device);
    push_additional_song_folder_option_lines(content, options.additional_song_folders);
    push_system_download_option_lines(content, options.system);
    push_system_bg_brightness_option_lines(content, options.system);
    push_gameplay_bg_color_option_line(content, options.gameplay_bg_color);
    push_system_banner_cache_option_lines(content, options.system);
    push_runtime_cache_option_lines(content, options.runtime);
    push_never_cache_list_option_line(content, options.never_cache_list);
    push_system_cdtitle_center_option_lines(content, options.system);
    push_system_course_option_lines(content, options.system);
    push_default_noteskin_option_line(content, options.default_noteskin);
    push_display_size_option_lines(content, options.display);
    push_display_monitor_option_lines(content, options.display);
    push_system_online_option_lines(content, options.system);
    push_runtime_fastload_option_lines(content, options.runtime);
    push_display_fullscreen_option_lines(content, options.display);
    push_system_input_hardware_option_lines(content, options.input_hardware);
    push_runtime_state_id_option_lines(content, options.runtime_state_ids);
    push_pad_order_option_lines(content, options.pad_order_lines);
    push_system_diagnostics_option_lines(content, options.system);
    push_runtime_audio_backend_option_lines(content, options.runtime_io);
    push_display_frame_timing_option_lines(content, options.display);
    push_audio_playback_prefix_lines(content, options.audio);
    push_system_mine_hit_sound_option_lines(content, options.system);
    push_audio_music_option_lines(content, options.audio);
    push_select_music_option_lines(content, options.select_music);
    push_stats_overlay_option_lines(content, options.stats_overlay);
    push_runtime_input_debounce_option_lines(content, options.runtime_io);
    push_runtime_navigation_option_lines(content, options.runtime);
    push_runtime_lights_driver_option_lines(content, options.runtime_io);
    push_runtime_lights_option_lines(content, options.runtime);
    push_runtime_lights_port_option_lines(content, options.runtime_io);
    push_runtime_menu_option_lines(content, options.runtime);
    push_runtime_worker_theme_option_lines(content, options.runtime);
    push_audio_tail_option_lines(content, options.audio);
    push_system_translation_option_lines(content, options.system);
    push_display_video_tail_option_lines(content, options.display);
    push_audio_write_current_screen_option_lines(content, options.audio);
    content.push('\n');
}

pub fn push_theme_section(content: &mut String, section: ThemeSection<'_>) {
    push_section(content, "[Theme]");
    push_theme_option_lines(
        content,
        section.presentation,
        section.machine,
        section.shortcuts,
    );
    if let Some(null_or_die) = section.null_or_die {
        push_null_or_die_option_lines(content, null_or_die);
    }
    content.push('\n');
}
