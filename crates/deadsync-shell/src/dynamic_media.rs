use deadlib_assets::dynamic;
use deadlib_render::{Backend, Texture as RendererTexture};
use deadlib_render_core::TextureHandle;
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
use log::{debug, warn};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};

/// Lazily started, session-owned transport for blocking media preparation.
///
/// `DynamicMedia` is the sole producer/result consumer. Worker threads share
/// the bounded request receiver behind one mutex only while receiving a job;
/// preparation runs after that guard is released. Capacity, worker count, and
/// result capacity are fixed by the owning media class. First submission is the
/// warmup point. A full queue rejects the job and leaves the caller unsettled so
/// it retries later; there is no growth, pruning, or eviction. Results apply
/// under the separate per-frame completion budget. Drop disconnects both
/// channels before joining workers, so queued payloads and prepared resources
/// are destroyed outside a live frame. Accepted/rejected totals are logged on
/// saturation. Worst app-frame work is fixed-pool startup once plus one
/// nonblocking send per desired media item.
struct MediaPrepWorker<J: Send + 'static, R: Send + 'static> {
    name: &'static str,
    worker_count: usize,
    request_capacity: usize,
    result_capacity: usize,
    prepare: fn(J) -> R,
    request_tx: Option<mpsc::SyncSender<J>>,
    result_rx: Option<mpsc::Receiver<R>>,
    workers: Vec<JoinHandle<()>>,
    accepted: u64,
    rejected: u64,
    #[cfg(test)]
    test_result_tx: Option<mpsc::SyncSender<R>>,
}

impl<J: Send + 'static, R: Send + 'static> MediaPrepWorker<J, R> {
    const fn new(
        name: &'static str,
        worker_count: usize,
        request_capacity: usize,
        result_capacity: usize,
        prepare: fn(J) -> R,
    ) -> Self {
        Self {
            name,
            worker_count,
            request_capacity,
            result_capacity,
            prepare,
            request_tx: None,
            result_rx: None,
            workers: Vec::new(),
            accepted: 0,
            rejected: 0,
            #[cfg(test)]
            test_result_tx: None,
        }
    }

    fn try_submit(&mut self, job: J) -> bool {
        let accepted = self.start()
            && self
                .request_tx
                .as_ref()
                .is_some_and(|tx| tx.try_send(job).is_ok());
        if accepted {
            self.accepted = self.accepted.saturating_add(1);
        } else {
            self.rejected = self.rejected.saturating_add(1);
        }
        accepted
    }

    fn try_recv(&self) -> Result<R, mpsc::TryRecvError> {
        self.result_rx
            .as_ref()
            .ok_or(mpsc::TryRecvError::Empty)?
            .try_recv()
    }

    fn start(&mut self) -> bool {
        if self.request_tx.is_some() {
            return true;
        }
        let (request_tx, request_rx) = mpsc::sync_channel(self.request_capacity);
        let (result_tx, result_rx) = mpsc::sync_channel(self.result_capacity);
        let request_rx = Arc::new(Mutex::new(request_rx));
        for index in 0..self.worker_count {
            let request_rx = request_rx.clone();
            let result_tx = result_tx.clone();
            let prepare = self.prepare;
            let name = format!("{}-{index}", self.name);
            match thread::Builder::new().name(name).spawn(move || {
                loop {
                    let job = match request_rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    let Ok(job) = job else {
                        return;
                    };
                    if result_tx.send(prepare(job)).is_err() {
                        return;
                    }
                }
            }) {
                Ok(worker) => self.workers.push(worker),
                Err(e) => warn!("Failed to start {} worker: {e}", self.name),
            }
        }
        if self.workers.is_empty() {
            return false;
        }
        #[cfg(test)]
        {
            self.test_result_tx = Some(result_tx);
        }
        self.request_tx = Some(request_tx);
        self.result_rx = Some(result_rx);
        true
    }

    #[cfg(test)]
    fn send_test_result(&mut self, result: R) {
        assert!(self.start());
        self.test_result_tx
            .as_ref()
            .expect("started media worker exposes a test result sender")
            .send(result)
            .expect("media test result receiver remains connected");
    }
}

