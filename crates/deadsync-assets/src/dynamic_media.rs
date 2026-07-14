use crate::{AssetManager, media_cache, open_image_fallback, register_texture_dims};
use deadlib_assets::dynamic;
use deadlib_render::SamplerDesc;
use deadlib_renderer::Backend;
use deadlib_video as video;
use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use image::RgbaImage;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct DynamicVideoState {
    pub player: video::Player,
    pub started_at: Option<Instant>,
    pub path: PathBuf,
}

pub struct PreparedBannerVideo {
    pub key: String,
    pub path: PathBuf,
    pub poster: RgbaImage,
    pub player: video::Player,
}

pub enum BannerVideoPrepResult {
    Ready(PreparedBannerVideo),
    Failed { path: PathBuf, msg: String },
}

pub struct PreparedGameplayBackground {
    pub key: String,
    pub path: PathBuf,
    pub player: video::Player,
}

pub enum GameplayBackgroundPrepResult {
    Ready(PreparedGameplayBackground),
    Failed {
        key: String,
        path: PathBuf,
        msg: String,
    },
}

pub struct PreparedSongLuaVideo {
    pub key: String,
    pub player: video::Player,
    pub poster: Result<Option<RgbaImage>, String>,
}

pub enum SongLuaVideoPrepResult {
    Ready(PreparedSongLuaVideo),
    FailedOpen { key: String, msg: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicImageTextureError {
    Load(String),
    Create(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundTextureError {
    OpenVideo(String),
    CreateVideo(String),
    LoadPoster(String),
    CreatePoster(String),
    OpenImage(String),
    CreateImage(String),
}

pub fn cdtitle_texture_key(path: &Path) -> String {
    let path_key = path.to_string_lossy();
    format!("__cdtitle::{path_key}")
}

pub fn path_texture_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn texture_key_set<'a, I>(paths: I) -> HashSet<String>
where
    I: IntoIterator<Item = &'a PathBuf>,
{
    paths
        .into_iter()
        .map(|path| path_texture_key(path))
        .collect()
}

pub fn dynamic_video_key_set<'a, I>(paths: I) -> HashSet<String>
where
    I: IntoIterator<Item = &'a PathBuf>,
{
    paths
        .into_iter()
        .filter(|path| dynamic::is_dynamic_video_path(path))
        .map(|path| path_texture_key(path))
        .collect()
}

pub fn stale_texture_keys(current: &HashSet<String>, next: &HashSet<String>) -> Vec<String> {
    current.difference(next).cloned().collect()
}

pub fn replace_texture_key_set(
    current: &mut HashSet<String>,
    next: HashSet<String>,
) -> Vec<String> {
    let stale = stale_texture_keys(current, &next);
    *current = next;
    stale
}

pub fn dynamic_video_path_in_set(path: &Path, desired_paths: &[&Path]) -> bool {
    dynamic::is_dynamic_video_path(path)
        && desired_paths
            .iter()
            .any(|desired| path == *desired && dynamic::is_dynamic_video_path(desired))
}

pub fn push_gameplay_media_paths<'a>(
    out: &mut Vec<&'a PathBuf>,
    song: &'a SongData,
    gameplay_background_changes: &'a [SongBackgroundChange],
) {
    if let Some(path) = song.background_path.as_ref() {
        out.push(path);
    }
    for change in gameplay_background_changes {
        push_bgchange_paths(out, change);
    }
    for change in &song.background_layer2_changes {
        push_bgchange_paths(out, change);
    }
    for change in &song.foreground_changes {
        out.push(&change.path);
    }
}

pub fn gameplay_media_paths_capacity(
    song: &SongData,
    gameplay_background_changes: &[SongBackgroundChange],
) -> usize {
    1usize
        .saturating_add(gameplay_background_changes.len())
        .saturating_add(song.background_layer2_changes.len())
        .saturating_add(song.foreground_changes.len())
}

pub fn gameplay_media_keys(
    song: &SongData,
    gameplay_background_changes: &[SongBackgroundChange],
) -> Vec<String> {
    let mut keys = Vec::with_capacity(gameplay_media_paths_capacity(
        song,
        gameplay_background_changes,
    ));
    push_gameplay_media_keys(&mut keys, song, gameplay_background_changes);
    keys
}

pub fn push_gameplay_media_keys(
    out: &mut Vec<String>,
    song: &SongData,
    gameplay_background_changes: &[SongBackgroundChange],
) {
    if let Some(path) = song.background_path.as_ref() {
        out.push(path_texture_key(path));
    }
    for change in gameplay_background_changes {
        push_bgchange_keys(out, change);
    }
    for change in &song.background_layer2_changes {
        push_bgchange_keys(out, change);
    }
    for change in &song.foreground_changes {
        out.push(path_texture_key(&change.path));
    }
}

fn push_bgchange_paths<'a>(out: &mut Vec<&'a PathBuf>, change: &'a SongBackgroundChange) {
    if let SongBackgroundChangeTarget::File(path) = &change.target {
        out.push(path);
    }
    if let Some(path) = change.file2.as_ref() {
        out.push(path);
    }
}

fn push_bgchange_keys(out: &mut Vec<String>, change: &SongBackgroundChange) {
    if let SongBackgroundChangeTarget::File(path) = &change.target {
        out.push(path_texture_key(path));
    }
    if let Some(path) = change.file2.as_ref() {
        out.push(path_texture_key(path));
    }
}

pub fn create_cdtitle_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
) -> Result<String, DynamicImageTextureError> {
    let rgba =
        media_cache::load_cdtitle_source_rgba(path).map_err(DynamicImageTextureError::Load)?;
    let texture = backend
        .create_texture(&rgba, SamplerDesc::default())
        .map_err(|e| DynamicImageTextureError::Create(e.to_string()))?;
    let key = cdtitle_texture_key(path);
    assets.insert_texture(key.clone(), texture, rgba.width(), rgba.height());
    register_texture_dims(&key, rgba.width(), rgba.height());
    Ok(key)
}

pub fn create_inserted_banner_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
) -> Result<String, DynamicImageTextureError> {
    let rgba =
        media_cache::load_banner_source_rgba(path).map_err(DynamicImageTextureError::Load)?;
    let texture = backend
        .create_texture(&rgba, SamplerDesc::default())
        .map_err(|e| DynamicImageTextureError::Create(e.to_string()))?;
    let key = path_texture_key(path);
    assets.insert_texture(key.clone(), texture, rgba.width(), rgba.height());
    register_texture_dims(&key, rgba.width(), rgba.height());
    Ok(key)
}

