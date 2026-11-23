use deadsync_audio_stream::{AudioControl, SfxId};
use deadsync_config as config;
use deadsync_theme::views::{AudioOptionsView, AudioOutputDeviceView};
use deadsync_theme::{
    AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, ThemeSounds, UiCue, UiSound,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Application-thread handles: fixed cues live for the session; content cues
/// live for the screen. Preparation sizes the maps before input/update runs.
/// Each map is bounded by its preparation path list, with no runtime insertions.
/// Playback only borrows handles; missing/failed sounds are silent. Clearing
/// screen handles happens on exit; the audio cache retains the sample data.
pub(super) struct UiSfx {
    fixed: HashMap<&'static str, SfxId>,
    cues: [Option<SfxId>; UiCue::ALL.len()],
    pub(super) screen: HashMap<PathBuf, SfxId>,
}

impl UiSfx {
    pub fn prepare(
        audio: &mut AudioControl,
        fallback: ThemeSounds,
        installed: ThemeSounds,
    ) -> Self {
        Self::prepare_sounds(fallback, installed, |sound| match sound {
            UiSound::File(path) => audio.prepare_sfx(&deadsync_assets::resolve_asset_path(path)),
            UiSound::EmbeddedWav { name, bytes } => audio.prepare_embedded_sfx(name, bytes),
        })
    }

    fn prepare_sounds(
        fallback: ThemeSounds,
        installed: ThemeSounds,
        mut load: impl FnMut(UiSound) -> Option<SfxId>,
    ) -> Self {
        let mut files = HashMap::new();
        let mut prepare = |sound| match sound {
            UiSound::File(path) => files.entry(path).or_insert_with(|| load(sound)).clone(),
            UiSound::EmbeddedWav { .. } => load(sound),
        };
        let cues = UiCue::ALL.map(|cue| {
            installed
                .cue(cue)
                .and_then(&mut prepare)
                .or_else(|| fallback.cue(cue).and_then(&mut prepare))
        });
        let fixed = fallback
            .effects
            .iter()
            .chain(installed.effects)
            .filter_map(|&path| {
                let sound = match fallback
                    .cue_for_path(path)
                    .or_else(|| installed.cue_for_path(path))
                {
                    Some(cue) => cues[cue.index()].clone(),
                    None => prepare(UiSound::File(path)),
                };
                sound.map(|sound| (path, sound))
            })
            .collect();
        Self {
            fixed,
            cues,
            screen: HashMap::new(),
        }
    }

    pub fn prepare_screen<'a>(
        &mut self,
        audio: &mut AudioControl,
        paths: impl Iterator<Item = &'a Path>,
    ) {
        self.screen = paths
            .filter_map(|path| {
                audio
                    .prepare_sfx(&deadsync_assets::resolve_asset_path(
                        path.to_string_lossy().as_ref(),
                    ))
                    .map(|sound| (path.to_owned(), sound))
            })
            .collect();
    }

    pub fn play(&self, audio: &mut AudioControl, path: &str) {
        if let Some(sound) = self.fixed.get(path) {
            audio.play_sfx(sound);
        }
    }

    pub fn play_cue(&self, audio: &mut AudioControl, cue: UiCue) {
        if let Some(sound) = &self.cues[cue.index()] {
            audio.play_sfx(sound);
        }
    }
}

pub(super) fn options_view(audio: &deadsync_audio_stream::AudioControl) -> AudioOptionsView {
    let cfg = config::runtime::get();
    let output_devices = audio
        .startup_output_devices()
        .iter()
        .map(|device| AudioOutputDeviceView {
            name: device.name.clone(),
            is_default: device.is_default,
            sample_rates_hz: device.sample_rates_hz.clone(),
        })
        .collect();
    #[cfg(target_os = "linux")]
    let (available_backend_names, selected_backend_name) = (
        deadsync_audio_stream::available_linux_backends()
            .into_iter()
            .map(|backend| backend.as_str().to_owned())
            .collect(),
        cfg.linux_audio_backend.as_str().to_owned(),
    );
    #[cfg(not(target_os = "linux"))]
    let (available_backend_names, selected_backend_name) = (Vec::new(), String::new());

    AudioOptionsView {
        output_devices,
        available_backend_names,
        output_device: cfg.audio_output_device_index,
        output_mode: output_mode_choice(cfg.audio_output_mode),
        selected_backend_name,
        sample_rate_hz: cfg.audio_sample_rate_hz,
        preserve_pitch: cfg.rate_mod_preserves_pitch,
        replay_gain: cfg.enable_replaygain,
        master_volume: cfg.master_volume,
        music_volume: cfg.music_volume,
        sfx_volume: cfg.sfx_volume,
        assist_tick_volume: cfg.assist_tick_volume,
    }
}

