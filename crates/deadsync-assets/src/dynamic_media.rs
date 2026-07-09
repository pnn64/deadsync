use crate::{AssetManager, media_cache, open_image_fallback, register_texture_dims};
use deadlib_assets::dynamic;
use deadlib_render::SamplerDesc;
use deadlib_renderer::Backend;
use deadlib_video as video;
use image::RgbaImage;
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
