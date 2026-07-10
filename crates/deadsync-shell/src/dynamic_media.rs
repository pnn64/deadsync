use deadlib_assets::dynamic;
use deadlib_render::TextureHandle;
use deadlib_renderer::{Backend, Texture as RendererTexture};
use deadlib_video as video;
use deadsync_assets::AssetManager;
use deadsync_assets::dynamic_media::{
    BackgroundTextureError, BannerVideoPrepResult, DynamicBackgroundState,
    DynamicImageTextureError, DynamicVideoState, GameplayBackgroundPrepResult,
    SongLuaVideoPrepResult, create_cdtitle_texture, create_inserted_banner_texture,
    dynamic_video_key_set, dynamic_video_path_in_set, path_texture_key, prepare_banner_video,
    prepare_gameplay_background, prepare_song_lua_video, replace_texture_key_set,
    retire_dynamic_background_state, retire_dynamic_video_state, retire_video_player,
    set_banner_texture_for_path, set_image_background_texture, set_video_background_poster_texture,
    set_video_background_texture, start_background_video,
};
use deadsync_assets::media_cache;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use log::warn;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};

struct DynamicBannerState {
    key: String,
    path: PathBuf,
}

pub struct DynamicMedia {
    current_dynamic_banner: Option<DynamicBannerState>,
    active_banner_videos: HashMap<String, DynamicVideoState>,
    pending_banner_video_preps: HashSet<PathBuf>,
    banner_video_prep_tx: mpsc::Sender<BannerVideoPrepResult>,
    banner_video_prep_rx: mpsc::Receiver<BannerVideoPrepResult>,
    current_dynamic_cdtitle: Option<(String, PathBuf)>,
    current_dynamic_pack_banner: Option<(String, PathBuf)>,
    dynamic_pack_banner_keys: std::collections::HashSet<String>,
    wheel_item_background_keys: HashSet<String>,
    current_dynamic_background: Option<DynamicBackgroundState>,
    active_song_lua_videos: HashMap<String, video::Player>,
    failed_song_lua_video_keys: HashSet<String>,
    gameplay_background_keys: HashSet<String>,
    pending_gameplay_background_preps: HashSet<String>,
    gameplay_background_prep_tx: mpsc::Sender<GameplayBackgroundPrepResult>,
    gameplay_background_prep_rx: mpsc::Receiver<GameplayBackgroundPrepResult>,
    failed_gameplay_background_key: Option<String>,
    current_profile_avatars: [Option<(String, PathBuf)>; 2],
    preloaded_profile_avatar_keys: HashSet<String>,
}

impl DynamicMedia {
    pub fn new() -> Self {
        let (banner_video_prep_tx, banner_video_prep_rx) = mpsc::channel();
        let (gameplay_background_prep_tx, gameplay_background_prep_rx) = mpsc::channel();
        Self {
            current_dynamic_banner: None,
            active_banner_videos: HashMap::new(),
            pending_banner_video_preps: HashSet::new(),
            banner_video_prep_tx,
            banner_video_prep_rx,
            current_dynamic_cdtitle: None,
            current_dynamic_pack_banner: None,
            dynamic_pack_banner_keys: std::collections::HashSet::new(),
            wheel_item_background_keys: HashSet::new(),
            current_dynamic_background: None,
            active_song_lua_videos: HashMap::new(),
            failed_song_lua_video_keys: HashSet::new(),
            gameplay_background_keys: HashSet::new(),
            pending_gameplay_background_preps: HashSet::new(),
            gameplay_background_prep_tx,
            gameplay_background_prep_rx,
            failed_gameplay_background_key: None,
            current_profile_avatars: std::array::from_fn(|_| None),
            preloaded_profile_avatar_keys: HashSet::new(),
        }
    }

    pub fn preload_profile_avatars(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        let profile = profile::get();
        for p in profile::scan_local_profiles() {
            if let Some(path) = p.avatar_path {
                media_cache::ensure_banner_texture(assets, backend, &path);
                self.preloaded_profile_avatar_keys
                    .insert(path.to_string_lossy().into_owned());
            }
        }
        self.set_profile_avatar(assets, backend, profile.avatar_path);
    }

