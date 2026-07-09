use crate::app_update as config_update;
use crate::cache::load_never_cache_list;
use crate::folders::{AdditionalSongFolder, load_additional_song_folders};
use crate::ini::SimpleIni;
use crate::keybinds::publish_keymap_from_ini;
use crate::load::load_app_config;
use crate::machine::{DEFAULT_MACHINE_NOTESKIN, normalize_machine_default_noteskin};
use crate::null_or_die::{null_or_die_bias_cfg, null_or_die_options_from_config};
use crate::pad_order as config_pad_order;
use crate::save::build_saved_app_config_file;
use crate::writer::push_line;
use crate::{
    app_config::Config,
    cache::group_is_never_cached,
    folders::song_path_is_writable_for_roots,
    update::{
        DedicatedMenuNavigation, resolve_dedicated_menu_navigation, set_if_changed,
        set_pair_if_changed,
    },
};
use deadlib_platform::lock_wait::{LockWaitStats, lock_mutex};
use deadsync_audio::AudioMixLevels;
use deadsync_input::Keymap;
use null_or_die::BiasCfg;
use std::fmt::Display;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStateOptions {
    pub machine_default_noteskin: String,
    pub additional_song_folders: Vec<AdditionalSongFolder>,
    pub never_cache_list: Vec<String>,
    pub ids: RuntimeStateIds,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeStateIds {
    pub smx_p1_serial: Option<String>,
    pub smx_p2_serial: Option<String>,
    pub default_profile_p1: Option<String>,
    pub default_profile_p2: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStateIdTokens<'a> {
    pub smx_p1_serial: &'a str,
    pub smx_p2_serial: &'a str,
    pub default_profile_p1: &'a str,
    pub default_profile_p2: &'a str,
}

pub type PadOrderEntry = (String, String);

#[derive(Debug, Clone)]
pub struct SaveSnapshot {
    pub config: Config,
    pub machine_default_noteskin: String,
    pub additional_song_folders: Vec<AdditionalSongFolder>,
    pub never_cache_list: Vec<String>,
    pub smx_p1_serial: String,
    pub smx_p2_serial: String,
    pub default_profile_p1: String,
    pub default_profile_p2: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PublishedConfigEffects {
    pub audio_mix_levels: AudioMixLevels,
    pub replaygain_enabled: bool,
    pub preserve_pitch_enabled: bool,
    pub overscan: (i32, i32, i32, i32),
    pub log_to_file: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputRuntimeState {
    pub dedicated: DedicatedMenuNavigation,
    pub three_key_navigation: bool,
    pub input_debounce_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreeKeyNavigationUpdate {
    pub changed: bool,
    pub dedicated: DedicatedMenuNavigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DedicatedMenuButtonsUpdate {
    pub changed: bool,
    pub dedicated: DedicatedMenuNavigation,
    pub three_key_navigation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmxUnderglowColors {
    pub grb: bool,
    pub colors: [Option<[u8; 3]>; 2],
}

pub struct RuntimeConfigStore {
    config: Mutex<Config>,
    machine_default_noteskin: Mutex<String>,
    additional_song_folders: Mutex<Vec<AdditionalSongFolder>>,
    never_cache_list: Mutex<Vec<String>>,
    smx_p1_serial: Mutex<Option<String>>,
    smx_p2_serial: Mutex<Option<String>>,
    default_profile_p1: Mutex<Option<String>>,
    default_profile_p2: Mutex<Option<String>>,
    config_lock_wait_stats: LockWaitStats,
}

impl RuntimeConfigStore {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::default()),
            machine_default_noteskin: Mutex::new(DEFAULT_MACHINE_NOTESKIN.to_string()),
            additional_song_folders: Mutex::new(Vec::new()),
            never_cache_list: Mutex::new(Vec::new()),
            smx_p1_serial: Mutex::new(None),
            smx_p2_serial: Mutex::new(None),
            default_profile_p1: Mutex::new(None),
            default_profile_p2: Mutex::new(None),
            config_lock_wait_stats: LockWaitStats::new(),
        }
    }

    #[inline(always)]
    pub fn lock_config(&self) -> MutexGuard<'_, Config> {
        lock_mutex("CONFIG", &self.config, &self.config_lock_wait_stats)
    }

    pub fn config(&self) -> Config {
        *self.lock_config()
    }

    pub fn publish_config(&self, cfg: Config) -> PublishedConfigEffects {
        let effects = PublishedConfigEffects::from_config(&cfg);
        *self.lock_config() = cfg;
        effects
    }

    pub fn update_config(&self, apply: impl FnOnce(&mut Config) -> bool) -> bool {
        let mut cfg = self.lock_config();
        apply(&mut cfg)
    }

    pub fn update_three_key_navigation(
        &self,
        enabled: bool,
        dedicated_bindings_supported: bool,
    ) -> ThreeKeyNavigationUpdate {
        let mut cfg = self.lock_config();
        let dedicated = resolve_dedicated_menu_navigation(
            cfg.only_dedicated_menu_buttons,
            dedicated_bindings_supported,
        );
        if cfg.three_key_navigation == enabled {
            return ThreeKeyNavigationUpdate {
                changed: false,
                dedicated,
            };
        }

        cfg.three_key_navigation = enabled;
        if dedicated.disabled_by_missing_bindings {
            cfg.only_dedicated_menu_buttons = dedicated.enabled;
        }
        ThreeKeyNavigationUpdate {
            changed: true,
            dedicated,
        }
    }

    pub fn update_only_dedicated_menu_buttons(
        &self,
        enabled: bool,
        dedicated_bindings_supported: bool,
    ) -> DedicatedMenuButtonsUpdate {
        let mut cfg = self.lock_config();
        let dedicated = resolve_dedicated_menu_navigation(enabled, dedicated_bindings_supported);
        let enabled = dedicated.enabled;
        let changed = set_if_changed(&mut cfg.only_dedicated_menu_buttons, enabled);
        DedicatedMenuButtonsUpdate {
            changed,
            dedicated,
            three_key_navigation: cfg.three_key_navigation,
        }
    }

    pub fn update_input_debounce_seconds(&self, seconds: f32) -> Option<f32> {
        let mut cfg = self.lock_config();
        if config_update::set_input_debounce_seconds(&mut cfg, seconds) {
            Some(cfg.input_debounce_seconds)
        } else {
            None
        }
    }

    pub fn update_master_volume(&self, volume: u8) -> Option<AudioMixLevels> {
        self.update_audio_mix_config(|cfg| config_update::set_master_volume(cfg, volume))
    }

    pub fn update_music_volume(&self, volume: u8) -> Option<AudioMixLevels> {
        self.update_audio_mix_config(|cfg| config_update::set_music_volume(cfg, volume))
    }

    pub fn update_sfx_volume(&self, volume: u8) -> Option<AudioMixLevels> {
        self.update_audio_mix_config(|cfg| config_update::set_sfx_volume(cfg, volume))
    }

    pub fn update_assist_tick_volume(&self, volume: u8) -> Option<AudioMixLevels> {
        self.update_audio_mix_config(|cfg| config_update::set_assist_tick_volume(cfg, volume))
    }

    pub fn update_rate_mod_preserves_pitch(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_rate_mod_preserves_pitch(cfg, enabled))
    }

    pub fn update_enable_replaygain(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_enable_replaygain(cfg, enabled))
    }

    pub fn update_simply_love_color(&self, index: i32) -> bool {
        self.update_config(|cfg| config_update::set_simply_love_color(cfg, index))
    }

    pub fn update_log_level(&self, level: crate::theme::LogLevel) -> bool {
        self.update_config(|cfg| config_update::set_log_level(cfg, level))
    }

    pub fn update_log_to_file(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_log_to_file(cfg, enabled))
    }

    pub fn update_overscan(
        &self,
        translate_x: i32,
        translate_y: i32,
        add_width: i32,
        add_height: i32,
    ) -> bool {
        self.update_config(|cfg| {
            config_update::set_overscan(cfg, translate_x, translate_y, add_width, add_height)
        })
    }

    pub fn smx_underglow_colors(&self, lone_pad: bool) -> Option<SmxUnderglowColors> {
        smx_underglow_colors_from_config(&self.config(), lone_pad)
    }

    pub fn null_or_die_bias_cfg(&self) -> BiasCfg {
        null_or_die_bias_cfg(null_or_die_options_from_config(self.config()))
    }

    pub fn update_smx_input(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_input(cfg, enabled))
    }

    pub fn update_smx_manages_pad_config(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_manages_pad_config(cfg, enabled))
    }

    pub fn update_smx_panel_lights(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_panel_lights(cfg, enabled))
    }

    pub fn update_smx_pad_gifs_pack(&self, pack: crate::options::SmxPackName) -> bool {
        self.update_config(|cfg| config_update::set_smx_pad_gifs_pack(cfg, pack))
    }

    pub fn update_smx_judge_gifs_pack(&self, pack: crate::options::SmxPackName) -> bool {
        self.update_config(|cfg| config_update::set_smx_judge_gifs_pack(cfg, pack))
    }

    pub fn update_smx_underglow_theme(&self, enabled: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_underglow_theme(cfg, enabled))
    }

    pub fn update_smx_underglow_grb(&self, grb: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_underglow_grb(cfg, grb))
    }

    pub fn update_smx_idle_lights_black(&self, black: bool) -> bool {
        self.update_config(|cfg| config_update::set_smx_idle_lights_black(cfg, black))
    }

    pub fn update_smx_default_pad_config(&self, preset: deadsync_smx::SmxPadPreset) -> bool {
        self.update_config(|cfg| config_update::set_smx_default_pad_config(cfg, preset))
    }

    pub fn update_smx_default_light_brightness(&self, percent: u8) -> bool {
        self.update_config(|cfg| config_update::set_smx_default_light_brightness(cfg, percent))
    }

    fn update_audio_mix_config(
        &self,
        apply: impl FnOnce(&mut Config) -> bool,
    ) -> Option<AudioMixLevels> {
        let mut cfg = self.lock_config();
        if apply(&mut cfg) {
            Some(audio_mix_levels_from_config(&cfg))
        } else {
            None
        }
    }

    pub fn publish_runtime_state(&self, state: RuntimeStateOptions) {
        *self.machine_default_noteskin.lock().unwrap() = state.machine_default_noteskin;
        *self.additional_song_folders.lock().unwrap() = state.additional_song_folders;
        *self.never_cache_list.lock().unwrap() = state.never_cache_list;
        let ids = state.ids;
        *self.smx_p1_serial.lock().unwrap() = ids.smx_p1_serial;
        *self.smx_p2_serial.lock().unwrap() = ids.smx_p2_serial;
        *self.default_profile_p1.lock().unwrap() = ids.default_profile_p1;
        *self.default_profile_p2.lock().unwrap() = ids.default_profile_p2;
    }

    pub fn publish_runtime_state_from_ini(&self, conf: &SimpleIni) {
        self.publish_runtime_state(load_runtime_state_options(conf));
    }

    pub fn load_from_ini(&self, conf: &SimpleIni, default: Config) -> PublishedConfigEffects {
        self.publish_runtime_state_from_ini(conf);
        config_pad_order::load_order_from_ini(conf);
        let effects = self.publish_config(load_app_config(conf, default));
        publish_keymap_from_ini(conf);
        effects
    }

    pub fn reset_runtime_state(&self) {
        self.publish_runtime_state(RuntimeStateOptions::default());
    }

    pub fn reset_load_state(&self) {
        self.reset_runtime_state();
        config_pad_order::reset();
    }

    pub fn machine_default_noteskin(&self) -> String {
        self.machine_default_noteskin.lock().unwrap().clone()
    }

    pub fn smx_pad_assignment(&self) -> (Option<String>, Option<String>) {
        (
            self.smx_p1_serial.lock().unwrap().clone(),
            self.smx_p2_serial.lock().unwrap().clone(),
        )
    }

    pub fn default_profiles(&self) -> (Option<String>, Option<String>) {
        (
            self.default_profile_p1.lock().unwrap().clone(),
            self.default_profile_p2.lock().unwrap().clone(),
        )
    }

    pub fn additional_song_folder_roots(&self) -> Vec<AdditionalSongFolder> {
        self.additional_song_folders.lock().unwrap().clone()
    }

    pub fn never_cache_list(&self) -> Vec<String> {
        self.never_cache_list.lock().unwrap().clone()
    }

    pub fn group_is_never_cached(&self, group: &str) -> bool {
        group_is_never_cached(self.never_cache_list.lock().unwrap().as_slice(), group)
    }

    pub fn song_path_is_writable(&self, path: &Path) -> bool {
        let roots = self.additional_song_folders.lock().unwrap().clone();
        song_path_is_writable_for_roots(path, &roots)
    }

    pub fn save_snapshot(&self) -> SaveSnapshot {
        SaveSnapshot {
            config: self.config(),
            machine_default_noteskin: self.machine_default_noteskin(),
            additional_song_folders: self.additional_song_folder_roots(),
            never_cache_list: self.never_cache_list(),
            smx_p1_serial: self
                .smx_p1_serial
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default(),
            smx_p2_serial: self
                .smx_p2_serial
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default(),
            default_profile_p1: self
                .default_profile_p1
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default(),
            default_profile_p2: self
                .default_profile_p2
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default(),
        }
    }

    pub fn save_content(&self, keymap: &Keymap) -> String {
        let snapshot = self.save_snapshot();
        build_saved_app_config_file(
            &snapshot.config,
            keymap,
            &snapshot.machine_default_noteskin,
            snapshot.additional_song_folders.as_slice(),
            snapshot.never_cache_list.as_slice(),
            &snapshot.smx_p1_serial,
            &snapshot.smx_p2_serial,
            &snapshot.default_profile_p1,
            &snapshot.default_profile_p2,
        )
    }

    pub fn set_smx_pad_assignment(
        &self,
        p1_serial: Option<String>,
        p2_serial: Option<String>,
    ) -> bool {
        set_pair_if_changed(
            &mut *self.smx_p1_serial.lock().unwrap(),
            p1_serial,
            &mut *self.smx_p2_serial.lock().unwrap(),
            p2_serial,
        )
    }

    pub fn set_default_profiles(&self, p1: Option<String>, p2: Option<String>) -> bool {
        set_pair_if_changed(
            &mut *self.default_profile_p1.lock().unwrap(),
            p1,
            &mut *self.default_profile_p2.lock().unwrap(),
            p2,
        )
    }

    pub fn set_machine_default_noteskin(&self, noteskin: &str) -> bool {
        let normalized = normalize_machine_default_noteskin(noteskin);
        set_if_changed(
            &mut *self.machine_default_noteskin.lock().unwrap(),
            normalized,
        )
    }

    pub fn apply_input_runtime_state(
        &self,
        dedicated_bindings_supported: bool,
    ) -> InputRuntimeState {
        let mut cfg = self.lock_config();
        let dedicated = resolve_dedicated_menu_navigation(
            cfg.only_dedicated_menu_buttons,
            dedicated_bindings_supported,
        );
        if dedicated.disabled_by_missing_bindings {
            cfg.only_dedicated_menu_buttons = dedicated.enabled;
        }
        InputRuntimeState {
            dedicated,
            three_key_navigation: cfg.three_key_navigation,
            input_debounce_seconds: cfg.input_debounce_seconds,
        }
    }
}

