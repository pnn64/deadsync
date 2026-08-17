//! Deterministic launch contract for opt-in shipping-path performance cases.
//!
//! The manifest is parsed before configuration or profile state is loaded so a
//! case can install an isolated data root. Execution remains App-owned: the
//! regular renderer, audio runtime, Gameplay initialization, frame loop, and
//! presentation path are used unchanged.

use deadsync_config::prelude as config;
use deadsync_profile::{PlayStyle, PlayerSide};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const LIVE_CASE_SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct BuildIdentity {
    pub version: &'static str,
    pub hash: &'static str,
    pub stamp: &'static str,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedRuntime {
    pub renderer: String,
    pub vsync: bool,
    pub max_fps: u16,
    pub present_mode: String,
    pub display_mode: String,
    pub display_monitor: usize,
    pub display_name: String,
    pub display_width: u32,
    pub display_height: u32,
    pub display_refresh_millihertz: u32,
    pub audio_device_index: Option<u16>,
    pub audio_output_mode: String,
    pub audio_sample_rate_hz: Option<u32>,
    pub audio_backend: String,
    pub audio_fallback_from_native: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    name: String,
    data_dir: PathBuf,
    artifact_dir: PathBuf,
    config_sha256: String,
    simfile: PathBuf,
    simfile_sha256: String,
    music_sha256: String,
    play_style: String,
    player_side: String,
    joined: [bool; 2],
    chart_difficulties: [String; 2],
    chart_hashes: [String; 2],
    music_rate: f32,
    autoplay: bool,
    warmup_frames: u32,
    measured_frames: u32,
    expected_runtime: ExpectedRuntime,
}

#[derive(Clone, Debug)]
pub struct LiveCase {
    pub(crate) build: BuildIdentity,
    pub(crate) name: String,
    pub(crate) manifest_path: PathBuf,
    pub(crate) manifest_sha256: String,
    pub(crate) data_dir: PathBuf,
    pub(crate) artifact_dir: PathBuf,
    pub(crate) config_sha256: String,
    pub(crate) simfile: PathBuf,
    pub(crate) simfile_sha256: String,
    pub(crate) music_sha256: String,
    pub(crate) play_style: PlayStyle,
    pub(crate) player_side: PlayerSide,
    pub(crate) joined: [bool; 2],
    pub(crate) chart_difficulties: [String; 2],
    pub(crate) chart_hashes: [String; 2],
    pub(crate) music_rate: f32,
    pub(crate) autoplay: bool,
    pub(crate) warmup_frames: u32,
    pub(crate) measured_frames: u32,
    pub(crate) expected_runtime: ExpectedRuntime,
}