const fn output_mode_choice(mode: deadlib_audio_core::AudioOutputMode) -> AudioOutputModeChoice {
    match mode {
        deadlib_audio_core::AudioOutputMode::Auto => AudioOutputModeChoice::Auto,
        deadlib_audio_core::AudioOutputMode::Shared => AudioOutputModeChoice::Shared,
        deadlib_audio_core::AudioOutputMode::Exclusive => AudioOutputModeChoice::Exclusive,
    }
}

const fn output_mode(choice: AudioOutputModeChoice) -> deadlib_audio_core::AudioOutputMode {
    match choice {
        AudioOutputModeChoice::Auto => deadlib_audio_core::AudioOutputMode::Auto,
        AudioOutputModeChoice::Shared => deadlib_audio_core::AudioOutputMode::Shared,
        AudioOutputModeChoice::Exclusive => deadlib_audio_core::AudioOutputMode::Exclusive,
    }
}

#[cfg(target_os = "linux")]
fn linux_backend(name: &str) -> deadlib_audio::LinuxAudioBackend {
    match name {
        "PipeWire" => deadlib_audio::LinuxAudioBackend::PipeWire,
        "PulseAudio" => deadlib_audio::LinuxAudioBackend::PulseAudio,
        "JACK" => deadlib_audio::LinuxAudioBackend::Jack,
        "ALSA" => deadlib_audio::LinuxAudioBackend::Alsa,
        _ => deadlib_audio::LinuxAudioBackend::Auto,
    }
}