impl PublishedConfigEffects {
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            audio_mix_levels: audio_mix_levels_from_config(cfg),
            replaygain_enabled: cfg.enable_replaygain,
            preserve_pitch_enabled: cfg.rate_mod_preserves_pitch,
            overscan: (
                cfg.center_image_translate_x,
                cfg.center_image_translate_y,
                cfg.center_image_add_width,
                cfg.center_image_add_height,
            ),
            log_to_file: cfg.log_to_file,
        }
    }
}

pub fn audio_mix_levels_from_config(cfg: &Config) -> AudioMixLevels {
    AudioMixLevels {
        master_volume: cfg.master_volume,
        music_volume: cfg.music_volume,
        sfx_volume: cfg.sfx_volume,
        assist_tick_volume: cfg.assist_tick_volume,
    }
}

pub fn smx_underglow_colors_from_config(
    cfg: &Config,
    lone_pad: bool,
) -> Option<SmxUnderglowColors> {
    if !cfg.smx_input || !cfg.smx_underglow_theme {
        return None;
    }

    let p1_rgb = decorative_rgb(cfg.simply_love_color);
    let p2_rgb = if lone_pad {
        p1_rgb
    } else {
        decorative_rgb(cfg.simply_love_color - 2)
    };
    Some(SmxUnderglowColors {
        grb: cfg.smx_underglow_grb,
        colors: [Some(p1_rgb), Some(p2_rgb)],
    })
}