impl<J: Send + 'static, R: Send + 'static> Drop for MediaPrepWorker<J, R> {
    fn drop(&mut self) {
        if self.rejected > 0 {
            debug!(
                "{} worker dispatch totals: accepted={} rejected={}",
                self.name, self.accepted, self.rejected
            );
        }
        self.request_tx.take();
        self.result_rx.take();
        #[cfg(test)]
        self.test_result_tx.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

struct BannerVideoPrepJob {
    key: String,
    path: PathBuf,
    looped: bool,
}

fn run_banner_video_prep(job: BannerVideoPrepJob) -> BannerVideoPrepResult {
    prepare_banner_video(job.key, job.path, job.looped)
}

struct GameplayBackgroundPrepJob {
    key: String,
    path: PathBuf,
}

fn run_gameplay_background_prep(job: GameplayBackgroundPrepJob) -> GameplayBackgroundPrepResult {
    prepare_gameplay_background(job.key, job.path)
}

struct DynamicBannerState {
    key: Arc<str>,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BannerVideoPrepState {
    Pending(bool),
    Failed(bool),
}

impl BannerVideoPrepState {
    const fn pending(looped: bool) -> Self {
        Self::Pending(looped)
    }

    const fn failed(looped: bool) -> Self {
        Self::Failed(looped)
    }

    const fn looped(self) -> bool {
        match self {
            Self::Pending(looped) | Self::Failed(looped) => looped,
        }
    }
}

struct SongLuaVideoState {
    player: video::Player,
    upload_handle: TextureHandle,
}

const MAX_CACHED_BANNER_VIDEO_PATHS: usize = 8;
const BANNER_VIDEO_PREP_WORKERS: usize = 2;
const BANNER_VIDEO_PREP_RESULTS: usize = MAX_MEDIA_COMPLETIONS_PER_FRAME;
const GAMEPLAY_BACKGROUND_PREP_WORKERS: usize = 1;
const GAMEPLAY_BACKGROUND_PREP_REQUESTS: usize = 1;
const GAMEPLAY_BACKGROUND_PREP_RESULTS: usize = 1;

/// Maximum decoded media completions integrated by one live frame.
///
/// Session-owned workers own decode/open work and send owned results through
/// bounded channels; `DynamicMedia` alone receives them on the frame thread.
/// Changed requests make old results stale, and both stale and current results
/// consume this budget before their players are retired or installed. A full
/// request queue rejects new work so the unsettled caller retries on a later
/// frame; it never grows backlog memory. Shutdown/explicit reset still drains
/// cleanup results outside the live-frame path. `dynamic_media_completion_budget`
/// measures burst latency, cycles, and destruction churn. Worst live-frame
/// integration is exactly this many results; remaining work is deferred.
const MAX_MEDIA_COMPLETIONS_PER_FRAME: usize = 2;

#[cfg(feature = "bench-support")]
#[must_use]
pub const fn benchmark_media_completion_budget() -> usize {
    MAX_MEDIA_COMPLETIONS_PER_FRAME
}

#[cfg(feature = "bench-support")]
pub struct BenchmarkMediaPrepDispatch {
    worker: MediaPrepWorker<usize, usize>,
}

#[cfg(feature = "bench-support")]
impl BenchmarkMediaPrepDispatch {
    pub fn new() -> Self {
        Self {
            worker: MediaPrepWorker::new(
                "media-prep-bench",
                BANNER_VIDEO_PREP_WORKERS,
                MAX_CACHED_BANNER_VIDEO_PATHS,
                BANNER_VIDEO_PREP_RESULTS,
                std::convert::identity,
            ),
        }
    }

    /// # Panics
    ///
    /// Panics if an internal worker fails or disconnects unexpectedly.
    pub fn warm(&mut self) {
        assert!(self.worker.start());
    }

    pub fn submit_burst(&mut self) -> usize {
        (0..MAX_CACHED_BANNER_VIDEO_PATHS)
            .filter(|&job| self.worker.try_submit(job))
            .count()
    }

    #[must_use]
    /// # Panics
    ///
    /// Panics if an internal worker fails or disconnects unexpectedly.
    pub fn drain_burst(&self) -> usize {
        let mut checksum = 0usize;
        for _ in 0..MAX_CACHED_BANNER_VIDEO_PATHS {
            loop {
                match self.worker.try_recv() {
                    Ok(result) => {
                        checksum ^= result;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => thread::yield_now(),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        panic!("media preparation benchmark worker disconnected")
                    }
                }
            }
        }
        checksum
    }
}

#[cfg(feature = "bench-support")]
impl Default for BenchmarkMediaPrepDispatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Render-thread snapshot of the desired banner-video set.
///
/// Owner/threading: `DynamicMedia` on the render thread; no synchronization.
/// Lifetime: one screen request, replaced at transitions. Capacity: eight
/// paths inline, matching the largest live caller; larger sets bypass the
/// settled fast path. Warmup: gameplay entry. Miss/eviction: a changed request
/// reconciles immediately and replaces the snapshot; no gameplay pruning.
/// Destruction: paths drop with `DynamicMedia`. Instrumentation: the release
/// benchmark compares reconciled and settled frames. Worst steady-frame cost:
/// at most eight path comparisons and no allocation, decoder, upload, or scan.
#[derive(Default)]
struct BannerVideoRequest {
    paths: SmallVec<[PathBuf; MAX_CACHED_BANNER_VIDEO_PATHS]>,
    looped: bool,
    settled: bool,
    cacheable: bool,
}

impl BannerVideoRequest {
    fn matches(&self, desired_paths: &[&Path], looped: bool) -> bool {
        if !self.cacheable || self.looped != looped {
            return false;
        }
        let mut cached = self.paths.iter();
        for desired in desired_paths
            .iter()
            .copied()
            .filter(|path| dynamic::is_dynamic_video_path(path))
        {
            if cached.next().is_none_or(|path| path != desired) {
                return false;
            }
        }
        cached.next().is_none()
    }

    /// Returns whether reconciliation is required for a changed request.
    fn begin(&mut self, desired_paths: &[&Path], looped: bool) -> bool {
        if self.matches(desired_paths, looped) {
            return !self.settled;
        }
        self.paths.clear();
        self.looped = looped;
        self.settled = false;
        self.cacheable = true;
        for path in desired_paths
            .iter()
            .copied()
            .filter(|path| dynamic::is_dynamic_video_path(path))
        {
            if self.paths.len() == MAX_CACHED_BANNER_VIDEO_PATHS {
                self.paths.clear();
                self.cacheable = false;
                break;
            }
            self.paths.push(path.to_path_buf());
        }
        true
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BgVideoTiming {
    pub current_sec: f32,
    pub start_sec: Option<f32>,
    pub rate: f32,
}

fn sync_bg_video_timing(state: &mut DynamicBackgroundState, timing: BgVideoTiming) {
    if let Some(start) = timing.start_sec
        && (state.video_start_sec() - start).abs() > f32::EPSILON
    {
        state.reset_video(start, timing.rate);
    } else if (state.video_rate() - timing.rate).abs() > f32::EPSILON {
        state.set_video_rate(timing.rate, timing.current_sec);
    }
}

pub struct DynamicMedia {
    current_dynamic_banner: Option<DynamicBannerState>,
    active_banner_videos: FxHashMap<String, DynamicVideoState>,
    /// Screen-local banner preparation states. Entries are retained after a
    /// failure only while that exact path/mode remains desired, preventing
    /// live-frame retry storms without becoming a session-growing cache.
    banner_video_preps: FxHashMap<PathBuf, BannerVideoPrepState>,
    banner_video_prep: MediaPrepWorker<BannerVideoPrepJob, BannerVideoPrepResult>,
    banner_video_request: BannerVideoRequest,
    current_dynamic_cdtitle: Option<(Arc<str>, PathBuf)>,
    current_dynamic_pack_banner: Option<(String, PathBuf)>,
    dynamic_pack_banner_keys: FxHashSet<String>,
    wheel_item_background_keys: FxHashSet<String>,
    current_dynamic_background: Option<DynamicBackgroundState>,
    active_song_lua_videos: FxHashMap<String, SongLuaVideoState>,
    failed_song_lua_video_keys: FxHashSet<String>,
    gameplay_background_keys: FxHashSet<String>,
    pending_gameplay_background_preps: FxHashSet<String>,
    gameplay_background_prep:
        MediaPrepWorker<GameplayBackgroundPrepJob, GameplayBackgroundPrepResult>,
    failed_gameplay_background_key: Option<String>,
    current_profile_avatars: [Option<(String, PathBuf)>; 2],
    preloaded_profile_avatar_keys: FxHashSet<String>,
}

impl DynamicMedia {
    pub fn new() -> Self {
        Self {
            current_dynamic_banner: None,
            active_banner_videos: FxHashMap::default(),
            banner_video_preps: FxHashMap::default(),
            banner_video_prep: MediaPrepWorker::new(
                "banner-video-prep",
                BANNER_VIDEO_PREP_WORKERS,
                MAX_CACHED_BANNER_VIDEO_PATHS,
                BANNER_VIDEO_PREP_RESULTS,
                run_banner_video_prep,
            ),
            banner_video_request: BannerVideoRequest::default(),
            current_dynamic_cdtitle: None,
            current_dynamic_pack_banner: None,
            dynamic_pack_banner_keys: FxHashSet::default(),
            wheel_item_background_keys: FxHashSet::default(),
            current_dynamic_background: None,
            active_song_lua_videos: FxHashMap::default(),
            failed_song_lua_video_keys: FxHashSet::default(),
            gameplay_background_keys: FxHashSet::default(),
            pending_gameplay_background_preps: FxHashSet::default(),
            gameplay_background_prep: MediaPrepWorker::new(
                "gameplay-background-prep",
                GAMEPLAY_BACKGROUND_PREP_WORKERS,
                GAMEPLAY_BACKGROUND_PREP_REQUESTS,
                GAMEPLAY_BACKGROUND_PREP_RESULTS,
                run_gameplay_background_prep,
            ),
            failed_gameplay_background_key: None,
            current_profile_avatars: std::array::from_fn(|_| None),
            preloaded_profile_avatar_keys: FxHashSet::default(),
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
        let mut keys: Vec<String> = Vec::with_capacity(
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
            keys.push(state.key.to_string());
        }
        keys.extend(self.active_banner_videos.drain().map(|(key, state)| {
            retire_dynamic_video_state(state);
            key
        }));
        self.banner_video_preps.clear();
        self.banner_video_request = BannerVideoRequest::default();
        if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
            keys.push(key.to_string());
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
        keys.extend(self.active_song_lua_videos.drain().map(|(key, state)| {
            retire_video_player(state.player);
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
    ) -> Option<Arc<str>> {
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
                    let key = Arc::<str>::from(key);
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

            // Gameplay prewarms visible pack banners before this command runs. Reuse that upload
            // instead of replacing the texture again in the same frame.
            if assets.has_texture_key(&key) {
                if banner_cache_opts.enabled {
                    self.dynamic_pack_banner_keys.insert(key.clone());
                }
                self.current_dynamic_pack_banner = Some((key, path));
                return;
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
        let mut desired = FxHashSet::with_capacity_and_hasher(paths.len(), FxBuildHasher);
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
    ) -> Arc<str> {
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
                    let key = Arc::<str>::from(key);
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
                    Arc::<str>::from(FALLBACK_KEY)
                }
                Err(DynamicImageTextureError::Create(e)) => {
                    warn!("Failed to create GPU texture for banner '{key}': {e}. Using fallback.");
                    Arc::<str>::from(FALLBACK_KEY)
                }
            }
        } else {
            self.destroy_current_dynamic_banner(assets, backend);
            Arc::<str>::from(FALLBACK_KEY)
        }
    }

    pub fn sync_active_banner_video(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_path: Option<&Path>,
        looped: bool,
    ) {
        let desired_path = desired_path.filter(|path| dynamic::is_dynamic_video_path(path));
        let desired_paths = desired_path.as_slice();
        if !self.banner_video_request.begin(desired_paths, looped) {
            return;
        }
        self.retain_banner_video_prep(desired_path, looped);
        let stale_keys = self
            .active_banner_videos
            .iter()
            .filter(|(_, state)| {
                Some(state.path.as_path()) != desired_path || state.looped != looped
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(state) = self.active_banner_videos.remove(&key) {
                retire_dynamic_video_state(state);
            }
            self.release_texture_key(assets, backend, key);
        }
        self.drain_banner_video_preps(assets, desired_path, looped);
        if let Some(path) = desired_path
            && !self
                .active_banner_videos
                .values()
                .any(|state| state.path.as_path() == path)
            && !self
                .banner_video_preps
                .get(path)
                .is_some_and(|prep| prep.looped() == looped)
        {
            self.submit_banner_video_prep(path, looped);
        }
        self.finish_banner_video_request(desired_paths, looped);
    }

    pub fn sync_active_banner_videos(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_paths: &[&Path],
        looped: bool,
    ) {
        if !self.banner_video_request.begin(desired_paths, looped) {
            return;
        }
        self.retain_banner_video_preps(desired_paths, looped);
        let stale_keys = self
            .active_banner_videos
            .iter()
            .filter(|(_, state)| {
                !dynamic_video_path_in_set(&state.path, desired_paths) || state.looped != looped
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(state) = self.active_banner_videos.remove(&key) {
                retire_dynamic_video_state(state);
            }
            self.release_texture_key(assets, backend, key);
        }
        self.drain_banner_video_preps_multi(assets, desired_paths, looped);
        for path in desired_paths {
            if !dynamic::is_dynamic_video_path(path) {
                continue;
            }
            if self
                .active_banner_videos
                .values()
                .any(|state| state.path.as_path() == *path)
                || self
                    .banner_video_preps
                    .get(*path)
                    .is_some_and(|prep| prep.looped() == looped)
            {
                continue;
            }
            self.submit_banner_video_prep(path, looped);
        }
        self.finish_banner_video_request(desired_paths, looped);
    }

    fn finish_banner_video_request(&mut self, desired_paths: &[&Path], looped: bool) {
        self.banner_video_request.settled = self.banner_video_request.cacheable
            && desired_paths
                .iter()
                .copied()
                .filter(|path| dynamic::is_dynamic_video_path(path))
                .all(|path| {
                    self.active_banner_videos
                        .values()
                        .any(|state| state.path.as_path() == path && state.looped == looped)
                        || self
                            .banner_video_preps
                            .get(path)
                            .is_some_and(|prep| *prep == BannerVideoPrepState::failed(looped))
                });
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
                let upload_handle = assets.reserve_texture_handle(key.clone());
                self.current_dynamic_background = Some(DynamicBackgroundState::new(
                    key.clone(),
                    upload_handle,
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
        timing: BgVideoTiming,
    ) -> Option<String> {
        const FALLBACK_KEY: &str = "__black";

        let Some(path) = desired_path else {
            if self.gameplay_background_settled(None, None, false, timing) {
                return None;
            }
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
        let wants_video = animate_video && dynamic::is_dynamic_video_path(path);
        let timing = BgVideoTiming {
            rate: dynamic::normalize_video_rate(timing.rate),
            ..timing
        };
        if !wants_video || self.failed_gameplay_background_key.as_deref() != Some(desired_key) {
            self.failed_gameplay_background_key = None;
        }
        if self.gameplay_background_settled(Some(path), Some(desired_key), wants_video, timing) {
            return None;
        }

        if wants_video {
            self.drain_gameplay_background_preps(assets, backend, desired_key, timing);
        } else {
            self.reset_pending_gameplay_background();
        }
        if self.gameplay_background_settled(Some(path), Some(desired_key), wants_video, timing) {
            return None;
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
            if let Some(state) = self.current_dynamic_background.as_mut() {
                sync_bg_video_timing(state, timing);
            }
            let needs_video = self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| state.video.is_none());
            if needs_video
                && !self.pending_gameplay_background_preps.contains(desired_key)
                && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
            {
                self.submit_gameplay_background_prep(path);
            }
            return None;
        }
        if !current_path_matches {
            self.destroy_current_dynamic_background(assets, backend);
            let upload_handle = assets.reserve_texture_handle(desired_key.to_owned());
            self.current_dynamic_background = Some(DynamicBackgroundState::new(
                desired_key.to_owned(),
                upload_handle,
                path.to_path_buf(),
                None,
                timing.start_sec.unwrap_or(timing.current_sec),
                timing.rate,
            ));
            if wants_video
                && !self.pending_gameplay_background_preps.contains(desired_key)
                && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
            {
                self.submit_gameplay_background_prep(path);
            }
            return Some(desired_key.to_owned());
        }

        if wants_video
            && !self.pending_gameplay_background_preps.contains(desired_key)
            && self.failed_gameplay_background_key.as_deref() != Some(desired_key)
        {
            self.submit_gameplay_background_prep(path);
        }
        None
    }

    pub(crate) fn gameplay_bg_pending(&self) -> bool {
        !self.pending_gameplay_background_preps.is_empty()
    }

    pub(crate) const fn banner_sync_pending(&self) -> bool {
        !self.banner_video_request.settled
    }

    /// Whether the game-thread-owned background state needs no maintenance.
    ///
    /// A settled frame performs only bounded field/path comparisons. It does
    /// not poll worker channels, query texture registries, allocate, evict, or
    /// destroy resources. Pending video work keeps the request unsettled until
    /// its bounded completion drain finishes; a failed video request saturates
    /// on its prewarmed poster until the desired path or mode changes.
    fn gameplay_background_settled(
        &self,
        desired_path: Option<&Path>,
        desired_key: Option<&str>,
        wants_video: bool,
        timing: BgVideoTiming,
    ) -> bool {
        if !self.pending_gameplay_background_preps.is_empty() {
            return false;
        }
        let Some(path) = desired_path else {
            return self.current_dynamic_background.is_none()
                && self.failed_gameplay_background_key.is_none();
        };
        let Some(key) = desired_key else {
            return false;
        };
        let Some(state) = self.current_dynamic_background.as_ref() else {
            return false;
        };
        if state.path != path || state.key != key {
            return false;
        }
        if state.video.is_some() == wants_video
            && (!wants_video || (state.video_rate() - timing.rate).abs() <= f32::EPSILON)
            && (!wants_video
                || timing
                    .start_sec
                    .is_none_or(|start| (state.video_start_sec() - start).abs() <= f32::EPSILON))
        {
            return true;
        }
        wants_video
            && state.video.is_none()
            && self.failed_gameplay_background_key.as_deref() == Some(key)
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
            .filter(|key| !desired.contains(key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let stale_failed = self
            .failed_song_lua_video_keys
            .iter()
            .filter(|key| !desired.contains(key.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        for key in stale_active {
            if let Some(state) = self.active_song_lua_videos.remove(&key) {
                retire_video_player(state.player);
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
            let key = path.to_string_lossy();
            if self.active_song_lua_videos.contains_key(key.as_ref())
                || self.failed_song_lua_video_keys.contains(key.as_ref())
            {
                continue;
            }
            match prepare_song_lua_video(path, !assets.has_texture_key(key.as_ref())) {
                SongLuaVideoPrepResult::Ready(prepared) => {
                    let prepared = *prepared;
                    match prepared.poster {
                        Ok(Some(poster)) => {
                            assets.queue_texture_upload(prepared.key.clone(), poster);
                        }
                        Ok(None) => {}
                        Err(e) => warn!(
                            "Failed to load song lua video poster '{}': {e}",
                            path.display()
                        ),
                    }
                    let upload_handle = assets.reserve_texture_handle(prepared.key.clone());
                    self.active_song_lua_videos.insert(
                        prepared.key.clone(),
                        SongLuaVideoState {
                            player: prepared.player,
                            upload_handle,
                        },
                    );
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
        for (key, state) in std::mem::take(&mut self.active_song_lua_videos) {
            retire_video_player(state.player);
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

    pub(crate) fn has_gameplay_video(&self) -> bool {
        self.current_dynamic_background
            .as_ref()
            .is_some_and(|state| state.video.is_some())
            || !self.active_song_lua_videos.is_empty()
    }

    pub(crate) fn has_active_video(&self) -> bool {
        !self.active_banner_videos.is_empty() || self.has_gameplay_video()
    }

    pub fn queue_video_frames(
        &mut self,
        assets: &mut AssetManager,
        gameplay_time_sec: Option<f32>,
        ui_time_sec: f32,
    ) {
        for video in self.active_banner_videos.values_mut() {
            if assets.has_pending_texture_upload_handle(video.upload_handle) {
                continue;
            }
            let play_time = video
                .started_at
                .map_or(0.0, |start| start.elapsed().as_secs_f32());
            if let Some(frame) = video.player.take_due_frame(play_time) {
                video.started_at.get_or_insert_with(Instant::now);
                assets.queue_video_frame_upload(video.upload_handle, frame);
            }
        }

        if let Some(state) = self.current_dynamic_background.as_mut()
            && !assets.has_pending_texture_upload_handle(state.upload_handle)
        {
            let play_time = gameplay_time_sec.unwrap_or(ui_time_sec);
            let play_time = state.video_play_time(play_time);
            if let Some(video) = state.video.as_mut()
                && let Some(frame) = video.take_due_frame(play_time)
            {
                assets.queue_video_frame_upload(state.upload_handle, frame);
            }
        }

        let song_lua_play_time = gameplay_time_sec.unwrap_or(0.0).max(0.0);
        for state in self.active_song_lua_videos.values_mut() {
            if assets.has_pending_texture_upload_handle(state.upload_handle) {
                continue;
            }
            if let Some(frame) = state.player.take_due_frame(song_lua_play_time) {
                assets.queue_video_frame_upload(state.upload_handle, frame);
            }
        }
    }

    #[inline(always)]
    fn texture_key_in_use(&self, key: &str) -> bool {
        self.current_dynamic_banner
            .as_ref()
            .is_some_and(|state| state.key.as_ref() == key)
            || self.active_banner_videos.contains_key(key)
            || self
                .current_dynamic_cdtitle
                .as_ref()
                .is_some_and(|(owned, _)| owned.as_ref() == key)
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
        &self,
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
        &self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        key: impl AsRef<str>,
    ) {
        if let Some((_handle, texture)) = self.take_releasable_texture(assets, key.as_ref()) {
            backend.retire_texture(texture);
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

    fn submit_banner_video_prep(&mut self, path: &Path, looped: bool) {
        if self
            .banner_video_preps
            .get(path)
            .is_some_and(|prep| prep.looped() == looped)
        {
            return;
        }
        let key = path.to_string_lossy().into_owned();
        let path = path.to_path_buf();
        if self.banner_video_prep.try_submit(BannerVideoPrepJob {
            key,
            path: path.clone(),
            looped,
        }) {
            self.banner_video_preps
                .insert(path, BannerVideoPrepState::pending(looped));
        }
    }

    fn retain_banner_video_prep(&mut self, desired_path: Option<&Path>, looped: bool) {
        self.banner_video_preps
            .retain(|path, prep| Some(path.as_path()) == desired_path && prep.looped() == looped);
    }

    fn retain_banner_video_preps(&mut self, desired_paths: &[&Path], looped: bool) {
        self.banner_video_preps.retain(|path, prep| {
            dynamic_video_path_in_set(path, desired_paths) && prep.looped() == looped
        });
    }

    fn submit_gameplay_background_prep(&mut self, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if self.pending_gameplay_background_preps.contains(&key) {
            return;
        }

        let path = path.to_path_buf();
        if self
            .gameplay_background_prep
            .try_submit(GameplayBackgroundPrepJob {
                key: key.clone(),
                path,
            })
        {
            self.pending_gameplay_background_preps.insert(key);
        }
    }

    fn drain_banner_video_preps(
        &mut self,
        assets: &mut AssetManager,
        desired_path: Option<&Path>,
        looped: bool,
    ) {
        for _ in 0..MAX_MEDIA_COMPLETIONS_PER_FRAME {
            let Ok(result) = self.banner_video_prep.try_recv() else {
                break;
            };
            match result {
                BannerVideoPrepResult::Ready(prepared) => {
                    let prepared = *prepared;
                    self.clear_pending_banner_video_prep(&prepared.path, prepared.looped);
                    if Some(prepared.path.as_path()) != desired_path || prepared.looped != looped {
                        retire_video_player(prepared.player);
                        continue;
                    }
                    assets.queue_texture_upload(prepared.key.clone(), prepared.poster);
                    let upload_handle = assets.reserve_texture_handle(prepared.key.clone());
                    if let Some(old) = self.active_banner_videos.insert(
                        prepared.key,
                        DynamicVideoState {
                            player: prepared.player,
                            upload_handle,
                            started_at: None,
                            path: prepared.path,
                            looped: prepared.looped,
                        },
                    ) {
                        retire_dynamic_video_state(old);
                    }
                }
                BannerVideoPrepResult::Failed {
                    path,
                    looped: failed_looped,
                    msg,
                } => {
                    if Some(path.as_path()) == desired_path && failed_looped == looped {
                        warn!("Failed to start banner video '{}': {msg}", path.display());
                        self.banner_video_preps
                            .insert(path, BannerVideoPrepState::failed(failed_looped));
                    } else {
                        self.clear_pending_banner_video_prep(&path, failed_looped);
                    }
                }
            }
        }
    }

    fn drain_banner_video_preps_multi(
        &mut self,
        assets: &mut AssetManager,
        desired_paths: &[&Path],
        looped: bool,
    ) {
        for _ in 0..MAX_MEDIA_COMPLETIONS_PER_FRAME {
            let Ok(result) = self.banner_video_prep.try_recv() else {
                break;
            };
            match result {
                BannerVideoPrepResult::Ready(prepared) => {
                    let prepared = *prepared;
                    self.clear_pending_banner_video_prep(&prepared.path, prepared.looped);
                    if !desired_paths.iter().any(|path| {
                        dynamic::is_dynamic_video_path(path) && *path == prepared.path.as_path()
                    }) || prepared.looped != looped
                    {
                        retire_video_player(prepared.player);
                        continue;
                    }
                    assets.queue_texture_upload(prepared.key.clone(), prepared.poster);
                    let upload_handle = assets.reserve_texture_handle(prepared.key.clone());
                    if let Some(old) = self.active_banner_videos.insert(
                        prepared.key,
                        DynamicVideoState {
                            player: prepared.player,
                            upload_handle,
                            started_at: None,
                            path: prepared.path,
                            looped: prepared.looped,
                        },
                    ) {
                        retire_dynamic_video_state(old);
                    }
                }
                BannerVideoPrepResult::Failed {
                    path,
                    looped: failed_looped,
                    msg,
                } => {
                    if desired_paths.iter().any(|desired| {
                        dynamic::is_dynamic_video_path(desired) && *desired == path.as_path()
                    }) && failed_looped == looped
                    {
                        warn!("Failed to start banner video '{}': {msg}", path.display());
                        self.banner_video_preps
                            .insert(path, BannerVideoPrepState::failed(failed_looped));
                    } else {
                        self.clear_pending_banner_video_prep(&path, failed_looped);
                    }
                }
            }
        }
    }

    fn clear_pending_banner_video_prep(&mut self, path: &Path, looped: bool) {
        if self
            .banner_video_preps
            .get(path)
            .is_some_and(|prep| prep.looped() == looped)
        {
            self.banner_video_preps.remove(path);
        }
    }

    fn drain_gameplay_background_preps(
        &mut self,
        assets: &mut AssetManager,
        backend: &mut Backend,
        desired_key: &str,
        timing: BgVideoTiming,
    ) {
        for _ in 0..MAX_MEDIA_COMPLETIONS_PER_FRAME {
            let Ok(result) = self.gameplay_background_prep.try_recv() else {
                break;
            };
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
                        sync_bg_video_timing(state, timing);
                        state.attach_video(prepared.player);
                    } else {
                        if let Some(state) = self.current_dynamic_background.take() {
                            let key = retire_dynamic_background_state(state);
                            self.release_texture_key(assets, backend, key);
                        }
                        let upload_handle = assets.reserve_texture_handle(prepared.key.clone());
                        self.current_dynamic_background = Some(DynamicBackgroundState::new(
                            prepared.key,
                            upload_handle,
                            prepared.path,
                            Some(prepared.player),
                            timing.start_sec.unwrap_or(timing.current_sec),
                            timing.rate,
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

    fn clear_gameplay_background_results(&self) {
        while let Ok(result) = self.gameplay_background_prep.try_recv() {
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

/// Models the steady frames after a desired banner decoder failed to open.
/// Models two prewarmed gameplay banners after both decoders have settled.
impl Default for DynamicMedia {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    struct BlockingPrep {
        value: usize,
        started: Arc<Barrier>,
        release: Arc<Barrier>,
    }

    fn run_blocking_prep(job: BlockingPrep) -> usize {
        job.started.wait();
        job.release.wait();
        job.value
    }

    fn recv_blocking_prep(worker: &MediaPrepWorker<BlockingPrep, usize>) -> usize {
        loop {
            match worker.try_recv() {
                Ok(value) => return value,
                Err(mpsc::TryRecvError::Empty) => thread::yield_now(),
                Err(mpsc::TryRecvError::Disconnected) => panic!("bounded worker disconnected"),
            }
        }
    }

    #[test]
    fn media_prep_worker_rejects_work_beyond_its_fixed_capacity() {
        let started = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let mut worker = MediaPrepWorker::new("bounded-test", 1, 1, 1, run_blocking_prep);

        assert!(worker.workers.is_empty());
        assert!(worker.try_submit(BlockingPrep {
            value: 1,
            started: started.clone(),
            release: release.clone(),
        }));
        started.wait();
        assert!(worker.try_submit(BlockingPrep {
            value: 2,
            started: started.clone(),
            release: release.clone(),
        }));
        assert!(!worker.try_submit(BlockingPrep {
            value: 3,
            started: started.clone(),
            release: release.clone(),
        }));
        assert_eq!((worker.accepted, worker.rejected), (2, 1));

        release.wait();
        assert_eq!(recv_blocking_prep(&worker), 1);
        started.wait();
        release.wait();
        assert_eq!(recv_blocking_prep(&worker), 2);
    }

    #[test]
    fn bg_timing_resets_segment() {
        let mut state = DynamicBackgroundState::new(
            "movie".to_owned(),
            1,
            PathBuf::from("movie.mp4"),
            None,
            -3.0,
            1.0,
        );

        sync_bg_video_timing(
            &mut state,
            BgVideoTiming {
                current_sec: 0.0,
                start_sec: Some(-3.0),
                rate: 1.0,
            },
        );
        assert_eq!(state.video_play_time(0.0), 3.0);

        sync_bg_video_timing(
            &mut state,
            BgVideoTiming {
                current_sec: 0.0,
                start_sec: Some(-1.0),
                rate: 0.5,
            },
        );
        assert_eq!(state.video_start_sec(), -1.0);
        assert_eq!(state.video_play_time(1.0), 1.0);
    }

    #[test]
    fn absent_gameplay_background_settles_only_without_pending_state() {
        let timing = BgVideoTiming {
            current_sec: 4.0,
            start_sec: None,
            rate: 1.0,
        };
        let mut media = DynamicMedia::new();

        assert!(!media.gameplay_bg_pending());
        assert!(media.gameplay_background_settled(None, None, false, timing));

        media
            .pending_gameplay_background_preps
            .insert("old.mp4".to_owned());
        assert!(media.gameplay_bg_pending());
        assert!(!media.gameplay_background_settled(None, None, false, timing));
        media.pending_gameplay_background_preps.clear();
        assert!(!media.gameplay_bg_pending());

        media.failed_gameplay_background_key = Some("old.mp4".to_owned());
        assert!(!media.gameplay_background_settled(None, None, false, timing));
    }

    #[test]
    fn video_tick_requires_an_active_decoder() {
        let mut media = DynamicMedia::new();
        assert!(!media.has_active_video());
        assert!(!media.has_gameplay_video());

        media.current_dynamic_background = Some(DynamicBackgroundState::new(
            "background.png".to_owned(),
            1,
            PathBuf::from("background.png"),
            None,
            0.0,
            1.0,
        ));
        assert!(!media.has_active_video());
        assert!(!media.has_gameplay_video());
    }

    #[test]
    fn gameplay_background_settles_static_and_failed_video_posters() {
        let path = Path::new("background.png");
        let key = "background.png";
        let timing = BgVideoTiming {
            current_sec: 12.0,
            start_sec: Some(3.0),
            rate: 1.25,
        };
        let mut media = DynamicMedia::new();
        media.current_dynamic_background = Some(DynamicBackgroundState::new(
            key.to_owned(),
            1,
            path.to_path_buf(),
            None,
            0.0,
            1.0,
        ));

        assert!(media.gameplay_background_settled(Some(path), Some(key), false, timing));
        assert!(!media.gameplay_background_settled(
            Some(Path::new("next.png")),
            Some("next.png"),
            false,
            timing,
        ));
        assert!(!media.gameplay_background_settled(Some(path), Some(key), true, timing));

        media.failed_gameplay_background_key = Some(key.to_owned());
        assert!(media.gameplay_background_settled(Some(path), Some(key), true, timing));

        media
            .pending_gameplay_background_preps
            .insert(key.to_owned());
        assert!(!media.gameplay_background_settled(Some(path), Some(key), true, timing));
    }

    #[test]
    fn settled_banner_request_skips_until_paths_or_mode_change() {
        let paths = [Path::new("p1.mp4"), Path::new("p2.webm")];
        let mut media = DynamicMedia::new();

        assert!(media.banner_sync_pending());
        assert!(media.banner_video_request.begin(&paths, true));
        media.banner_video_request.settled = true;
        assert!(!media.banner_sync_pending());
        assert!(!media.banner_video_request.begin(&paths, true));
        assert!(media.banner_video_request.begin(&paths, false));
        assert!(media.banner_sync_pending());

        let changed = [Path::new("p1.mp4"), Path::new("p3.avi")];
        media.banner_video_request.settled = true;
        assert!(media.banner_video_request.begin(&changed, false));
    }

    #[test]
    fn banner_request_cache_is_bounded_and_ignores_static_art() {
        let videos = (0..=MAX_CACHED_BANNER_VIDEO_PATHS)
            .map(|index| PathBuf::from(format!("banner-{index}.mp4")))
            .collect::<Vec<_>>();
        let paths = videos.iter().map(PathBuf::as_path).collect::<Vec<_>>();
        let mut request = BannerVideoRequest::default();

        assert!(request.begin(&paths, true));
        assert!(!request.cacheable);
        assert!(request.paths.is_empty());

        let mixed = [Path::new("banner.png"), Path::new("banner.mp4")];
        assert!(request.begin(&mixed, true));
        assert_eq!(request.paths.as_slice(), [PathBuf::from("banner.mp4")]);
    }

    #[test]
    fn banner_request_settles_only_after_pending_prep_finishes() {
        let path = Path::new("banner.mp4");
        let paths = [path];
        let mut media = DynamicMedia::new();
        assert!(media.banner_video_request.begin(&paths, true));
        media
            .banner_video_preps
            .insert(path.to_path_buf(), BannerVideoPrepState::pending(true));

        media.finish_banner_video_request(&paths, true);
        assert!(!media.banner_video_request.settled);

        media
            .banner_video_preps
            .insert(path.to_path_buf(), BannerVideoPrepState::failed(true));
        media.finish_banner_video_request(&paths, true);
        assert!(media.banner_video_request.settled);
    }

    #[test]
    fn shared_dynamic_key_stays_until_last_owner_releases_it() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "shared.mp4".to_string();
        let path = PathBuf::from(&key);

        assets.reserve_texture_handle(key.clone());
        media.current_dynamic_banner = Some(DynamicBannerState {
            key: Arc::from(key.as_str()),
            path: path.clone(),
        });
        media.current_dynamic_background = Some(DynamicBackgroundState::new(
            key.clone(),
            1,
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
            key: Arc::from(key.as_str()),
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
    fn failed_banner_video_prep_saturates_until_the_request_changes() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let key = "banner.mp4".to_string();
        media
            .banner_video_preps
            .insert(PathBuf::from(&key), BannerVideoPrepState::pending(false));
        media
            .banner_video_prep
            .send_test_result(BannerVideoPrepResult::Failed {
                path: PathBuf::from(&key),
                looped: false,
                msg: "failed".to_string(),
            });

        media.drain_banner_video_preps(&mut assets, Some(Path::new(&key)), false);

        assert_eq!(
            media.banner_video_preps.get(Path::new(&key)),
            Some(&BannerVideoPrepState::failed(false))
        );
        assert!(!media.active_banner_videos.contains_key(&key));

        media.retain_banner_video_prep(None, false);
        assert!(!media.banner_video_preps.contains_key(Path::new(&key)));
    }

    #[test]
    fn banner_completion_bursts_drain_under_a_per_frame_budget() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let paths: Vec<PathBuf> = (0..5)
            .map(|index| PathBuf::from(format!("stale-{index}.mp4")))
            .collect();
        for path in &paths {
            media
                .banner_video_preps
                .insert(path.clone(), BannerVideoPrepState::pending(false));
        }
        for path in &paths[..MAX_MEDIA_COMPLETIONS_PER_FRAME] {
            media
                .banner_video_prep
                .send_test_result(BannerVideoPrepResult::Failed {
                    path: path.clone(),
                    looped: false,
                    msg: "stale".to_string(),
                });
        }

        media.drain_banner_video_preps(&mut assets, None, false);
        assert_eq!(media.banner_video_preps.len(), 3);

        for path in &paths[MAX_MEDIA_COMPLETIONS_PER_FRAME..MAX_MEDIA_COMPLETIONS_PER_FRAME * 2] {
            media
                .banner_video_prep
                .send_test_result(BannerVideoPrepResult::Failed {
                    path: path.clone(),
                    looped: false,
                    msg: "stale".to_string(),
                });
        }
        media.drain_banner_video_preps(&mut assets, None, false);
        assert_eq!(media.banner_video_preps.len(), 1);

        media
            .banner_video_prep
            .send_test_result(BannerVideoPrepResult::Failed {
                path: paths[4].clone(),
                looped: false,
                msg: "stale".to_string(),
            });
        media.drain_banner_video_preps(&mut assets, None, false);
        assert!(media.banner_video_preps.is_empty());
        assert!(media.banner_video_prep.try_recv().is_err());
    }

    #[test]
    fn stale_banner_prep_result_keeps_new_playback_request_pending() {
        let mut assets = AssetManager::new();
        let mut media = DynamicMedia::new();
        let path = PathBuf::from("banner.mp4");
        media
            .banner_video_preps
            .insert(path.clone(), BannerVideoPrepState::pending(false));
        media
            .banner_video_prep
            .send_test_result(BannerVideoPrepResult::Failed {
                path: path.clone(),
                looped: true,
                msg: "stale failure".to_string(),
            });

        media.drain_banner_video_preps(&mut assets, Some(&path), false);

        assert_eq!(
            media.banner_video_preps.get(&path),
            Some(&BannerVideoPrepState::pending(false))
        );
    }
}