    pub fn destroy_assets(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        let mut keys = Vec::with_capacity(
            self.active_banner_videos
                .len()
                .saturating_add(self.dynamic_pack_banner_keys.len())
                .saturating_add(self.wheel_item_background_keys.len())
                .saturating_add(self.active_song_lua_videos.len())
                .saturating_add(self.failed_song_lua_video_keys.len())
                .saturating_add(self.current_profile_avatars.len())
                .saturating_add(5),
        );
        if let Some(state) = self.current_dynamic_banner.take() {
            keys.push(state.key);
        }
        keys.extend(self.active_banner_videos.drain().map(|(key, state)| {
            retire_dynamic_video_state(state);
            key
        }));
        if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
            keys.push(key);
        }
        if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
            self.dynamic_pack_banner_keys.remove(&key);
            keys.push(key);
        }
        keys.extend(self.dynamic_pack_banner_keys.drain());
        keys.extend(self.wheel_item_background_keys.drain());
        if let Some(state) = self.current_dynamic_background.take() {
            keys.push(retire_dynamic_background_state(state));
        }
        keys.extend(self.active_song_lua_videos.drain().map(|(key, player)| {
            retire_video_player(player);
            key
        }));
        keys.extend(self.failed_song_lua_video_keys.drain());
        keys.extend(self.gameplay_background_keys.drain());
        self.pending_gameplay_background_preps.clear();
        self.failed_gameplay_background_key = None;
        self.clear_gameplay_background_results();
        for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
            let ix = profile_data::player_side_index(side);
            if let Some((key, _)) = self.current_profile_avatars[ix].take() {
                keys.push(key);
            }
            profile::set_avatar_texture_key_for_side(side, None);
        }
        for key in dynamic::dedupe_dynamic_keys(keys) {
            self.release_texture_key(assets, backend, key);
        }
    }

    pub fn destroy_banner(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        self.destroy_current_dynamic_banner(assets, backend);
    }

    pub fn set_cdtitle(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> Option<String> {
        if let Some(path) = path_opt {
            if let Some((key, current_path)) = self.current_dynamic_cdtitle.as_ref()
                && current_path == &path
                && assets.has_texture_key(key)
            {
                return Some(key.clone());
            }

            self.destroy_current_dynamic_cdtitle(assets, backend);
            match create_cdtitle_texture(assets, backend, &path) {
                Ok(key) => {
                    self.current_dynamic_cdtitle = Some((key.clone(), path));
                    Some(key)
                }
                Err(DynamicImageTextureError::Load(e)) => {
                    warn!(
                        "Failed to load CDTitle '{}': {e}. Skipping.",
                        path.display()
                    );
                    None
                }
                Err(DynamicImageTextureError::Create(e)) => {
                    warn!(
                        "Failed to create GPU texture for CDTitle image {path:?}: {e}. Skipping."
                    );
                    None
                }
            }
        } else {
            self.destroy_current_dynamic_cdtitle(assets, backend);
            None
        }
    }

    pub fn set_pack_banner(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) {
        let banner_cache_opts = media_cache::banner_cache_options();
        if let Some(path) = path_opt {
            if self
                .current_dynamic_pack_banner
                .as_ref()
                .is_some_and(|(key, p)| p == &path && assets.has_texture_key(key))
            {
                return;
            }

            let key = path_texture_key(&path);
            if banner_cache_opts.enabled
                && self.dynamic_pack_banner_keys.contains(&key)
                && assets.has_texture_key(&key)
            {
                self.current_dynamic_pack_banner = Some((key, path));
                return;
            }

            if banner_cache_opts.enabled {
                self.current_dynamic_pack_banner = None;
            } else if let Some((old_key, _)) = self.current_dynamic_pack_banner.take() {
                self.dynamic_pack_banner_keys.remove(&old_key);
                self.release_texture_key(assets, backend, old_key);
            }

            match create_inserted_banner_texture(assets, backend, &path) {
                Ok(key) => {
                    if banner_cache_opts.enabled {
                        self.dynamic_pack_banner_keys.insert(key.clone());
                    }
                    self.current_dynamic_pack_banner = Some((key, path));
                }
                Err(DynamicImageTextureError::Load(e)) => {
                    warn!(
                        "Failed to load pack banner '{}': {e}. Skipping.",
                        path.display()
                    );
                }
                Err(DynamicImageTextureError::Create(e)) => {
                    warn!("Failed to create GPU texture for pack banner {path:?}: {e}. Skipping.");
                }
            }
        } else if banner_cache_opts.enabled {
            self.current_dynamic_pack_banner = None;
        } else if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
            self.dynamic_pack_banner_keys.remove(&key);
            self.release_texture_key(assets, backend, key);
        }
    }

    pub fn set_wheel_item_backgrounds(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        paths: Vec<PathBuf>,
    ) {
        let mut desired = HashSet::with_capacity(paths.len());
        for path in paths {
            let key = path.to_string_lossy().into_owned();
            if desired.insert(key) {
                media_cache::ensure_banner_texture(assets, backend, &path);
            }
        }

        let release_keys = replace_texture_key_set(&mut self.wheel_item_background_keys, desired);
        for key in dynamic::dedupe_dynamic_keys(release_keys) {
            self.release_texture_key(assets, backend, key);
        }
    }

    pub fn set_banner(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "banner1.png";

        if let Some(path) = path_opt {
            let key = path_texture_key(&path);
            if let Some(current) = self.current_dynamic_banner.as_ref()
                && current.path == path
                && assets.has_texture_key(&current.key)
            {
                return current.key.clone();
            }
            self.destroy_current_dynamic_banner(assets, backend);
            match set_banner_texture_for_path(assets, backend, &path) {
                Ok(key) => {
                    self.current_dynamic_banner = Some(DynamicBannerState {
                        key: key.clone(),
                        path,
                    });
                    key
                }
                Err(DynamicImageTextureError::Load(e)) => {
                    warn!(
                        "Failed to load banner '{}': {e}. Using fallback.",
                        path.display()
                    );
                    FALLBACK_KEY.to_string()
                }
                Err(DynamicImageTextureError::Create(e)) => {
                    warn!(
                        "Failed to create GPU texture for banner '{}': {e}. Using fallback.",
                        key
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_banner(assets, backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn sync_active_banner_video(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_path: Option<&Path>,
    ) {
        let desired_path = desired_path.filter(|path| dynamic::is_dynamic_video_path(path));
        let stale_keys = self
            .active_banner_videos
            .iter()
            .filter(|(_, state)| Some(state.path.as_path()) != desired_path)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(state) = self.active_banner_videos.remove(&key) {
                retire_dynamic_video_state(state);
            }
            self.release_texture_key(assets, backend, key);
        }
        self.drain_banner_video_preps(assets, desired_path);
        let Some(path) = desired_path else {
            return;
        };
        if self
            .active_banner_videos
            .values()
            .any(|state| state.path.as_path() == path)
            || self.pending_banner_video_preps.contains(path)
        {
            return;
        }
        self.spawn_banner_video_prep(path);
    }

    pub fn sync_active_banner_videos(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_paths: &[PathBuf],
    ) {
        let stale_keys = self
            .active_banner_videos
            .iter()
            .filter(|(_, state)| !dynamic_video_path_in_set(&state.path, desired_paths))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(state) = self.active_banner_videos.remove(&key) {
                retire_dynamic_video_state(state);
            }
            self.release_texture_key(assets, backend, key);
        }
        self.drain_banner_video_preps_multi(assets, desired_paths);
        for path in desired_paths {
            if !dynamic::is_dynamic_video_path(path) {
                continue;
            }
            if self
                .active_banner_videos
                .values()
                .any(|state| state.path.as_path() == path.as_path())
                || self.pending_banner_video_preps.contains(path)
            {
                continue;
            }
            self.spawn_banner_video_prep(path);
        }
    }

    pub fn set_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
        video_started_at_sec: f32,
        animate_video: bool,
    ) -> String {
        const FALLBACK_KEY: &str = "__black";

        self.failed_gameplay_background_key = None;
        self.reset_pending_gameplay_background();

        if let Some(path) = path_opt {
            let key = path.to_string_lossy().into_owned();
            let wants_video = animate_video && dynamic::is_dynamic_video_path(&path);
            if self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| {
                    state.path == path
                        && assets.has_texture_key(&state.key)
                        && (state.video.is_some() == wants_video)
                })
            {
                return self
                    .current_dynamic_background
                    .as_ref()
                    .unwrap()
                    .key
                    .clone();
            }

            self.destroy_current_dynamic_background(assets, backend);

            if assets.has_texture_key(&key) {
                let video = if wants_video {
                    match start_background_video(&path) {
                        Ok(player) => Some(player),
                        Err(e) => {
                            warn!(
                                "Failed to start video background '{}': {e}. Using prewarmed poster.",
                                path.display()
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                self.current_dynamic_background = Some(DynamicBackgroundState::new(
                    key.clone(),
                    path,
                    video,
                    video_started_at_sec,
                    1.0,
                ));
                return key;
            }

            if dynamic::is_dynamic_video_path(&path) {
                if wants_video {
                    match set_video_background_texture(
                        assets,
                        backend,
                        &path,
                        video_started_at_sec,
                        1.0,
                    ) {
                        Ok((key, state)) => {
                            self.current_dynamic_background = Some(state);
                            return key;
                        }
                        Err(BackgroundTextureError::OpenVideo(e)) => {
                            warn!(
                                "Failed to open video background '{}': {e}. Using fallback.",
                                path.display()
                            );
                            return FALLBACK_KEY.to_string();
                        }
                        Err(BackgroundTextureError::CreateVideo(e)) => {
                            warn!(
                                "Failed to create GPU texture for video background {path:?}: {e}. Using fallback."
                            );
                            return FALLBACK_KEY.to_string();
                        }
                        Err(_) => unreachable!("video background helper returned wrong error kind"),
                    }
                }
                match set_video_background_poster_texture(
                    assets,
                    backend,
                    &path,
                    video_started_at_sec,
                    1.0,
                ) {
                    Ok((key, state)) => {
                        self.current_dynamic_background = Some(state);
                        return key;
                    }
                    Err(BackgroundTextureError::LoadPoster(e)) => {
                        warn!(
                            "Failed to load video background poster '{}': {e}. Using fallback.",
                            path.display()
                        );
                        return FALLBACK_KEY.to_string();
                    }
                    Err(BackgroundTextureError::CreatePoster(e)) => {
                        warn!(
                            "Failed to create GPU texture for video background poster {path:?}: {e}. Using fallback."
                        );
                        return FALLBACK_KEY.to_string();
                    }
                    Err(_) => unreachable!("video poster helper returned wrong error kind"),
                }
            }

            match set_image_background_texture(assets, backend, &path, video_started_at_sec, 1.0) {
                Ok((key, state)) => {
                    self.current_dynamic_background = Some(state);
                    key
                }
                Err(BackgroundTextureError::OpenImage(e)) => {
                    warn!("Failed to open background image {path:?}: {e}. Using fallback.");
                    FALLBACK_KEY.to_string()
                }
                Err(BackgroundTextureError::CreateImage(e)) => {
                    warn!(
                        "Failed to create GPU texture for background {path:?}: {e}. Using fallback."
                    );
                    FALLBACK_KEY.to_string()
                }
                Err(_) => unreachable!("image background helper returned wrong error kind"),
            }
        } else {
            self.destroy_current_dynamic_background(assets, backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn sync_gameplay_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_path: Option<&Path>,
        desired_key: Option<&str>,
        animate_video: bool,
        gameplay_time_sec: f32,
        video_rate: f32,
    ) -> Option<String> {
        const FALLBACK_KEY: &str = "__black";

        let Some(path) = desired_path else {
            self.failed_gameplay_background_key = None;
            self.reset_pending_gameplay_background();
            let had_background = self.current_dynamic_background.is_some();
            self.destroy_current_dynamic_background(assets, backend);
            return had_background.then(|| FALLBACK_KEY.to_string());
        };
        let desired_key = desired_key
            .map(Cow::Borrowed)
            .unwrap_or_else(|| path.to_string_lossy());
        let desired_key = desired_key.as_ref();
        if self.failed_gameplay_background_key.as_deref() != Some(desired_key) {
            self.failed_gameplay_background_key = None;
        }
        let wants_video = animate_video && dynamic::is_dynamic_video_path(path);
        let video_rate = dynamic::normalize_video_rate(video_rate);

        if wants_video {
            self.drain_gameplay_background_preps(
                assets,
                backend,
                desired_key,
                gameplay_time_sec,
                video_rate,
            );
        } else {
            self.reset_pending_gameplay_background();
        }

        if !assets.has_texture_key(desired_key) {
            if self.failed_gameplay_background_key.as_deref() != Some(desired_key) {
                warn!(
                    "Gameplay background '{}' was not prewarmed; using fallback.",
                    path.display()
                );
                self.failed_gameplay_background_key = Some(desired_key.to_owned());
                self.destroy_current_dynamic_background(assets, backend);
                return Some(FALLBACK_KEY.to_string());
            }
            return None;
        }

        let current_matches = self
            .current_dynamic_background
            .as_ref()
            .is_some_and(|state| {
                state.path == path
                    && state.key == desired_key
                    && (state.video.is_some() == wants_video)
                    && (!wants_video || (state.video_rate() - video_rate).abs() <= f32::EPSILON)
            });
        if current_matches {
            return None;
        }

        let current_path_matches = self
            .current_dynamic_background
            .as_ref()
            .is_some_and(|state| state.path == path && state.key == desired_key);
        if current_path_matches && !wants_video {
            if let Some(state) = self.current_dynamic_background.as_mut() {
                state.video = None;
            }
            return None;
        }
        if current_path_matches && wants_video {
            if let Some(state) = self.current_dynamic_background.as_mut()
                && (state.video_rate() - video_rate).abs() > f32::EPSILON
            {
                state.set_video_rate(video_rate, gameplay_time_sec);
            }
            let needs_video = self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| state.video.is_none());
            if needs_video
                && !self.pending_gameplay_background_preps.contains(desired_key)
                && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
            {
                self.spawn_gameplay_background_prep(path);
            }
            return None;
        }
        if !current_path_matches {
            self.destroy_current_dynamic_background(assets, backend);
            self.current_dynamic_background = Some(DynamicBackgroundState::new(
                desired_key.to_owned(),
                path.to_path_buf(),
                None,
                gameplay_time_sec,
                video_rate,
            ));
            if wants_video
                && !self.pending_gameplay_background_preps.contains(desired_key)
                && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
            {
                self.spawn_gameplay_background_prep(path);
            }
            return Some(desired_key.to_owned());
        }

        if wants_video
            && !self.pending_gameplay_background_preps.contains(desired_key)
            && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
        {
            self.spawn_gameplay_background_prep(path);
        }
        None
    }

    pub fn sync_active_song_lua_videos(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        paths: &[PathBuf],
    ) {
        let desired = dynamic_video_key_set(paths);
        let stale_active = self
            .active_song_lua_videos
            .keys()
            .filter(|key| !desired.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        let stale_failed = self
            .failed_song_lua_video_keys
            .iter()
            .filter(|key| !desired.contains(*key))
            .cloned()
            .collect::<Vec<_>>();

        for key in stale_active {
            if let Some(player) = self.active_song_lua_videos.remove(&key) {
                retire_video_player(player);
            }
            self.release_texture_key(assets, backend, key);
        }
        for key in stale_failed {
            self.failed_song_lua_video_keys.remove(&key);
            self.release_texture_key(assets, backend, key);
        }

        for path in paths {
            if !dynamic::is_dynamic_video_path(path) {
                continue;
            }
            let key = path.to_string_lossy().into_owned();
            if self.active_song_lua_videos.contains_key(&key)
                || self.failed_song_lua_video_keys.contains(&key)
            {
                continue;
            }
            match prepare_song_lua_video(path, !assets.has_texture_key(&key)) {
                SongLuaVideoPrepResult::Ready(prepared) => {
                    match prepared.poster {
                        Ok(Some(poster)) => {
                            assets.queue_texture_upload(prepared.key.clone(), poster)
                        }
                        Ok(None) => {}
                        Err(e) => warn!(
                            "Failed to load song lua video poster '{}': {e}",
                            path.display()
                        ),
                    }
                    self.active_song_lua_videos
                        .insert(prepared.key, prepared.player);
                }
                SongLuaVideoPrepResult::FailedOpen { key, msg } => {
                    warn!(
                        "Failed to start song lua video '{}': {msg}. Using prewarmed poster.",
                        path.display()
                    );
                    self.failed_song_lua_video_keys.insert(key);
                }
            }
        }
    }

    pub fn set_gameplay_background_keys<I>(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        keys: I,
    ) where
        I: IntoIterator<Item = String>,
    {
        let stale = replace_texture_key_set(
            &mut self.gameplay_background_keys,
            keys.into_iter().collect(),
        );
        for key in stale {
            self.release_texture_key(assets, backend, key);
        }
    }

    pub fn clear_gameplay_backgrounds(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        self.destroy_current_dynamic_background(assets, backend);
        for (key, player) in std::mem::take(&mut self.active_song_lua_videos) {
            retire_video_player(player);
            self.release_texture_key(assets, backend, key);
        }
        for key in std::mem::take(&mut self.failed_song_lua_video_keys) {
            self.release_texture_key(assets, backend, key);
        }
        self.reset_pending_gameplay_background();
        self.failed_gameplay_background_key = None;
        for key in std::mem::take(&mut self.gameplay_background_keys) {
            self.release_texture_key(assets, backend, key);
        }
    }

    pub fn set_profile_avatar(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) {
        let side = profile::get_session_player_side();
        self.set_profile_avatar_for_side(assets, backend, side, path_opt);
    }

    pub fn set_profile_avatar_for_side(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        side: profile_data::PlayerSide,
        path_opt: Option<PathBuf>,
    ) {
        let ix = profile_data::player_side_index(side);

        if let Some(path) = path_opt {
            if let Some((key, current_path)) = self.current_profile_avatars[ix].as_ref()
                && current_path == &path
                && assets.has_texture_key(key)
            {
                profile::set_avatar_texture_key_for_side(side, Some(key.clone()));
                return;
            }
            self.destroy_current_profile_avatar_for_side(assets, backend, side);
            let key = path.to_string_lossy().into_owned();
            media_cache::ensure_banner_texture(assets, backend, &path);
            self.current_profile_avatars[ix] = Some((key.clone(), path));
            if assets.has_texture_key(&key) {
                profile::set_avatar_texture_key_for_side(side, Some(key));
            } else {
                profile::set_avatar_texture_key_for_side(side, None);
            }
        } else {
            self.destroy_current_profile_avatar_for_side(assets, backend, side);
        }
    }

    pub fn queue_video_frames(
        &mut self,
        assets: &mut AssetManager,
        gameplay_time_sec: Option<f32>,
        ui_time_sec: f32,
    ) {
        for (key, video) in &mut self.active_banner_videos {
            if assets.has_pending_texture_upload(key) {
                continue;
            }
            let play_time = video
                .started_at
                .map_or(0.0, |start| start.elapsed().as_secs_f32());
            if let Some(frame) = video.player.take_due_frame(play_time) {
                video.started_at.get_or_insert_with(Instant::now);
                assets.queue_texture_upload(key.clone(), frame);
            }
        }

        if let Some(state) = self.current_dynamic_background.as_mut()
            && !assets.has_pending_texture_upload(&state.key)
        {
            let play_time = gameplay_time_sec.unwrap_or(ui_time_sec).max(0.0);
            let play_time = state.video_play_time(play_time);
            if let Some(video) = state.video.as_mut()
                && let Some(frame) = video.take_due_frame(play_time)
            {
                assets.queue_texture_upload(state.key.clone(), frame);
            }
        }

        let song_lua_play_time = gameplay_time_sec.unwrap_or(0.0).max(0.0);
        for (key, player) in &mut self.active_song_lua_videos {
            if assets.has_pending_texture_upload(key) {
                continue;
            }
            if let Some(frame) = player.take_due_frame(song_lua_play_time) {
                assets.queue_texture_upload(key.clone(), frame);
            }
        }
    }

    #[inline(always)]
    fn texture_key_in_use(&self, key: &str) -> bool {
        self.current_dynamic_banner
            .as_ref()
            .is_some_and(|state| state.key == key)
            || self.active_banner_videos.contains_key(key)
            || self
                .current_dynamic_cdtitle
                .as_ref()
                .is_some_and(|(owned, _)| owned == key)
            || self
                .current_dynamic_pack_banner
                .as_ref()
                .is_some_and(|(owned, _)| owned == key)
            || self.dynamic_pack_banner_keys.contains(key)
            || self.wheel_item_background_keys.contains(key)
            || self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| state.key == key)
            || self.active_song_lua_videos.contains_key(key)
            || self.failed_song_lua_video_keys.contains(key)
            || self.gameplay_background_keys.contains(key)
            || self
                .current_profile_avatars
                .iter()
                .flatten()
                .any(|(owned, _)| owned == key)
            || self.preloaded_profile_avatar_keys.contains(key)
    }

    #[inline(always)]
    fn take_releasable_texture(
        &mut self,
        assets: &mut AssetManager,
        key: &str,
    ) -> Option<(TextureHandle, RendererTexture)> {
        if self.texture_key_in_use(key) {
            None
        } else {
            assets.remove_texture(key)
        }
    }

    fn release_texture_key(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        key: String,
    ) {
        if let Some((handle, texture)) = self.take_releasable_texture(assets, &key) {
            assets.retire_texture(backend, handle, texture);
        }
    }

    fn destroy_current_dynamic_banner(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        if let Some(state) = self.current_dynamic_banner.take() {
            self.release_texture_key(assets, backend, state.key);
        }
    }

    fn destroy_current_dynamic_cdtitle(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) {
        if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
            self.release_texture_key(assets, backend, key);
        }
    }

    fn destroy_current_dynamic_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) {
        if let Some(state) = self.current_dynamic_background.take() {
            self.release_texture_key(assets, backend, retire_dynamic_background_state(state));
        }
    }

    fn reset_pending_gameplay_background(&mut self) {
        self.pending_gameplay_background_preps.clear();
        self.clear_gameplay_background_results();
    }

    fn spawn_banner_video_prep(&mut self, path: &Path) {
        if !self.pending_banner_video_preps.insert(path.to_path_buf()) {
            return;
        }

        let key = path.to_string_lossy().into_owned();
        let path = path.to_path_buf();
        let tx = self.banner_video_prep_tx.clone();
        thread::spawn(move || {
            let result = prepare_banner_video(key, path);
            let _ = tx.send(result);
        });
    }

    fn spawn_gameplay_background_prep(&mut self, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if !self.pending_gameplay_background_preps.insert(key.clone()) {
            return;
        }

        let path = path.to_path_buf();
        let tx = self.gameplay_background_prep_tx.clone();
        thread::spawn(move || {
            let result = prepare_gameplay_background(key, path);
            let _ = tx.send(result);
        });
    }

    fn drain_banner_video_preps(&mut self, assets: &mut AssetManager, desired_path: Option<&Path>) {
        while let Ok(result) = self.banner_video_prep_rx.try_recv() {
            match result {
                BannerVideoPrepResult::Ready(prepared) => {
                    self.pending_banner_video_preps.remove(&prepared.path);
                    if Some(prepared.path.as_path()) != desired_path {
                        retire_video_player(prepared.player);
                        continue;
                    }
                    assets.queue_texture_upload(prepared.key.clone(), prepared.poster);
                    if let Some(old) = self.active_banner_videos.insert(
                        prepared.key,
                        DynamicVideoState {
                            player: prepared.player,
                            started_at: None,
                            path: prepared.path,
                        },
                    ) {
                        retire_dynamic_video_state(old);
                    }
                }
                BannerVideoPrepResult::Failed { path, msg } => {
                    self.pending_banner_video_preps.remove(&path);
                    if Some(path.as_path()) == desired_path {
                        warn!("Failed to start banner video '{}': {msg}", path.display());
                    }
                }
            }
        }
    }

    fn drain_banner_video_preps_multi(
        &mut self,
        assets: &mut AssetManager,
        desired_paths: &[PathBuf],
    ) {
        while let Ok(result) = self.banner_video_prep_rx.try_recv() {
            match result {
                BannerVideoPrepResult::Ready(prepared) => {
                    self.pending_banner_video_preps.remove(&prepared.path);
                    if !desired_paths.iter().any(|path| {
                        dynamic::is_dynamic_video_path(path)
                            && path.as_path() == prepared.path.as_path()
                    }) {
                        retire_video_player(prepared.player);
                        continue;
                    }
                    assets.queue_texture_upload(prepared.key.clone(), prepared.poster);
                    if let Some(old) = self.active_banner_videos.insert(
                        prepared.key,
                        DynamicVideoState {
                            player: prepared.player,
                            started_at: None,
                            path: prepared.path,
                        },
                    ) {
                        retire_dynamic_video_state(old);
                    }
                }
                BannerVideoPrepResult::Failed { path, msg } => {
                    self.pending_banner_video_preps.remove(&path);
                    if desired_paths.iter().any(|desired| {
                        dynamic::is_dynamic_video_path(desired)
                            && desired.as_path() == path.as_path()
                    }) {
                        warn!("Failed to start banner video '{}': {msg}", path.display());
                    }
                }
            }
        }
    }

    fn drain_gameplay_background_preps(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_key: &str,
        gameplay_time_sec: f32,
        video_rate: f32,
    ) {
        while let Ok(result) = self.gameplay_background_prep_rx.try_recv() {
            match result {
                GameplayBackgroundPrepResult::Ready(prepared) => {
                    self.pending_gameplay_background_preps.remove(&prepared.key);
                    if prepared.key != desired_key {
                        retire_video_player(prepared.player);
                        continue;
                    }
                    self.failed_gameplay_background_key = None;
                    if let Some(state) = self.current_dynamic_background.as_mut()
                        && state.key == prepared.key
                        && state.path == prepared.path
                    {
                        state.restart_video(prepared.player, gameplay_time_sec);
                    } else {
                        if let Some(state) = self.current_dynamic_background.take() {
                            let key = retire_dynamic_background_state(state);
                            self.release_texture_key(assets, backend, key);
                        }
                        self.current_dynamic_background = Some(DynamicBackgroundState::new(
                            prepared.key,
                            prepared.path,
                            Some(prepared.player),
                            gameplay_time_sec,
                            video_rate,
                        ));
                    }
                }
                GameplayBackgroundPrepResult::Failed { key, path, msg } => {
                    self.pending_gameplay_background_preps.remove(&key);
                    if key != desired_key {
                        continue;
                    }
                    warn!(
                        "Failed to start gameplay background video '{}': {msg}. Keeping prewarmed poster.",
                        path.display()
                    );
                    self.failed_gameplay_background_key = Some(key);
                }
            }
        }
    }

    fn clear_gameplay_background_results(&mut self) {
        while let Ok(result) = self.gameplay_background_prep_rx.try_recv() {
            if let GameplayBackgroundPrepResult::Ready(prepared) = result {
                retire_video_player(prepared.player);
            }
        }
    }
    fn destroy_current_profile_avatar_for_side(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        side: profile_data::PlayerSide,
    ) {
        let ix = profile_data::player_side_index(side);
        let key = self.current_profile_avatars[ix].take().map(|(key, _)| key);
        profile::set_avatar_texture_key_for_side(side, None);
        if let Some(key) = key {
            self.release_texture_key(assets, backend, key);
        }
    }
}

impl Default for DynamicMedia {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_dynamic_key_stays_until_last_owner_releases_it() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "shared.mp4".to_string();
        let path = PathBuf::from(&key);

        assets.reserve_texture_handle(key.clone());
        media.current_dynamic_banner = Some(DynamicBannerState {
            key: key.clone(),
            path: path.clone(),
        });
        media.current_dynamic_background = Some(DynamicBackgroundState::new(
            key.clone(),
            path,
            None,
            0.0,
            1.0,
        ));

        media.current_dynamic_banner = None;
        let removed = media.take_releasable_texture(&mut assets, &key);

        assert!(removed.is_none());
        assert!(assets.has_texture_key(&key));
    }

    #[test]
    fn last_dynamic_owner_releases_texture_mapping() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "banner.mp4".to_string();
        let path = PathBuf::from(&key);

        assets.reserve_texture_handle(key.clone());
        media.current_dynamic_banner = Some(DynamicBannerState {
            key: key.clone(),
            path,
        });

        media.current_dynamic_banner = None;
        let removed = media.take_releasable_texture(&mut assets, &key);

        assert!(removed.is_none());
        assert!(!assets.has_texture_key(&key));
    }

    #[test]
    fn profile_avatar_counts_as_dynamic_texture_owner() {
        let mut media = DynamicMedia::new();
        let key = "avatar.png".to_string();
        media.current_profile_avatars[0] = Some((key.clone(), PathBuf::from(&key)));
        assert!(media.texture_key_in_use(&key));
    }

    #[test]
    fn gameplay_background_pool_counts_as_dynamic_texture_owner() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "queued-bg.mp4".to_string();

        assets.reserve_texture_handle(key.clone());
        media.gameplay_background_keys.insert(key.clone());

        let removed = media.take_releasable_texture(&mut assets, &key);

        assert!(removed.is_none());
        assert!(assets.has_texture_key(&key));
    }

    #[test]
    fn song_lua_video_key_counts_as_dynamic_texture_owner() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "overlay.avi".to_string();

        assets.reserve_texture_handle(key.clone());
        media.failed_song_lua_video_keys.insert(key.clone());

        let removed = media.take_releasable_texture(&mut assets, &key);

        assert!(removed.is_none());
        assert!(assets.has_texture_key(&key));
    }

    #[test]
    fn failed_banner_video_prep_clears_pending_key() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "banner.mp4".to_string();
        media.pending_banner_video_preps.insert(PathBuf::from(&key));
        media
            .banner_video_prep_tx
            .send(BannerVideoPrepResult::Failed {
                path: PathBuf::from(&key),
                msg: "failed".to_string(),
            })
            .unwrap();

        media.drain_banner_video_preps(&mut assets, Some(Path::new(&key)));

        assert!(!media.pending_banner_video_preps.contains(Path::new(&key)));
        assert!(!media.active_banner_videos.contains_key(&key));
    }
}
