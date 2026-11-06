use deadlib_assets::AssetManager;
use deadlib_present::{compose::TextureContext, space::Metrics, texture::TextureMeta};
use deadlib_render::{
    Backend, BackendType, DrawStats, PresentModePolicy, RenderFrame, Texture, TextureHandle,
    TextureHandleMap,
};
use std::{
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const PRESETS: &[(&str, u32, u32)] = &[
    ("vga", 640, 480),
    ("svga", 800, 600),
    ("xga", 1024, 768),
    ("5-4", 1280, 1024),
    ("720p", 1280, 720),
    ("square", 640, 640),
    ("16-10", 1280, 800),
    ("ultrawide", 1280, 540),
    ("uwqhd", 1720, 720),
    ("portrait-9-16", 540, 960),
    ("portrait-10-16", 540, 960),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub aspect: (u32, u32),
}

#[derive(Default)]
pub struct CaptureBatch {
    pending: VecDeque<PathBuf>,
    render_elapsed: Duration,
}

pub struct CaptureTimings {
    pub render: Duration,
    pub readback: Duration,
    pub png_write: Duration,
}

impl CaptureBatch {
    pub fn draw(
        &mut self,
        backend: &mut Backend,
        frame: &RenderFrame,
        textures: &TextureHandleMap<Texture>,
        path: PathBuf,
    ) -> Result<DrawStats> {
        backend.request_screenshot();
        let started = Instant::now();
        let stats = backend.draw(frame, textures, false)?;
        self.render_elapsed += started.elapsed();
        self.pending.push_back(path);
        Ok(stats)
    }

    pub fn finish(mut self, backend: &mut Backend, size: &Size) -> Result<CaptureTimings> {
        let mut timings = CaptureTimings {
            render: self.render_elapsed,
            readback: Duration::ZERO,
            png_write: Duration::ZERO,
        };
        while let Some(path) = self.pending.pop_front() {
            let started = Instant::now();
            let image = backend.capture_frame()?;
            timings.readback += started.elapsed();
            if image.dimensions() != (size.width, size.height) {
                return Err(format!(
                    "capture returned {:?}, expected {}x{}",
                    image.dimensions(),
                    size.width,
                    size.height
                )
                .into());
            }
            let started = Instant::now();
            image.save(&path)?;
            timings.png_write += started.elapsed();
            println!("{}", path.display());
        }
        Ok(timings)
    }
}

impl Size {
    pub fn from_preset(width: u32, height: u32) -> Self {
        let divisor = gcd(width, height);
        Self {
            width,
            height,
            aspect: (width / divisor, height / divisor),
        }
    }

    pub fn name(&self) -> String {
        format!(
            "{}x{}-{}x{}",
            self.width, self.height, self.aspect.0, self.aspect.1
        )
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

pub struct CaptureTextures<'a> {
    assets: &'a AssetManager,
    keys: RefCell<BTreeSet<String>>,
}

impl<'a> CaptureTextures<'a> {
    pub fn new(assets: &'a AssetManager) -> Self {
        Self {
            assets,
            keys: RefCell::default(),
        }
    }

    pub fn missing_keys(self) -> Vec<String> {
        self.keys
            .into_inner()
            .into_iter()
            .filter(|key| !self.assets.has_uploaded_texture_key(key))
            .collect()
    }
}

impl TextureContext for CaptureTextures<'_> {
    fn texture_registry_generation(&self) -> u64 {
        self.assets.texture_context().revision()
    }

    fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
        self.assets.texture_context().texture_dims(key)
    }

    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        self.assets.texture_context().sprite_sheet_dims(key)
    }

    fn texture_handle(&self, key: &str) -> TextureHandle {
        self.keys.borrow_mut().insert(key.to_owned());
        self.assets.texture_context().texture_handle(key)
    }
}

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init_logger() -> Result<()> {
    log::set_logger(&Logger).map_err(|error| error.to_string())?;
    log::set_max_level(log::LevelFilter::Warn);
    Ok(())
}

pub fn initialize_runtime(dirs: &deadsync_config::dirs::AppDirs) -> Result<()> {
    dirs.ensure_dirs_exist();
    deadsync_assets::init_paths(dirs.asset_paths(None))?;
    deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())?;
    deadsync_profile::app_runtime::init_paths(
        dirs.profiles_root(),
        dirs.default_player_options_path(),
        dirs.pad_config_path(),
    )?;
    deadsync_simfile::app_runtime::init_paths(deadsync_simfile::app_runtime::ScanPaths {
        song_cache: dirs.song_cache_dir(),
        extra_songs: Vec::new(),
        extra_courses: Vec::new(),
        autogen_courses: dirs.courses_dir(),
        song_movies: Vec::new(),
        random_movies: Vec::new(),
        bg_animations: Vec::new(),
    })?;
    Ok(())
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Simply Love crate is under the workspace crates directory")
        .to_path_buf()
}

pub fn create_backend(window: Arc<winit::window::Window>) -> Result<Backend> {
    Ok(deadlib_render::create_backend(
        BackendType::VulkanWgpu,
        window,
        Metrics::centered(640.0, 480.0).projection(),
        false,
        PresentModePolicy::Immediate,
        false,
        true,
    )?)
}

pub fn load_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    key: &str,
    model: bool,
) -> Result<()> {
    if assets.has_uploaded_texture_key(key) || assets.load_generated_texture(backend, key, None)? {
        return Ok(());
    }
    assets
        .load_texture(
            backend,
            &deadsync_assets::textures::texture_decode_job(key, model),
        )
        .map_err(|error| format!("cannot load capture texture '{key}': {error}").into())
}

pub fn load_initial_assets(assets: &mut AssetManager, backend: &mut Backend) -> Result<()> {
    deadsync_assets::load_initial_assets(
        assets,
        backend,
        deadsync_theme_simply_love::asset_manifest(),
    )?;
    Ok(())
}