pub fn set_banner_texture_for_path(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
) -> Result<String, DynamicImageTextureError> {
    let rgba =
        media_cache::load_banner_source_rgba(path).map_err(DynamicImageTextureError::Load)?;
    let texture = backend
        .create_texture(&rgba, SamplerDesc::default())
        .map_err(|e| DynamicImageTextureError::Create(e.to_string()))?;
    let key = path_texture_key(path);
    assets.set_texture_for_key(backend, key.clone(), texture, rgba.width(), rgba.height());
    register_texture_dims(&key, rgba.width(), rgba.height());
    Ok(key)
}

pub fn start_background_video(path: &Path) -> Result<video::Player, String> {
    video::open_player(path, true)
}

pub fn set_video_background_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
    video_started_at_sec: f32,
    video_rate: f32,
) -> Result<(String, DynamicBackgroundState), BackgroundTextureError> {
    let video = video::open(path, true).map_err(BackgroundTextureError::OpenVideo)?;
    let texture = backend
        .create_texture(&video.poster, SamplerDesc::default())
        .map_err(|e| BackgroundTextureError::CreateVideo(e.to_string()))?;
    let key = path_texture_key(path);
    assets.set_texture_for_key(
        backend,
        key.clone(),
        texture,
        video.info.width,
        video.info.height,
    );
    register_texture_dims(&key, video.info.width, video.info.height);
    let state = DynamicBackgroundState::new(
        key.clone(),
        path.to_path_buf(),
        Some(video.player),
        video_started_at_sec,
        video_rate,
    );
    Ok((key, state))
}

pub fn set_video_background_poster_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
    video_started_at_sec: f32,
    video_rate: f32,
) -> Result<(String, DynamicBackgroundState), BackgroundTextureError> {
    let rgba = video::load_poster(path).map_err(BackgroundTextureError::LoadPoster)?;
    let texture = backend
        .create_texture(&rgba, SamplerDesc::default())
        .map_err(|e| BackgroundTextureError::CreatePoster(e.to_string()))?;
    let key = path_texture_key(path);
    assets.set_texture_for_key(backend, key.clone(), texture, rgba.width(), rgba.height());
    register_texture_dims(&key, rgba.width(), rgba.height());
    let state = DynamicBackgroundState::new(
        key.clone(),
        path.to_path_buf(),
        None,
        video_started_at_sec,
        video_rate,
    );
    Ok((key, state))
}

