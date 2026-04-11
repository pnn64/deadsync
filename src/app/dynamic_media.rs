use super::media_cache;
use crate::assets::{AssetManager, dynamic, open_image_fallback, register_texture_dims};
use crate::engine::{
    gfx::{Backend, SamplerDesc, Texture as GfxTexture, TextureHandle},
    video,
};
use crate::game::profile;
use image::RgbaImage;
use log::warn;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};

struct DynamicVideoState {
    player: video::Player,
    started_at: Instant,
}

struct PreparedBannerVideo {
    key: String,
    poster: RgbaImage,
    player: video::Player,
}

enum BannerVideoPrepResult {
    Ready(PreparedBannerVideo),
    Failed {
        key: String,
        path: PathBuf,
        msg: String,
    },
}

struct PreparedGameplayBackground {
    key: String,
    path: PathBuf,
    image: RgbaImage,
    video: Option<video::Player>,
}

enum GameplayBackgroundPrepResult {
    Ready(PreparedGameplayBackground),
    Failed {
        key: String,
        path: PathBuf,
        msg: String,
    },
}

struct DynamicBannerState {
    key: String,
    path: PathBuf,
}

struct DynamicBackgroundState {
    key: String,
    path: PathBuf,
    video: Option<video::Player>,
}

pub(crate) struct DynamicMedia {
    current_dynamic_banner: Option<DynamicBannerState>,
    active_banner_videos: HashMap<String, DynamicVideoState>,
    pending_banner_video_preps: HashSet<String>,
    banner_video_prep_tx: mpsc::Sender<BannerVideoPrepResult>,
    banner_video_prep_rx: mpsc::Receiver<BannerVideoPrepResult>,
    current_dynamic_cdtitle: Option<(String, PathBuf)>,
    current_dynamic_pack_banner: Option<(String, PathBuf)>,
    dynamic_pack_banner_keys: std::collections::HashSet<String>,
    current_dynamic_background: Option<DynamicBackgroundState>,
    pending_gameplay_background_preps: HashSet<String>,
    gameplay_background_prep_tx: mpsc::Sender<GameplayBackgroundPrepResult>,
    gameplay_background_prep_rx: mpsc::Receiver<GameplayBackgroundPrepResult>,
    queued_gameplay_background: Option<DynamicBackgroundState>,
    failed_gameplay_background_key: Option<String>,
    current_profile_avatars: [Option<(String, PathBuf)>; 2],
}