impl LiveCase {
    /// Parse the optional `--perf-case <manifest.json>` argument from argv that
    /// was not consumed by the updater driver. Relative paths are resolved
    /// against the process launch directory, before startup changes cwd.
    pub fn from_args(args: &[String], build: BuildIdentity) -> Result<Option<Self>, String> {
        let mut path = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--perf-case" {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("--perf-case requires a manifest path")?;
                if path.replace(PathBuf::from(value)).is_some() {
                    return Err("--perf-case may be supplied only once".to_owned());
                }
            } else if let Some(value) = arg.strip_prefix("--perf-case=") {
                if value.is_empty() {
                    return Err("--perf-case requires a manifest path".to_owned());
                }
                if path.replace(PathBuf::from(value)).is_some() {
                    return Err("--perf-case may be supplied only once".to_owned());
                }
            }
            index += 1;
        }
        path.map(|path| Self::load(path, build)).transpose()
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn validate_config(&self, actual: &config::Config) -> Result<(), String> {
        let expected = &self.expected_runtime;
        let actual_renderer = actual.video_renderer.to_string();
        let actual_present = actual.present_mode_policy.as_str();
        let actual_audio_mode = actual.audio_output_mode.as_str();
        let actual_display_mode = match actual.display_mode() {
            config::DisplayMode::Windowed => "Windowed",
            config::DisplayMode::Fullscreen(kind) => kind.as_str(),
        };
        let matches = actual_renderer.eq_ignore_ascii_case(expected.renderer.trim())
            && actual.vsync == expected.vsync
            && actual.max_fps == expected.max_fps
            && actual_present.eq_ignore_ascii_case(expected.present_mode.trim())
            && actual_display_mode.eq_ignore_ascii_case(expected.display_mode.trim())
            && actual.display_monitor == expected.display_monitor
            && actual.display_width == expected.display_width
            && actual.display_height == expected.display_height
            && actual.audio_output_device_index == expected.audio_device_index
            && actual_audio_mode.eq_ignore_ascii_case(expected.audio_output_mode.trim())
            && actual.audio_sample_rate_hz == expected.audio_sample_rate_hz;
        if matches {
            return Ok(());
        }
        Err(format!(
            "performance case '{}' runtime mismatch: expected renderer={} vsync={} max_fps={} present={} display_mode={} monitor={} display={}x{} audio_device={:?} audio_mode={} audio_rate={:?}; actual renderer={} vsync={} max_fps={} present={} display_mode={} monitor={} display={}x{} audio_device={:?} audio_mode={} audio_rate={:?}",
            self.name,
            expected.renderer,
            expected.vsync,
            expected.max_fps,
            expected.present_mode,
            expected.display_mode,
            expected.display_monitor,
            expected.display_width,
            expected.display_height,
            expected.audio_device_index,
            expected.audio_output_mode,
            expected.audio_sample_rate_hz,
            actual_renderer,
            actual.vsync,
            actual.max_fps,
            actual_present,
            actual_display_mode,
            actual.display_monitor,
            actual.display_width,
            actual.display_height,
            actual.audio_output_device_index,
            actual_audio_mode,
            actual.audio_sample_rate_hz,
        ))
    }

    fn load(path: PathBuf, build: BuildIdentity) -> Result<Self, String> {
        let launch_dir = std::env::current_dir()
            .map_err(|error| format!("cannot resolve launch directory: {error}"))?;
        let path = absolute_from(&launch_dir, path)
            .canonicalize()
            .map_err(|error| format!("cannot open performance case manifest: {error}"))?;
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid performance case '{}': {error}", path.display()))?;
        validate_manifest(&manifest)?;
        let root = path
            .parent()
            .expect("canonical manifest must have a parent");
        let data_dir = absolute_from(root, manifest.data_dir)
            .canonicalize()
            .map_err(|error| format!("cannot resolve case data_dir: {error}"))?;
        if !data_dir.is_dir() {
            return Err(format!(
                "case data_dir is not a directory: '{}'",
                data_dir.display()
            ));
        }
        let config_path = data_dir.join("deadsync.ini");
        if !config_path.is_file() {
            return Err(format!(
                "case data_dir must contain an explicit deadsync.ini: '{}'",
                config_path.display()
            ));
        }
        let config_sha256 = sha256_file(&config_path)?;
        verify_hash("config", &manifest.config_sha256, &config_sha256)?;
        let simfile = absolute_from(root, manifest.simfile)
            .canonicalize()
            .map_err(|error| format!("cannot resolve case simfile: {error}"))?;
        let simfile_sha256 = sha256_file(&simfile)?;
        verify_hash("simfile", &manifest.simfile_sha256, &simfile_sha256)?;
        let artifact_dir = absolute_from(root, manifest.artifact_dir);
        let play_style = parse_play_style(&manifest.play_style)?;
        let player_side = parse_player_side(&manifest.player_side)?;
        validate_players(play_style, player_side, manifest.joined)?;
        Ok(Self {
            build,
            name: manifest.name,
            manifest_sha256: hex_digest(&bytes),
            manifest_path: path,
            data_dir,
            artifact_dir,
            config_sha256,
            simfile,
            simfile_sha256,
            music_sha256: manifest.music_sha256.to_ascii_lowercase(),
            play_style,
            player_side,
            joined: manifest.joined,
            chart_difficulties: manifest.chart_difficulties,
            chart_hashes: manifest.chart_hashes.map(|hash| hash.to_ascii_lowercase()),
            music_rate: manifest.music_rate,
            autoplay: manifest.autoplay,
            warmup_frames: manifest.warmup_frames,
            measured_frames: manifest.measured_frames,
            expected_runtime: manifest.expected_runtime,
        })
    }
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != LIVE_CASE_SCHEMA {
        return Err(format!(
            "unsupported performance case schema {}; expected {LIVE_CASE_SCHEMA}",
            manifest.schema
        ));
    }
    if manifest.name.is_empty()
        || !manifest
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("case name must contain only ASCII letters, digits, '-' or '_'".to_owned());
    }
    if !manifest.music_rate.is_finite() || !(0.5..=3.0).contains(&manifest.music_rate) {
        return Err("music_rate must be finite and between 0.5 and 3.0".to_owned());
    }
    if !manifest.autoplay {
        return Err("schema 1 performance cases require autoplay=true".to_owned());
    }
    if !(64..=100_000).contains(&manifest.warmup_frames) {
        return Err("warmup_frames must be between 64 and 100000".to_owned());
    }
    if !(100..=100_000).contains(&manifest.measured_frames) {
        return Err("measured_frames must be between 100 and 100000".to_owned());
    }
    validate_hash("simfile_sha256", &manifest.simfile_sha256)?;
    validate_hash("music_sha256", &manifest.music_sha256)?;
    validate_hash("config_sha256", &manifest.config_sha256)?;
    if manifest
        .chart_difficulties
        .iter()
        .any(|difficulty| difficulty.trim().is_empty())
    {
        return Err("chart_difficulties entries may not be empty".to_owned());
    }
    if manifest
        .chart_hashes
        .iter()
        .any(|hash| hash.len() != 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("chart_hashes entries must be 16-digit chart hashes".to_owned());
    }
    let runtime = &manifest.expected_runtime;
    if [
        runtime.renderer.as_str(),
        runtime.present_mode.as_str(),
        runtime.display_mode.as_str(),
        runtime.display_name.as_str(),
        runtime.audio_output_mode.as_str(),
        runtime.audio_backend.as_str(),
    ]
    .into_iter()
    .any(|value| value.trim().is_empty())
    {
        return Err("expected_runtime identity fields may not be empty".to_owned());
    }
    if runtime.display_width == 0
        || runtime.display_height == 0
        || runtime.display_refresh_millihertz == 0
    {
        return Err("expected_runtime display dimensions and refresh must be nonzero".to_owned());
    }
    Ok(())
}