pub fn set_image_background_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    path: &Path,
    video_started_at_sec: f32,
    video_rate: f32,
) -> Result<(String, DynamicBackgroundState), BackgroundTextureError> {
    let rgba = open_image_fallback(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| BackgroundTextureError::OpenImage(e.to_string()))?;
    let texture = backend
        .create_texture(&rgba, SamplerDesc::default())
        .map_err(|e| BackgroundTextureError::CreateImage(e.to_string()))?;
    let key = path_texture_key(path);
    assets.set_texture_for_key(backend, key.clone(), texture, rgba.width(), rgba.height());
    register_texture_dims(&key, rgba.width(), rgba.height());
    let state = DynamicBackgroundState::new(
        key.clone(),
        path.to_path_buf(),
        None,
        video_started_at_sec,
        video_rate,
    );
    Ok((key, state))
}

pub struct DynamicBackgroundState {
    pub key: String,
    pub path: PathBuf,
    pub video: Option<video::Player>,
    video_timing: dynamic::DynamicVideoTiming,
}

impl DynamicBackgroundState {
    pub fn new(
        key: String,
        path: PathBuf,
        video: Option<video::Player>,
        gameplay_time_sec: f32,
        video_rate: f32,
    ) -> Self {
        Self {
            key,
            path,
            video,
            video_timing: dynamic::DynamicVideoTiming::new(gameplay_time_sec, video_rate),
        }
    }

    pub fn video_play_time(&self, gameplay_time_sec: f32) -> f32 {
        self.video_timing.play_time(gameplay_time_sec)
    }

    pub fn set_video_rate(&mut self, video_rate: f32, gameplay_time_sec: f32) {
        self.video_timing.set_rate(video_rate, gameplay_time_sec);
    }

    pub fn restart_video(&mut self, player: video::Player, gameplay_time_sec: f32) {
        if let Some(old) = self.video.replace(player) {
            retire_video_player(old);
        }
        self.video_timing.restart(gameplay_time_sec);
    }

    pub fn video_rate(&self) -> f32 {
        self.video_timing.rate()
    }
}

pub fn prepare_banner_video(key: String, path: PathBuf) -> BannerVideoPrepResult {
    if !media_cache::banner_cache_options().enabled {
        return match video::open(&path, true) {
            Ok(video) => BannerVideoPrepResult::Ready(PreparedBannerVideo {
                key,
                path,
                poster: video.poster,
                player: video.player,
            }),
            Err(msg) => BannerVideoPrepResult::Failed { path, msg },
        };
    }

    let poster = match media_cache::load_banner_source_rgba(&path) {
        Ok(rgba) => rgba,
        Err(msg) => {
            return BannerVideoPrepResult::Failed { path, msg };
        }
    };
    let player = match video::open_player(&path, true) {
        Ok(player) => player,
        Err(msg) => {
            return BannerVideoPrepResult::Failed { path, msg };
        }
    };
    BannerVideoPrepResult::Ready(PreparedBannerVideo {
        key,
        path,
        poster,
        player,
    })
}

pub fn prepare_gameplay_background(key: String, path: PathBuf) -> GameplayBackgroundPrepResult {
    match video::open_player(&path, true) {
        Ok(player) => {
            GameplayBackgroundPrepResult::Ready(PreparedGameplayBackground { key, path, player })
        }
        Err(msg) => GameplayBackgroundPrepResult::Failed { key, path, msg },
    }
}

pub fn prepare_song_lua_video(path: &Path, load_poster: bool) -> SongLuaVideoPrepResult {
    let key = path_texture_key(path);
    let player = match video::open_player(path, true) {
        Ok(player) => player,
        Err(msg) => return SongLuaVideoPrepResult::FailedOpen { key, msg },
    };
    let poster = if load_poster {
        video::load_poster(path).map(Some)
    } else {
        Ok(None)
    };
    SongLuaVideoPrepResult::Ready(PreparedSongLuaVideo {
        key,
        player,
        poster,
    })
}