impl DynamicMedia {
    pub(crate) fn new() -> Self {
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
            current_dynamic_background: None,
            pending_gameplay_background_preps: HashSet::new(),
            gameplay_background_prep_tx,
            gameplay_background_prep_rx,
            queued_gameplay_background: None,
            failed_gameplay_background_key: None,
            current_profile_avatars: std::array::from_fn(|_| None),
        }
    }

    pub(crate) fn preload_profile_avatars(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) {
        let profile = profile::get();
        for p in profile::scan_local_profiles() {
            if let Some(path) = p.avatar_path {
                media_cache::ensure_banner_texture(assets, backend, &path);
            }
        }
        self.set_profile_avatar(assets, backend, profile.avatar_path);
    }

    pub(crate) fn destroy_assets(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        let mut keys = Vec::with_capacity(
            self.active_banner_videos
                .len()
                .saturating_add(self.dynamic_pack_banner_keys.len())
                .saturating_add(self.current_profile_avatars.len())
                .saturating_add(5),
        );
        if let Some(state) = self.current_dynamic_banner.take() {
            keys.push(state.key);
        }
        keys.extend(self.active_banner_videos.drain().map(|(key, _)| key));
        if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
            keys.push(key);
        }
        if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
            self.dynamic_pack_banner_keys.remove(&key);
            keys.push(key);
        }
        keys.extend(self.dynamic_pack_banner_keys.drain());
        if let Some(state) = self.current_dynamic_background.take() {
            keys.push(state.key);
        }
        if let Some(state) = self.queued_gameplay_background.take() {
            keys.push(state.key);
        }
        self.pending_gameplay_background_preps.clear();
        self.failed_gameplay_background_key = None;
        self.clear_gameplay_background_results();
        for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
            let ix = Self::side_ix(side);
            if let Some((key, _)) = self.current_profile_avatars[ix].take() {
                keys.push(key);
            }
            profile::set_avatar_texture_key_for_side(side, None);
        }
        for key in dynamic::dedupe_dynamic_keys(keys) {
            self.release_texture_key(assets, backend, key);
        }
    }

    pub(crate) fn destroy_banner(&mut self, assets: &mut AssetManager, backend: &mut Backend) {
        self.destroy_current_dynamic_banner(assets, backend);
    }

    pub(crate) fn set_cdtitle(
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
            let rgba = match media_cache::load_cdtitle_source_rgba(&path) {
                Ok(rgba) => rgba,
                Err(e) => {
                    warn!(
                        "Failed to load CDTitle '{}': {e}. Skipping.",
                        path.display()
                    );
                    return None;
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    let path_key = path.to_string_lossy();
                    let key = format!("__cdtitle::{path_key}");
                    assets.insert_texture(key.clone(), texture, rgba.width(), rgba.height());
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_cdtitle = Some((key.clone(), path));
                    Some(key)
                }
                Err(e) => {
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

    pub(crate) fn set_pack_banner(
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

            let key = path.to_string_lossy().into_owned();
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

            let rgba = match media_cache::load_banner_source_rgba(&path) {
                Ok(rgba) => rgba,
                Err(e) => {
                    warn!(
                        "Failed to load pack banner '{}': {e}. Skipping.",
                        path.display()
                    );
                    return;
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    assets.insert_texture(key.clone(), texture, rgba.width(), rgba.height());
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    if banner_cache_opts.enabled {
                        self.dynamic_pack_banner_keys.insert(key.clone());
                    }
                    self.current_dynamic_pack_banner = Some((key, path));
                }
                Err(e) => {
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

    pub(crate) fn set_banner(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "banner1.png";

        if let Some(path) = path_opt {
            let key = path.to_string_lossy().into_owned();
            if let Some(current) = self.current_dynamic_banner.as_ref()
                && current.path == path
                && assets.has_texture_key(&current.key)
            {
                return current.key.clone();
            }
            self.destroy_current_dynamic_banner(assets, backend);
            let rgba = match media_cache::load_banner_source_rgba(&path) {
                Ok(rgba) => rgba,
                Err(e) => {
                    warn!(
                        "Failed to load banner '{}': {e}. Using fallback.",
                        path.display()
                    );
                    return FALLBACK_KEY.to_string();
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    assets.set_texture_for_key(
                        backend,
                        key.clone(),
                        texture,
                        rgba.width(),
                        rgba.height(),
                    );
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_banner = Some(DynamicBannerState {
                        key: key.clone(),
                        path,
                    });
                    key
                }
                Err(e) => {
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

    pub(crate) fn sync_active_banner_videos(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_paths: &[PathBuf],
    ) {
        let mut desired = HashSet::<String>::with_capacity(desired_paths.len());
        for path in desired_paths {
            if !dynamic::is_dynamic_video_path(path) {
                continue;
            }
            desired.insert(path.to_string_lossy().into_owned());
        }
        let stale_keys =
            dynamic::collect_stale_dynamic_keys(self.active_banner_videos.keys(), &desired);
        for key in stale_keys {
            self.active_banner_videos.remove(&key);
            self.release_texture_key(assets, backend, key);
        }
        self.drain_banner_video_preps(assets, &desired);
        for path in desired_paths {
            if !dynamic::is_dynamic_video_path(path) {
                continue;
            }
            let key = path.to_string_lossy().into_owned();
            if self.active_banner_videos.contains_key(&key)
                || self.pending_banner_video_preps.contains(&key)
            {
                continue;
            }
            self.spawn_banner_video_prep(path);
        }
    }

    pub(crate) fn set_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "__black";

        self.reset_pending_gameplay_background(assets, backend);

        if let Some(path) = path_opt {
            let animate_video = crate::config::get().show_video_backgrounds;
            if self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| {
                    state.path == path
                        && assets.has_texture_key(&state.key)
                        && (state.video.is_some()
                            == (animate_video && dynamic::is_dynamic_video_path(&path)))
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

            if dynamic::is_dynamic_video_path(&path) {
                let key = path.to_string_lossy().into_owned();
                if animate_video {
                    match video::open(&path, true) {
                        Ok(video) => match backend
                            .create_texture(&video.poster, SamplerDesc::default())
                        {
                            Ok(texture) => {
                                assets.set_texture_for_key(
                                    backend,
                                    key.clone(),
                                    texture,
                                    video.info.width,
                                    video.info.height,
                                );
                                register_texture_dims(&key, video.info.width, video.info.height);
                                self.current_dynamic_background = Some(DynamicBackgroundState {
                                    key: key.clone(),
                                    path,
                                    video: Some(video.player),
                                });
                                return key;
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to create GPU texture for video background {path:?}: {e}. Using fallback."
                                );
                                return FALLBACK_KEY.to_string();
                            }
                        },
                        Err(e) => {
                            warn!(
                                "Failed to open video background '{}': {e}. Using fallback.",
                                path.display()
                            );
                            return FALLBACK_KEY.to_string();
                        }
                    }
                }
                match video::load_poster(&path) {
                    Ok(rgba) => match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            assets.set_texture_for_key(
                                backend,
                                key.clone(),
                                texture,
                                rgba.width(),
                                rgba.height(),
                            );
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_background = Some(DynamicBackgroundState {
                                key: key.clone(),
                                path,
                                video: None,
                            });
                            return key;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for video background poster {path:?}: {e}. Using fallback."
                            );
                            return FALLBACK_KEY.to_string();
                        }
                    },
                    Err(e) => {
                        warn!(
                            "Failed to load video background poster '{}': {e}. Using fallback.",
                            path.display()
                        );
                        return FALLBACK_KEY.to_string();
                    }
                }
            }

            let rgba = match open_image_fallback(&path) {
                Ok(img) => img.to_rgba8(),
                Err(e) => {
                    warn!("Failed to open background image {path:?}: {e}. Using fallback.");
                    return FALLBACK_KEY.to_string();
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    let key = path.to_string_lossy().into_owned();
                    assets.set_texture_for_key(
                        backend,
                        key.clone(),
                        texture,
                        rgba.width(),
                        rgba.height(),
                    );
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_background = Some(DynamicBackgroundState {
                        key: key.clone(),
                        path,
                        video: None,
                    });
                    key
                }
                Err(e) => {
                    warn!(
                        "Failed to create GPU texture for background {path:?}: {e}. Using fallback."
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_background(assets, backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub(crate) fn sync_gameplay_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_path: Option<&Path>,
        animate_video: bool,
    ) -> Option<String> {
        const FALLBACK_KEY: &str = "__black";

        let desired_key = desired_path.map(|path| path.to_string_lossy().into_owned());
        if self.failed_gameplay_background_key.as_deref() != desired_key.as_deref() {
            self.failed_gameplay_background_key = None;
        }

        if desired_key.is_none() {
            self.reset_pending_gameplay_background(assets, backend);
            self.destroy_current_dynamic_background(assets, backend);
            return Some(FALLBACK_KEY.to_string());
        }
        let desired_key = desired_key.unwrap();

        let failed = self.drain_gameplay_background_preps(assets, backend, &desired_key);
        if failed {
            self.reset_pending_gameplay_background(assets, backend);
            self.failed_gameplay_background_key = Some(desired_key.clone());
            self.destroy_current_dynamic_background(assets, backend);
            return Some(FALLBACK_KEY.to_string());
        }

        self.drop_stale_queued_gameplay_background(assets, backend, &desired_key);
        if self.promote_queued_gameplay_background(assets, backend, &desired_key) {
            return Some(desired_key);
        }

        if self
            .current_dynamic_background
            .as_ref()
            .is_some_and(|state| {
                state.key == desired_key && assets.has_uploaded_texture_key(&state.key)
            })
            || self
                .pending_gameplay_background_preps
                .contains(&desired_key)
            || self
                .queued_gameplay_background
                .as_ref()
                .is_some_and(|state| state.key == desired_key)
            || self.failed_gameplay_background_key.as_deref() == Some(desired_key.as_str())
        {
            return None;
        }

        self.spawn_gameplay_background_prep(desired_path.unwrap(), animate_video);
        None
    }

    pub(crate) fn set_profile_avatar(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) {
        let side = profile::get_session_player_side();
        self.set_profile_avatar_for_side(assets, backend, side, path_opt);
    }

    pub(crate) fn set_profile_avatar_for_side(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        side: profile::PlayerSide,
        path_opt: Option<PathBuf>,
    ) {
        let ix = Self::side_ix(side);

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

    pub(crate) fn queue_video_frames(
        &mut self,
        assets: &mut AssetManager,
        gameplay_time_sec: Option<f32>,
    ) {
        let banner_frames: Vec<_> = self
            .active_banner_videos
            .iter_mut()
            .filter_map(|(key, video)| {
                let play_time = video.started_at.elapsed().as_secs_f32();
                video
                    .player
                    .take_due_frame(play_time)
                    .map(|frame| (key.clone(), frame))
            })
            .collect();
        for (key, frame) in banner_frames {
            assets.queue_texture_upload(key, frame);
        }

        let background_frame = self.current_dynamic_background.as_mut().and_then(|state| {
            let video = state.video.as_mut()?;
            let play_time = gameplay_time_sec.unwrap_or(0.0).max(0.0);
            video
                .take_due_frame(play_time)
                .map(|frame| (state.key.clone(), frame))
        });
        if let Some((key, frame)) = background_frame {
            assets.queue_texture_upload(key, frame);
        }
    }

    #[inline(always)]
    fn side_ix(side: profile::PlayerSide) -> usize {
        match side {
            profile::PlayerSide::P1 => 0,
            profile::PlayerSide::P2 => 1,
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
            || self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| state.key == key)
            || self
                .queued_gameplay_background
                .as_ref()
                .is_some_and(|state| state.key == key)
            || self
                .current_profile_avatars
                .iter()
                .flatten()
                .any(|(owned, _)| owned == key)
    }

    #[inline(always)]
    fn take_releasable_texture(
        &mut self,
        assets: &mut AssetManager,
        key: &str,
    ) -> Option<(TextureHandle, GfxTexture)> {
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
            self.release_texture_key(assets, backend, state.key);
        }
    }

    fn reset_pending_gameplay_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
    ) {
        self.pending_gameplay_background_preps.clear();
        self.failed_gameplay_background_key = None;
        self.clear_gameplay_background_results();
        self.drop_stale_queued_gameplay_background(assets, backend, "");
    }

    fn spawn_banner_video_prep(&mut self, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if !self.pending_banner_video_preps.insert(key.clone()) {
            return;
        }

        let path = path.to_path_buf();
        let tx = self.banner_video_prep_tx.clone();
        thread::spawn(move || {
            let result = prepare_banner_video(key, path);
            let _ = tx.send(result);
        });
    }

    fn spawn_gameplay_background_prep(&mut self, path: &Path, animate_video: bool) {
        let key = path.to_string_lossy().into_owned();
        if !self.pending_gameplay_background_preps.insert(key.clone()) {
            return;
        }

        let path = path.to_path_buf();
        let tx = self.gameplay_background_prep_tx.clone();
        thread::spawn(move || {
            let result = prepare_gameplay_background(key, path, animate_video);
            let _ = tx.send(result);
        });
    }

    fn drain_banner_video_preps(&mut self, assets: &mut AssetManager, desired: &HashSet<String>) {
        while let Ok(result) = self.banner_video_prep_rx.try_recv() {
            match result {
                BannerVideoPrepResult::Ready(prepared) => {
                    self.pending_banner_video_preps.remove(&prepared.key);
                    if !desired.contains(&prepared.key) {
                        continue;
                    }
                    assets.queue_texture_upload(prepared.key.clone(), prepared.poster);
                    self.active_banner_videos.insert(
                        prepared.key,
                        DynamicVideoState {
                            player: prepared.player,
                            started_at: Instant::now(),
                        },
                    );
                }
                BannerVideoPrepResult::Failed { key, path, msg } => {
                    self.pending_banner_video_preps.remove(&key);
                    if desired.contains(&key) {
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
    ) -> bool {
        let mut failed = false;
        while let Ok(result) = self.gameplay_background_prep_rx.try_recv() {
            match result {
                GameplayBackgroundPrepResult::Ready(prepared) => {
                    self.pending_gameplay_background_preps.remove(&prepared.key);
                    if prepared.key != desired_key {
                        continue;
                    }
                    self.drop_stale_queued_gameplay_background(assets, backend, desired_key);
                    assets.queue_texture_upload(prepared.key.clone(), prepared.image);
                    self.queued_gameplay_background = Some(DynamicBackgroundState {
                        key: prepared.key,
                        path: prepared.path,
                        video: prepared.video,
                    });
                }
                GameplayBackgroundPrepResult::Failed { key, path, msg } => {
                    self.pending_gameplay_background_preps.remove(&key);
                    if key != desired_key {
                        continue;
                    }
                    warn!(
                        "Failed to prepare gameplay background '{}': {msg}. Using fallback.",
                        path.display()
                    );
                    failed = true;
                }
            }
        }
        failed
    }

    fn clear_gameplay_background_results(&mut self) {
        while self.gameplay_background_prep_rx.try_recv().is_ok() {}
    }

    fn drop_stale_queued_gameplay_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_key: &str,
    ) {
        let stale = self
            .queued_gameplay_background
            .as_ref()
            .is_some_and(|state| state.key != desired_key);
        if !stale {
            return;
        }
        let Some(state) = self.queued_gameplay_background.take() else {
            return;
        };
        if let Some((handle, texture)) = assets.remove_texture(&state.key) {
            assets.retire_texture(backend, handle, texture);
        }
    }

    fn promote_queued_gameplay_background(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_key: &str,
    ) -> bool {
        let ready = self
            .queued_gameplay_background
            .as_ref()
            .is_some_and(|state| {
                state.key == desired_key && assets.has_uploaded_texture_key(&state.key)
            });
        if !ready {
            return false;
        }
        let Some(state) = self.queued_gameplay_background.take() else {
            return false;
        };
        self.destroy_current_dynamic_background(assets, backend);
        self.current_dynamic_background = Some(state);
        true
    }

    fn destroy_current_profile_avatar_for_side(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        side: profile::PlayerSide,
    ) {
        let ix = Self::side_ix(side);
        let key = self.current_profile_avatars[ix].take().map(|(key, _)| key);
        profile::set_avatar_texture_key_for_side(side, None);
        if let Some(key) = key {
            self.release_texture_key(assets, backend, key);
        }
    }
}

fn prepare_banner_video(key: String, path: PathBuf) -> BannerVideoPrepResult {
    if !media_cache::banner_cache_options().enabled {
        return match video::open(&path, true) {
            Ok(video) => BannerVideoPrepResult::Ready(PreparedBannerVideo {
                key,
                poster: video.poster,
                player: video.player,
            }),
            Err(msg) => BannerVideoPrepResult::Failed { key, path, msg },
        };
    }

    let poster = match media_cache::load_banner_source_rgba(&path) {
        Ok(rgba) => rgba,
        Err(msg) => {
            return BannerVideoPrepResult::Failed { key, path, msg };
        }
    };
    let player = match video::open_player(&path, true) {
        Ok(player) => player,
        Err(msg) => {
            return BannerVideoPrepResult::Failed { key, path, msg };
        }
    };
    BannerVideoPrepResult::Ready(PreparedBannerVideo {
        key,
        poster,
        player,
    })
}

fn prepare_gameplay_background(
    key: String,
    path: PathBuf,
    animate_video: bool,
) -> GameplayBackgroundPrepResult {
    if dynamic::is_dynamic_video_path(&path) {
        if animate_video {
            return match video::open(&path, true) {
                Ok(video) => GameplayBackgroundPrepResult::Ready(PreparedGameplayBackground {
                    key,
                    path,
                    image: video.poster,
                    video: Some(video.player),
                }),
                Err(msg) => GameplayBackgroundPrepResult::Failed { key, path, msg },
            };
        }
        return match video::load_poster(&path) {
            Ok(image) => GameplayBackgroundPrepResult::Ready(PreparedGameplayBackground {
                key,
                path,
                image,
                video: None,
            }),
            Err(msg) => GameplayBackgroundPrepResult::Failed { key, path, msg },
        };
    }

    match open_image_fallback(&path) {
        Ok(image) => GameplayBackgroundPrepResult::Ready(PreparedGameplayBackground {
            key,
            path,
            image: image.to_rgba8(),
            video: None,
        }),
        Err(msg) => GameplayBackgroundPrepResult::Failed {
            key,
            path,
            msg: msg.to_string(),
        },
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
        media.current_dynamic_background = Some(DynamicBackgroundState {
            key: key.clone(),
            path,
            video: None,
        });

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
    fn queued_gameplay_background_counts_as_dynamic_texture_owner() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "queued-bg.mp4".to_string();

        assets.reserve_texture_handle(key.clone());
        media.queued_gameplay_background = Some(DynamicBackgroundState {
            key: key.clone(),
            path: PathBuf::from(&key),
            video: None,
        });

        let removed = media.take_releasable_texture(&mut assets, &key);

        assert!(removed.is_none());
        assert!(assets.has_texture_key(&key));
    }

    #[test]
    fn failed_banner_video_prep_clears_pending_key() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "banner.mp4".to_string();
        media.pending_banner_video_preps.insert(key.clone());
        media
            .banner_video_prep_tx
            .send(BannerVideoPrepResult::Failed {
                key: key.clone(),
                path: PathBuf::from(&key),
                msg: "failed".to_string(),
            })
            .unwrap();

        media.drain_banner_video_preps(&mut assets, &HashSet::from([key.clone()]));

        assert!(!media.pending_banner_video_preps.contains(&key));
        assert!(!media.active_banner_videos.contains_key(&key));
    }
}