pub(super) fn execute(audio: &mut AudioControl, sounds: &UiSfx, request: AudioRequest) {
    match request {
        AudioRequest::PlaySfx(path) => sounds.play(audio, path),
        AudioRequest::PlayUiCue(cue) => sounds.play_cue(audio, cue),
        AudioRequest::PlayScreenSfxPath(path) => {
            if let Some(sound) = sounds.screen.get(&path) {
                audio.play_screen_sfx(sound);
            }
        }
        AudioRequest::PlayMusic {
            path,
            cut,
            looping,
            rate,
        } => audio.play_music(
            path,
            deadlib_audio::stream::Cut {
                start_sec: cut.start_sec,
                length_sec: cut.length_sec,
                fade_in_sec: cut.fade_in_sec,
                fade_out_sec: cut.fade_out_sec,
            },
            looping,
            rate,
        ),
        AudioRequest::StopMusic => audio.stop_music(),
        AudioRequest::SetMusicRate(rate) => audio.set_music_rate(rate),
        AudioRequest::SetVolume { target, percent } => match target {
            AudioVolumeTarget::Master => config::runtime_update::update_master_volume(percent),
            AudioVolumeTarget::Music => config::runtime_update::update_music_volume(percent),
            AudioVolumeTarget::Sfx => config::runtime_update::update_sfx_volume(percent),
            AudioVolumeTarget::AssistTick => {
                config::runtime_update::update_assist_tick_volume(percent)
            }
        },
        AudioRequest::SetOutputDevice(device) => {
            config::runtime_update::update_audio_output_device(device)
        }
        AudioRequest::SetOutputMode(mode) => {
            config::runtime_update::update_audio_output_mode(output_mode(mode))
        }
        AudioRequest::SetOutputBackend(name) => {
            #[cfg(target_os = "linux")]
            config::runtime_update::update_linux_audio_backend(linux_backend(&name));
            #[cfg(not(target_os = "linux"))]
            drop(name);
        }
        AudioRequest::SetSampleRate(rate) => config::runtime_update::update_audio_sample_rate(rate),
        AudioRequest::SetMineHitSound(enabled) => {
            config::runtime_update::update_mine_hit_sound(enabled)
        }
        AudioRequest::SetGlobalOffsetMillis(milliseconds) => {
            config::runtime_update::update_global_offset(milliseconds as f32 / 1000.0);
        }
        AudioRequest::SetPreservePitch(enabled) => {
            config::runtime_update::update_rate_mod_preserves_pitch(enabled);
            audio.set_preserve_pitch_enabled(enabled);
        }
        AudioRequest::SetReplayGain(enabled) => {
            config::runtime_update::update_enable_replaygain(enabled);
            audio.set_replaygain_enabled(enabled);
        }
        AudioRequest::PrewarmReplayGain(paths) => deadsync_audio_replaygain::prewarm_paths(
            paths,
            deadsync_audio_replaygain::Priority::Background,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_theme::{Theme, ThemeAssetManifest, ThemeResources, ThemeScreenId};

    struct AlternateTheme;

    impl Theme for AlternateTheme {
        type Screen = ();
        type RuntimeRequest = ();

        fn screen_id(_: Self::Screen) -> ThemeScreenId {
            ThemeScreenId::new("alternate")
        }
    }

    impl ThemeResources for AlternateTheme {
        const SOUNDS: ThemeSounds = ThemeSounds {
            effects: &["alternate/menu.ogg"],
            cues: &[
                (UiCue::Change, UiSound::File("alternate/move.ogg")),
                (UiCue::Confirm, UiSound::File("alternate/menu.ogg")),
            ],
        };

        fn asset_manifest()
        -> ThemeAssetManifest<impl Iterator<Item = deadlib_assets::TextureAssetSpec>> {
            ThemeAssetManifest {
                fonts: &[],
                textures: std::iter::once(deadlib_assets::texture_asset("alternate/tile.png")),
                embedded_pngs: &[],
                texture_needs_repeat_sampler: |key| key == "alternate/tile.png",
            }
        }
    }

    #[test]
    fn alternate_theme_supplies_shell_cues_and_renderer_manifest() {
        let mut paths = Vec::new();
        UiSfx::prepare_sounds(ThemeSounds::default(), AlternateTheme::SOUNDS, |sound| {
            let UiSound::File(path) = sound else {
                panic!("expected file fixture")
            };
            paths.push(path);
            None
        });
        assert_eq!(paths, ["alternate/move.ogg", "alternate/menu.ogg"]);

        let root = tempfile::tempdir().unwrap();
        let dirs = deadsync_config::dirs::AppDirs {
            cache_dir: root.path().join("cache"),
            data_dir: root.path().join("data"),
            exe_dir: root.path().to_owned(),
            portable: false,
        };
        let manifest = AlternateTheme::asset_manifest();
        let jobs = deadsync_assets::textures::initial_texture_jobs(
            manifest.textures,
            &dirs.asset_paths(None),
            manifest.texture_needs_repeat_sampler,
        );
        let [job] = jobs.as_slice() else {
            panic!("expected only the alternate tile");
        };
        assert_eq!(job.key, "alternate/tile.png");
        assert_eq!(job.sampler.wrap, deadlib_render_core::SamplerWrap::Repeat);
    }

    #[test]
    fn openitg_embedded_cues_decode_like_wav_files_at_device_rates() {
        use deadlib_audio::stream::{
            OutputFormat, load_and_resample_sfx, load_and_resample_wav_sfx,
        };
        use deadsync_theme_openitg::OpenItgTheme;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("cue.wav");
        let mut input_samples = Vec::new();
        for cue in UiCue::ALL
            .into_iter()
            .filter(|cue| *cue != UiCue::Screenshot)
        {
            let Some(UiSound::EmbeddedWav { bytes, .. }) = OpenItgTheme::SOUNDS.cue(cue) else {
                panic!("{cue:?} should own embedded audio");
            };
            assert!(!input_samples.contains(&bytes));
            input_samples.push(bytes);
            std::fs::write(&path, bytes).unwrap();
            for (sample_rate_hz, channels) in [(24_000, 1), (44_100, 2), (48_000, 2)] {
                let format = OutputFormat {
                    sample_rate_hz,
                    channels,
                };
                let embedded = load_and_resample_wav_sfx(bytes, format).unwrap();
                let file = load_and_resample_sfx(&path, format, Default::default()).unwrap();
                assert_eq!(
                    embedded.as_ref(),
                    file.as_ref(),
                    "{cue:?} at {sample_rate_hz}"
                );
                assert!(embedded.iter().any(|&sample| sample.abs() > 500));
                if cue == UiCue::StageFailed && channels == 2 {
                    assert!(embedded.chunks_exact(2).any(|frame| frame[0] != frame[1]));
                } else {
                    for frame in embedded.chunks_exact(channels) {
                        assert!(frame.iter().all(|&sample| sample == frame[0]));
                    }
                }
            }
        }
    }

    #[test]
    fn audio_output_modes_round_trip_through_shell_mapping() {
        for choice in [
            AudioOutputModeChoice::Auto,
            AudioOutputModeChoice::Shared,
            AudioOutputModeChoice::Exclusive,
        ] {
            assert_eq!(output_mode_choice(output_mode(choice)), choice);
        }
    }
}