pub fn retire_video_player(player: video::Player) {
    player.retire_async();
}

pub fn retire_video_player_opt(player: Option<video::Player>) {
    if let Some(player) = player {
        retire_video_player(player);
    }
}

pub fn retire_dynamic_background_state(mut state: DynamicBackgroundState) -> String {
    retire_video_player_opt(state.video.take());
    state.key
}

pub fn retire_dynamic_video_state(state: DynamicVideoState) {
    retire_video_player(state.player);
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{SongBackgroundChange, SongForegroundChange};

    #[test]
    fn gameplay_media_keys_include_song_and_bgchange_paths() {
        let mut song = test_song();
        song.background_path = Some(PathBuf::from("base.png"));
        song.background_layer2_changes.push({
            let mut change = SongBackgroundChange::new(
                8.0,
                SongBackgroundChangeTarget::File("layer.png".into()),
            );
            change.file2 = Some("layer2.png".into());
            change
        });
        song.foreground_changes.push(SongForegroundChange {
            start_beat: 16.0,
            path: "fg.png".into(),
        });
        let gameplay_changes = vec![{
            let mut change =
                SongBackgroundChange::new(4.0, SongBackgroundChangeTarget::File("game.png".into()));
            change.file2 = Some("game2.png".into());
            change
        }];

        let keys = gameplay_media_keys(&song, &gameplay_changes);

        assert_eq!(
            keys,
            vec![
                "base.png",
                "game.png",
                "game2.png",
                "layer.png",
                "layer2.png",
                "fg.png",
            ]
        );
    }

    #[test]
    fn gameplay_media_paths_match_key_order() {
        let mut song = test_song();
        song.background_path = Some(PathBuf::from("base.png"));
        song.foreground_changes.push(SongForegroundChange {
            start_beat: 16.0,
            path: "fg.png".into(),
        });
        let gameplay_changes = vec![SongBackgroundChange::new(
            4.0,
            SongBackgroundChangeTarget::File("game.png".into()),
        )];
        let mut paths = Vec::new();

        push_gameplay_media_paths(&mut paths, &song, &gameplay_changes);

        let keys: Vec<_> = paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(keys, vec!["base.png", "game.png", "fg.png"]);
    }

    #[test]
    fn texture_key_set_dedupes_paths() {
        let paths = [
            PathBuf::from("a.png"),
            PathBuf::from("b.png"),
            PathBuf::from("a.png"),
        ];
        let keys = texture_key_set(paths.iter());

        assert_eq!(keys.len(), 2);
        assert!(keys.contains("a.png"));
        assert!(keys.contains("b.png"));
    }

    #[test]
    fn dynamic_video_key_set_filters_static_images() {
        let paths = [
            PathBuf::from("bg.png"),
            PathBuf::from("movie.mp4"),
            PathBuf::from("clip.avi"),
        ];
        let keys = dynamic_video_key_set(paths.iter());

        assert_eq!(keys.len(), 2);
        assert!(!keys.contains("bg.png"));
        assert!(keys.contains("movie.mp4"));
        assert!(keys.contains("clip.avi"));
    }

    #[test]
    fn replace_texture_key_set_returns_stale_keys() {
        let mut current = ["a.png".to_string(), "b.png".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();
        let next = ["b.png".to_string(), "c.png".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();

        let mut stale = replace_texture_key_set(&mut current, next);
        stale.sort();

        assert_eq!(stale, ["a.png".to_string()]);
        assert!(current.contains("b.png"));
        assert!(current.contains("c.png"));
        assert!(!current.contains("a.png"));
    }

    #[test]
    fn dynamic_video_path_in_set_requires_video_path_match() {
        let desired = [PathBuf::from("movie.mp4"), PathBuf::from("still.png")];
        let desired = desired.each_ref().map(PathBuf::as_path);

        assert!(dynamic_video_path_in_set(Path::new("movie.mp4"), &desired));
        assert!(!dynamic_video_path_in_set(Path::new("still.png"), &desired));
        assert!(!dynamic_video_path_in_set(Path::new("other.mp4"), &desired));
    }

    fn test_song() -> SongData {
        SongData {
            simfile_path: PathBuf::from("Songs/Test/test.ssc"),
            title: "Test".to_string(),
            subtitle: String::new(),
            translit_title: "Test".to_string(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }
}