fn validate_players(style: PlayStyle, side: PlayerSide, joined: [bool; 2]) -> Result<(), String> {
    if matches!(style, PlayStyle::Versus | PlayStyle::PumpVersus) {
        return (joined == [true, true])
            .then_some(())
            .ok_or_else(|| "versus cases require joined=[true,true]".to_owned());
    }
    let expected = match side {
        PlayerSide::P1 => [true, false],
        PlayerSide::P2 => [false, true],
    };
    (joined == expected).then_some(()).ok_or_else(|| {
        format!("single-player cases require joined={expected:?} for player_side={side:?}")
    })
}

fn parse_play_style(value: &str) -> Result<PlayStyle, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "single" | "dance-single" => Ok(PlayStyle::Single),
        "versus" | "dance-versus" => Ok(PlayStyle::Versus),
        "double" | "dance-double" => Ok(PlayStyle::Double),
        "pump-single" => Ok(PlayStyle::PumpSingle),
        "pump-versus" => Ok(PlayStyle::PumpVersus),
        "pump-double" => Ok(PlayStyle::PumpDouble),
        _ => Err(format!("unsupported play_style '{value}'")),
    }
}

fn parse_player_side(value: &str) -> Result<PlayerSide, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "p1" => Ok(PlayerSide::P1),
        "p2" => Ok(PlayerSide::P2),
        _ => Err(format!("unsupported player_side '{value}'")),
    }
}

fn validate_hash(label: &str, hash: &str) -> Result<(), String> {
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("{label} must be a 64-digit SHA-256 value"))
    }
}