fn decorative_rgb(index: i32) -> [u8; 3] {
    let rgba = deadlib_present::color::decorative_rgba(index);
    // The palette is sRGB for screen use; the LED strips are linear, so the
    // colours wash toward white without the gamma expansion.
    deadsync_smx::gifs::saturate_for_leds([
        color_component_to_u8(rgba[0]),
        color_component_to_u8(rgba[1]),
        color_component_to_u8(rgba[2]),
    ])
}

fn color_component_to_u8(c: f32) -> u8 {
    (c * 255.0).round() as u8
}

impl Default for RuntimeConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for RuntimeStateOptions {
    fn default() -> Self {
        Self {
            machine_default_noteskin: DEFAULT_MACHINE_NOTESKIN.to_string(),
            additional_song_folders: Vec::new(),
            never_cache_list: Vec::new(),
            ids: RuntimeStateIds::default(),
        }
    }
}

pub fn load_runtime_state_options(conf: &SimpleIni) -> RuntimeStateOptions {
    load_runtime_state_options_with_default_noteskin(conf, DEFAULT_MACHINE_NOTESKIN)
}

pub fn load_runtime_state_options_with_default_noteskin(
    conf: &SimpleIni,
    default_noteskin: &str,
) -> RuntimeStateOptions {
    RuntimeStateOptions {
        machine_default_noteskin: conf
            .get("Options", "DefaultNoteSkin")
            .map(|v| normalize_machine_default_noteskin(&v))
            .unwrap_or_else(|| default_noteskin.to_string()),
        additional_song_folders: load_additional_song_folders(conf),
        never_cache_list: load_never_cache_list(conf),
        ids: load_runtime_state_ids(conf),
    }
}