pub(crate) fn verify_hash(label: &str, expected: &str, actual: &str) -> Result<(), String> {
    if expected.eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(format!(
            "{label} SHA-256 mismatch: expected {}, got {actual}",
            expected.to_ascii_lowercase()
        ))
    }
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open '{}' for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash '{}': {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(digest_hex(hasher.finalize().as_ref()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    digest_hex(hasher.finalize().as_ref())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

fn absolute_from(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_case_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "deadsync-live-case-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time must follow the unix epoch")
                .as_nanos()
        ))
    }

    const TEST_BUILD: BuildIdentity = BuildIdentity {
        version: "1.2.3",
        hash: "test-hash",
        stamp: "test-stamp",
    };

    #[test]
    fn argv_accepts_only_one_case_path() {
        assert!(
            LiveCase::from_args(&["--console".to_owned()], TEST_BUILD)
                .unwrap()
                .is_none()
        );
        let error = LiveCase::from_args(
            &[
                "--perf-case=a.json".to_owned(),
                "--perf-case".to_owned(),
                "b.json".to_owned(),
            ],
            TEST_BUILD,
        )
        .unwrap_err();
        assert!(error.contains("only once"));
    }

    #[test]
    fn player_shape_is_explicit_for_every_style() {
        assert!(validate_players(PlayStyle::Versus, PlayerSide::P1, [true, true]).is_ok());
        assert!(validate_players(PlayStyle::Single, PlayerSide::P2, [false, true]).is_ok());
        assert!(validate_players(PlayStyle::Double, PlayerSide::P1, [true, true]).is_err());
    }

    #[test]
    fn hashes_require_the_full_sha256_domain() {
        assert!(validate_hash("fixture", &"a".repeat(64)).is_ok());
        assert!(validate_hash("fixture", &"g".repeat(64)).is_err());
        assert!(validate_hash("fixture", &"a".repeat(63)).is_err());
    }

    #[test]
    fn manifest_resolves_fixture_paths_and_normalizes_hashes() {
        let root = temp_case_root();
        std::fs::create_dir_all(root.join("data")).expect("create case data root");
        std::fs::write(root.join("data/deadsync.ini"), b"[Options]\n").expect("write case config");
        std::fs::write(root.join("chart.ssc"), b"fixture").expect("write case simfile");
        let fixture_hash = hex_digest(b"fixture");
        let config_hash = hex_digest(b"[Options]\n");
        let manifest = serde_json::json!({
            "schema": LIVE_CASE_SCHEMA,
            "name": "unit-case",
            "data_dir": "data",
            "artifact_dir": "artifacts",
            "config_sha256": config_hash,
            "simfile": "chart.ssc",
            "simfile_sha256": fixture_hash.to_ascii_uppercase(),
            "music_sha256": "a".repeat(64),
            "play_style": "single",
            "player_side": "p1",
            "joined": [true, false],
            "chart_difficulties": ["Challenge", "Challenge"],
            "chart_hashes": ["ABCDEF0123456789", "ABCDEF0123456789"],
            "music_rate": 1.0,
            "autoplay": true,
            "warmup_frames": 64,
            "measured_frames": 100,
            "expected_runtime": {
                "renderer": "OpenGL",
                "vsync": false,
                "max_fps": 0,
                "present_mode": "mailbox",
                "display_mode": "Windowed",
                "display_monitor": 0,
                "display_name": "Test monitor",
                "display_width": 1280,
                "display_height": 720,
                "display_refresh_millihertz": 60000,
                "audio_device_index": null,
                "audio_output_mode": "Auto",
                "audio_sample_rate_hz": null,
                "audio_backend": "wasapi-shared",
                "audio_fallback_from_native": false
            }
        });
        let manifest_path = root.join("case.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("encode case manifest"),
        )
        .expect("write case manifest");

        let case = LiveCase::load(manifest_path, TEST_BUILD).expect("load valid case");
        assert_eq!(case.data_dir, root.join("data").canonicalize().unwrap());
        assert_eq!(case.simfile_sha256, fixture_hash);
        assert_eq!(case.chart_hashes, ["abcdef0123456789"; 2]);

        std::fs::remove_dir_all(root).expect("remove case fixture");
    }
}