pub fn load_runtime_state_ids(conf: &SimpleIni) -> RuntimeStateIds {
    RuntimeStateIds {
        smx_p1_serial: nonempty_option(conf, "SmxP1Serial"),
        smx_p2_serial: nonempty_option(conf, "SmxP2Serial"),
        default_profile_p1: profile_id(conf, "DefaultLocalProfileIDP1", "LastProfileP1"),
        default_profile_p2: profile_id(conf, "DefaultLocalProfileIDP2", "LastProfileP2"),
    }
}

pub fn load_pad_order_entries(conf: &SimpleIni) -> Option<Vec<PadOrderEntry>> {
    conf.get_section("Options").map(|section| {
        section
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    })
}

fn profile_id(conf: &SimpleIni, key: &str, fallback_key: &str) -> Option<String> {
    nonempty_option(conf, key).or_else(|| nonempty_option(conf, fallback_key))
}

fn nonempty_option(conf: &SimpleIni, key: &str) -> Option<String> {
    conf.get("Options", key)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

pub fn push_runtime_state_id_option_lines(content: &mut String, ids: RuntimeStateIdTokens<'_>) {
    push_line(content, "SmxP1Serial", ids.smx_p1_serial);
    push_line(content, "SmxP2Serial", ids.smx_p2_serial);
    push_line(content, "DefaultLocalProfileIDP1", ids.default_profile_p1);
    push_line(content, "DefaultLocalProfileIDP2", ids.default_profile_p2);
}

pub fn push_pad_order_option_lines<I, V>(content: &mut String, lines: I)
where
    I: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    for (key, value) in lines {
        push_line(content, key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

    #[test]
    fn trims_smx_serials_and_ignores_blanks() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
SmxP1Serial= P1-123 \n\
SmxP2Serial=   \n"));

        assert_eq!(ids.smx_p1_serial.as_deref(), Some("P1-123"));
        assert_eq!(ids.smx_p2_serial, None);
    }

    #[test]
    fn current_profile_ids_take_precedence() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
DefaultLocalProfileIDP1= current-p1 \n\
LastProfileP1= legacy-p1\n\
DefaultLocalProfileIDP2=current-p2\n\
LastProfileP2=legacy-p2\n"));

        assert_eq!(ids.default_profile_p1.as_deref(), Some("current-p1"));
        assert_eq!(ids.default_profile_p2.as_deref(), Some("current-p2"));
    }

    #[test]
    fn profile_ids_fall_back_to_legacy_keys() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
DefaultLocalProfileIDP1=   \n\
LastProfileP1= legacy-p1 \n\
LastProfileP2=legacy-p2\n"));

        assert_eq!(ids.default_profile_p1.as_deref(), Some("legacy-p1"));
        assert_eq!(ids.default_profile_p2.as_deref(), Some("legacy-p2"));
    }

    #[test]
    fn missing_runtime_ids_are_none() {
        assert_eq!(
            load_runtime_state_ids(&ini("[Options]\n")),
            RuntimeStateIds::default()
        );
    }

    #[test]
    fn load_runtime_state_options_groups_runtime_only_values() {
        let state = load_runtime_state_options(&ini("[Options]\n\
DefaultNoteSkin= cyber \n\
AdditionalSongFoldersWritable=C:/Songs\n\
AdditionalSongFoldersReadOnly=D:/Locked\n\
NeverCacheList= Pack A, Pack B \n\
SmxP1Serial= pad-1\n\
DefaultLocalProfileIDP2= profile-2\n"));

        assert_eq!(state.machine_default_noteskin, "cyber");
        assert_eq!(
            state.additional_song_folders,
            vec![
                AdditionalSongFolder {
                    path: "D:/Locked".to_string(),
                    writable: false,
                },
                AdditionalSongFolder {
                    path: "C:/Songs".to_string(),
                    writable: true,
                },
            ]
        );
        assert_eq!(state.never_cache_list, ["Pack A", "Pack B"]);
        assert_eq!(state.ids.smx_p1_serial.as_deref(), Some("pad-1"));
        assert_eq!(state.ids.default_profile_p2.as_deref(), Some("profile-2"));
    }

    #[test]
    fn runtime_state_options_default_to_empty_runtime_state() {
        assert_eq!(
            load_runtime_state_options(&ini("[Options]\n")),
            RuntimeStateOptions::default()
        );
    }

    #[test]
    fn runtime_config_store_snapshots_runtime_state() {
        let store = RuntimeConfigStore::new();
        store.publish_runtime_state(RuntimeStateOptions {
            machine_default_noteskin: "cyber".to_string(),
            additional_song_folders: vec![AdditionalSongFolder {
                path: "C:/Songs".to_string(),
                writable: true,
            }],
            never_cache_list: vec!["WIP Pack".to_string()],
            ids: RuntimeStateIds {
                smx_p1_serial: Some("pad-1".to_string()),
                smx_p2_serial: None,
                default_profile_p1: None,
                default_profile_p2: Some("profile-2".to_string()),
            },
        });

        let snapshot = store.save_snapshot();
        assert_eq!(snapshot.machine_default_noteskin, "cyber");
        assert_eq!(snapshot.additional_song_folders[0].path, "C:/Songs");
        assert_eq!(snapshot.never_cache_list, ["WIP Pack"]);
        assert_eq!(snapshot.smx_p1_serial, "pad-1");
        assert_eq!(snapshot.smx_p2_serial, "");
        assert_eq!(snapshot.default_profile_p1, "");
        assert_eq!(snapshot.default_profile_p2, "profile-2");
    }

    #[test]
    fn runtime_config_store_builds_save_content_from_snapshot() {
        let store = RuntimeConfigStore::new();
        store.publish_runtime_state(RuntimeStateOptions {
            machine_default_noteskin: "cyber".to_string(),
            additional_song_folders: Vec::new(),
            never_cache_list: vec!["No Cache Pack".to_string()],
            ids: RuntimeStateIds {
                smx_p1_serial: Some("pad-1".to_string()),
                smx_p2_serial: None,
                default_profile_p1: Some("profile-1".to_string()),
                default_profile_p2: None,
            },
        });

        let content = store.save_content(&Keymap::default());
        assert!(content.contains("DefaultNoteSkin=cyber\n"));
        assert!(content.contains("NeverCacheList=No Cache Pack\n"));
        assert!(content.contains("SmxP1Serial=pad-1\n"));
        assert!(content.contains("DefaultLocalProfileIDP1=profile-1\n"));
    }

    #[test]
    fn runtime_config_store_mutators_report_changes() {
        let store = RuntimeConfigStore::new();

        assert!(store.set_smx_pad_assignment(Some("p1".to_string()), None));
        assert!(!store.set_smx_pad_assignment(Some("p1".to_string()), None));
        assert_eq!(store.smx_pad_assignment().0.as_deref(), Some("p1"));

        assert!(store.set_default_profiles(None, Some("profile-2".to_string())));
        assert!(!store.set_default_profiles(None, Some("profile-2".to_string())));
        assert_eq!(store.default_profiles().1.as_deref(), Some("profile-2"));

        assert!(store.set_machine_default_noteskin(" cyber "));
        assert!(!store.set_machine_default_noteskin("cyber"));
        assert_eq!(store.machine_default_noteskin(), "cyber");
    }

    #[test]
    fn runtime_config_store_publish_returns_effects() {
        let store = RuntimeConfigStore::new();
        let mut cfg = Config::default();
        cfg.master_volume = 80;
        cfg.enable_replaygain = true;
        cfg.rate_mod_preserves_pitch = false;
        cfg.center_image_translate_x = 1;
        cfg.center_image_translate_y = 2;
        cfg.center_image_add_width = 3;
        cfg.center_image_add_height = 4;
        cfg.log_to_file = false;

        let effects = store.publish_config(cfg);
        assert_eq!(store.config().master_volume, 80);
        assert_eq!(effects.audio_mix_levels.master_volume, 80);
        assert!(effects.replaygain_enabled);
        assert!(!effects.preserve_pitch_enabled);
        assert_eq!(effects.overscan, (1, 2, 3, 4));
        assert!(!effects.log_to_file);
    }

    #[test]
    fn runtime_config_store_loads_ini_state_and_config() {
        let store = RuntimeConfigStore::new();
        let effects = store.load_from_ini(
            &ini("[Options]\n\
DefaultNoteSkin= cyber \n\
MasterVolume=77\n\
ReplayGain=1\n\
SmxP1Serial= pad-1\n"),
            Config::default(),
        );

        assert_eq!(store.machine_default_noteskin(), "cyber");
        assert_eq!(store.smx_pad_assignment().0.as_deref(), Some("pad-1"));
        assert_eq!(store.config().master_volume, 77);
        assert_eq!(effects.audio_mix_levels.master_volume, 77);
        assert!(effects.replaygain_enabled);
    }

    #[test]
    fn runtime_config_store_updates_config_under_lock() {
        let store = RuntimeConfigStore::new();

        assert!(store.update_config(|cfg| {
            cfg.master_volume = 40;
            true
        }));
        assert_eq!(store.config().master_volume, 40);
        assert!(!store.update_config(|_| false));
    }

    #[test]
    fn runtime_config_store_updates_input_debounce_with_clamp() {
        let store = RuntimeConfigStore::new();

        assert_eq!(store.update_input_debounce_seconds(1.0), Some(0.2));
        assert_eq!(store.config().input_debounce_seconds, 0.2);
        assert_eq!(store.update_input_debounce_seconds(1.0), None);
    }

    #[test]
    fn runtime_config_store_updates_audio_mix_levels() {
        let store = RuntimeConfigStore::new();

        let levels = store
            .update_master_volume(40)
            .expect("volume should change");
        assert_eq!(levels.master_volume, 40);
        assert_eq!(levels.music_volume, Config::default().music_volume);
        assert_eq!(store.update_master_volume(40), None);

        let levels = store.update_music_volume(60).expect("volume should change");
        assert_eq!(levels.music_volume, 60);

        let levels = store.update_sfx_volume(30).expect("volume should change");
        assert_eq!(levels.sfx_volume, 30);

        let levels = store
            .update_assist_tick_volume(20)
            .expect("volume should change");
        assert_eq!(levels.assist_tick_volume, 20);
    }

    #[test]
    fn runtime_config_store_updates_audio_stream_flags() {
        let store = RuntimeConfigStore::new();

        assert!(store.update_enable_replaygain(true));
        assert!(store.config().enable_replaygain);
        assert!(!store.update_enable_replaygain(true));

        assert!(store.update_rate_mod_preserves_pitch(false));
        assert!(!store.config().rate_mod_preserves_pitch);
        assert!(!store.update_rate_mod_preserves_pitch(false));
    }

    #[test]
    fn runtime_config_store_updates_log_and_overscan_settings() {
        let store = RuntimeConfigStore::new();

        assert!(store.update_log_level(crate::theme::LogLevel::Info));
        assert_eq!(store.config().log_level, crate::theme::LogLevel::Info);
        assert!(!store.update_log_level(crate::theme::LogLevel::Info));

        assert!(store.update_log_to_file(!Config::default().log_to_file));
        assert_eq!(store.config().log_to_file, !Config::default().log_to_file);
        assert!(!store.update_log_to_file(!Config::default().log_to_file));

        assert!(store.update_overscan(1, 2, 3, 4));
        assert_eq!(
            (
                store.config().center_image_translate_x,
                store.config().center_image_translate_y,
                store.config().center_image_add_width,
                store.config().center_image_add_height,
            ),
            (1, 2, 3, 4)
        );
        assert!(!store.update_overscan(1, 2, 3, 4));
    }

    #[test]
    fn runtime_config_store_plans_smx_underglow_colors() {
        let store = RuntimeConfigStore::new();
        assert_eq!(store.smx_underglow_colors(false), None);

        store.update_config(|cfg| {
            cfg.smx_input = true;
            cfg.smx_underglow_theme = true;
            cfg.smx_underglow_grb = true;
            cfg.simply_love_color = 4;
            true
        });

        let plan = store
            .smx_underglow_colors(false)
            .expect("enabled underglow should produce colors");
        assert!(plan.grb);
        assert_eq!(plan.colors[0], Some(decorative_rgb(4)));
        assert_eq!(plan.colors[1], Some(decorative_rgb(2)));

        let lone_plan = store
            .smx_underglow_colors(true)
            .expect("enabled underglow should produce colors");
        assert_eq!(lone_plan.colors[0], lone_plan.colors[1]);

        assert!(store.update_simply_love_color(5));
        assert!(!store.update_simply_love_color(5));
    }

    #[test]
    fn runtime_config_store_updates_smx_machine_settings() {
        let store = RuntimeConfigStore::new();

        assert!(store.update_smx_input(!Config::default().smx_input));
        assert_eq!(store.config().smx_input, !Config::default().smx_input);
        assert!(!store.update_smx_input(!Config::default().smx_input));

        assert!(store.update_smx_manages_pad_config(true));
        assert!(store.config().smx_manages_pad_config);
        assert!(!store.update_smx_manages_pad_config(true));

        assert!(store.update_smx_panel_lights(!Config::default().smx_panel_lights));
        assert_eq!(
            store.config().smx_panel_lights,
            !Config::default().smx_panel_lights
        );
        assert!(!store.update_smx_panel_lights(!Config::default().smx_panel_lights));

        assert!(store.update_smx_underglow_theme(true));
        assert!(store.config().smx_underglow_theme);
        assert!(!store.update_smx_underglow_theme(true));

        assert!(store.update_smx_underglow_grb(!Config::default().smx_underglow_grb));
        assert_eq!(
            store.config().smx_underglow_grb,
            !Config::default().smx_underglow_grb
        );
        assert!(!store.update_smx_underglow_grb(!Config::default().smx_underglow_grb));
    }

    #[test]
    fn runtime_config_store_updates_smx_defaults() {
        let store = RuntimeConfigStore::new();

        assert!(store.update_smx_default_pad_config(deadsync_smx::SmxPadPreset::High));
        assert_eq!(
            store.config().smx_default_pad_config,
            deadsync_smx::SmxPadPreset::High
        );
        assert!(!store.update_smx_default_pad_config(deadsync_smx::SmxPadPreset::High));

        assert!(store.update_smx_default_light_brightness(42));
        assert_eq!(store.config().smx_default_light_brightness, 42);
        assert!(!store.update_smx_default_light_brightness(42));
    }

    #[test]
    fn runtime_config_store_updates_three_key_navigation() {
        let store = RuntimeConfigStore::new();

        let update = store.update_three_key_navigation(true, true);
        assert!(update.changed);
        assert_eq!(
            update.dedicated,
            DedicatedMenuNavigation {
                enabled: false,
                disabled_by_missing_bindings: false
            }
        );
        assert!(store.config().three_key_navigation);

        let update = store.update_three_key_navigation(true, true);
        assert!(!update.changed);
    }

    #[test]
    fn runtime_config_store_disables_unsupported_dedicated_on_menu_mode_change() {
        let store = RuntimeConfigStore::new();
        store.update_config(|cfg| {
            cfg.only_dedicated_menu_buttons = true;
            true
        });

        let update = store.update_three_key_navigation(true, false);
        assert!(update.changed);
        assert_eq!(
            update.dedicated,
            DedicatedMenuNavigation {
                enabled: false,
                disabled_by_missing_bindings: true
            }
        );
        let cfg = store.config();
        assert!(cfg.three_key_navigation);
        assert!(!cfg.only_dedicated_menu_buttons);
    }

    #[test]
    fn runtime_config_store_updates_dedicated_menu_buttons() {
        let store = RuntimeConfigStore::new();

        let update = store.update_only_dedicated_menu_buttons(true, true);
        assert!(update.changed);
        assert!(update.dedicated.enabled);
        assert!(store.config().only_dedicated_menu_buttons);

        let update = store.update_only_dedicated_menu_buttons(true, true);
        assert!(!update.changed);
        assert!(update.dedicated.enabled);
    }

    #[test]
    fn runtime_config_store_reports_unsupported_dedicated_request_without_change() {
        let store = RuntimeConfigStore::new();

        let update = store.update_only_dedicated_menu_buttons(true, false);
        assert!(!update.changed);
        assert_eq!(
            update.dedicated,
            DedicatedMenuNavigation {
                enabled: false,
                disabled_by_missing_bindings: true
            }
        );
        assert!(!update.three_key_navigation);
        assert!(!store.config().only_dedicated_menu_buttons);
    }

    #[test]
    fn runtime_config_store_applies_input_runtime_state() {
        let store = RuntimeConfigStore::new();
        {
            let mut cfg = store.lock_config();
            cfg.only_dedicated_menu_buttons = true;
            cfg.three_key_navigation = true;
            cfg.input_debounce_seconds = 0.05;
        }

        let state = store.apply_input_runtime_state(false);
        assert!(state.dedicated.disabled_by_missing_bindings);
        assert!(!state.dedicated.enabled);
        assert!(state.three_key_navigation);
        assert_eq!(state.input_debounce_seconds, 0.05);
        assert!(!store.config().only_dedicated_menu_buttons);
    }

    #[test]
    fn load_pad_order_entries_copies_options_section_entries() {
        let entries = load_pad_order_entries(&ini("[Options]\n\
PadOrderRawInput=1,0\n\
Unrelated=kept-for-native-filter\n"))
        .expect("options section should be present");

        assert!(entries.contains(&("PadOrderRawInput".to_string(), "1,0".to_string())));
        assert!(entries.contains(&(
            "Unrelated".to_string(),
            "kept-for-native-filter".to_string()
        )));
        assert_eq!(load_pad_order_entries(&ini("[Other]\nKey=Value\n")), None);
    }

    #[test]
    fn writes_runtime_state_id_option_lines() {
        let mut content = String::new();

        push_runtime_state_id_option_lines(
            &mut content,
            RuntimeStateIdTokens {
                smx_p1_serial: "p1",
                smx_p2_serial: "p2",
                default_profile_p1: "profile-a",
                default_profile_p2: "profile-b",
            },
        );

        assert_eq!(
            content,
            concat!(
                "SmxP1Serial=p1\n",
                "SmxP2Serial=p2\n",
                "DefaultLocalProfileIDP1=profile-a\n",
                "DefaultLocalProfileIDP2=profile-b\n",
            ),
        );
    }

    #[test]
    fn writes_pad_order_option_lines() {
        let mut content = String::new();

        push_pad_order_option_lines(
            &mut content,
            [("PadOrderRawInput", "0,1"), ("PadOrderSmx", "")],
        );

        assert_eq!(content, "PadOrderRawInput=0,1\nPadOrderSmx=\n");
    }
}
