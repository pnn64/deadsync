use deadlib_assets::AssetManager;
use deadlib_present::{compose::TextureContext, space::Metrics, texture::TextureMeta};
use deadlib_render::{
    Backend, BackendType, PresentModePolicy, ProjectionMatrix, RenderFrame, TextureHandle,
};
use image::ImageEncoder;
use std::{
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
    error::Error,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc, mpsc::SyncSender},
    time::{Duration, Instant},
};

pub(crate) type RenderAssets = AssetManager<deadlib_render::Texture>;

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

const CAPTURE_READBACK_DEPTH: usize = 3;
const PREPARED_FRAME_QUEUE_DEPTH: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub aspect: (u32, u32),
}

struct PendingCapture {
    path: PathBuf,
    expected_dimensions: (u32, u32),
    timings: Arc<Mutex<CaptureTimings>>,
}

type PngJob = (PathBuf, image::RgbaImage, Arc<Mutex<CaptureTimings>>);
type PipelineResponse<T> = SyncSender<std::result::Result<T, String>>;

struct PngWriter {
    sender: Option<SyncSender<PngJob>>,
    workers: Vec<std::thread::JoinHandle<std::result::Result<(), String>>>,
    startup_elapsed: Duration,
    worker_count: usize,
    startup_recorded: bool,
    last_timings: Option<Arc<Mutex<CaptureTimings>>>,
}

#[derive(Clone, Default)]
pub struct PngTimings {
    pub worker_startup: Duration,
    pub worker_count: usize,
    pub worker_time: Duration,
    pub drain_time: Duration,
}

enum ProducerCommand {
    Frame {
        frame: RenderFrame,
        path: PathBuf,
        prepare: Duration,
    },
    LoadTextures {
        textures: Vec<(String, bool)>,
        response: PipelineResponse<()>,
    },
    Png {
        path: PathBuf,
        image: image::RgbaImage,
        prepare: Duration,
    },
    Finished {
        result: std::result::Result<(), String>,
        frame_queue_wait: Duration,
        texture_wait: Duration,
        png_queue_wait: Duration,
    },
}

pub struct CapturePipeline {
    backend: Backend,
    assets: Arc<Mutex<RenderAssets>>,
    pending: VecDeque<PendingCapture>,
    current_dimensions: Option<(u32, u32)>,
    png_writer: Option<PngWriter>,
}

pub struct CaptureProducer {
    sender: SyncSender<ProducerCommand>,
    assets: Arc<Mutex<RenderAssets>>,
    frame_queue_wait: Duration,
    texture_wait: Duration,
    png_queue_wait: Duration,
}

pub struct CaptureTimings {
    pub render: Duration,
    pub readback: Duration,
    pub png: PngTimings,
}

impl Default for CaptureTimings {
    fn default() -> Self {
        Self {
            render: Duration::ZERO,
            readback: Duration::ZERO,
            png: PngTimings::default(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ScreenshotBackendTimings {
    pub acquire_us: u64,
    pub gpu_wait_us: u64,
    pub present_us: u64,
    pub suboptimal_frames: u32,
}

pub struct ScreenshotTimer {
    started: Instant,
    resize: Duration,
    prepare: Duration,
    elapsed: Option<Duration>,
    capture_timings: Arc<Mutex<CaptureTimings>>,
    frame_queue_wait: Duration,
    texture_wait: Duration,
    png_queue_wait: Duration,
    texture_load: Duration,
}

impl ScreenshotTimer {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            resize: Duration::ZERO,
            prepare: Duration::ZERO,
            elapsed: None,
            capture_timings: Arc::new(Mutex::new(CaptureTimings::default())),
            frame_queue_wait: Duration::ZERO,
            texture_wait: Duration::ZERO,
            png_queue_wait: Duration::ZERO,
            texture_load: Duration::ZERO,
        }
    }

    pub fn record_resize(&mut self, elapsed: Duration) {
        self.resize += elapsed;
    }

    pub fn record_prepare(&mut self, elapsed: Duration) {
        self.prepare += elapsed;
    }

    pub fn record_producer_waits(
        &mut self,
        frame_queue_wait: Duration,
        texture_wait: Duration,
        png_queue_wait: Duration,
    ) {
        self.frame_queue_wait += frame_queue_wait;
        self.texture_wait += texture_wait;
        self.png_queue_wait += png_queue_wait;
    }

    pub fn record_texture_load(&mut self, elapsed: Duration) {
        self.texture_load += elapsed;
    }

    fn record_render(&mut self, elapsed: Duration) -> Result<()> {
        self.capture_timings
            .lock()
            .map_err(|error| format!("screenshot timing lock failed: {error}"))?
            .render += elapsed;
        Ok(())
    }

    fn finish_resolution(&mut self) {
        self.elapsed = Some(self.started.elapsed());
    }

    pub fn report(
        &self,
        exporter: &str,
        size: &Size,
        backend: Option<ScreenshotBackendTimings>,
    ) -> Result<()> {
        let timings = self
            .capture_timings
            .lock()
            .map_err(|error| format!("screenshot timing lock failed: {error}"))?;
        let summary = format!(
            "{exporter} resolution={} elapsed={:.2}s resize={:.2}s prepare={:.2}s render={:.2}s readback={:.2}s png_worker_startup={:.3}ms png_workers={} png_final_drain={:.2}s png_worker={:.2}s frame_queue_wait={:.2}s texture_wait={:.2}s texture_load={:.2}s png_queue_wait={:.2}s",
            size.name(),
            self.elapsed
                .unwrap_or_else(|| self.started.elapsed())
                .as_secs_f64(),
            self.resize.as_secs_f64(),
            self.prepare.as_secs_f64(),
            timings.render.as_secs_f64(),
            timings.readback.as_secs_f64(),
            timings.png.worker_startup.as_secs_f64() * 1_000.0,
            timings.png.worker_count,
            timings.png.drain_time.as_secs_f64(),
            timings.png.worker_time.as_secs_f64(),
            self.frame_queue_wait.as_secs_f64(),
            self.texture_wait.as_secs_f64(),
            self.texture_load.as_secs_f64(),
            self.png_queue_wait.as_secs_f64(),
        );
        if let Some(backend) = backend {
            eprintln!(
                "{summary} acquire={:.2}s gpu_wait={:.2}s present={:.2}s suboptimal={}",
                backend.acquire_us as f64 / 1_000_000.0,
                backend.gpu_wait_us as f64 / 1_000_000.0,
                backend.present_us as f64 / 1_000_000.0,
                backend.suboptimal_frames,
            );
        } else {
            eprintln!("{summary}");
        }
        Ok(())
    }
}

fn write_fast_png(path: &Path, image: &image::RgbaImage) -> image::ImageResult<()> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    encode_fast_png(&mut writer, image)?;
    writer.flush()?;
    Ok(())
}

fn encode_fast_png<W: Write>(writer: W, image: &image::RgbaImage) -> image::ImageResult<()> {
    image::codecs::png::PngEncoder::new_with_quality(
        writer,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
}

impl PngWriter {
    fn new() -> Result<Self> {
        let startup_started = Instant::now();
        let worker_count =
            std::thread::available_parallelism().map_or(1, |count| count.get().min(4));
        let (sender, receiver) = mpsc::sync_channel::<PngJob>(worker_count * 2);
        let (ready_sender, ready_receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut writer = Self {
            sender: Some(sender),
            workers: Vec::with_capacity(worker_count),
            startup_elapsed: Duration::ZERO,
            worker_count,
            startup_recorded: false,
            last_timings: None,
        };

        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let ready_sender = ready_sender.clone();
            let worker = std::thread::Builder::new()
                .name(format!("screenshot-png-{index}"))
                .spawn(move || {
                    ready_sender
                        .send(())
                        .map_err(|error| format!("PNG worker startup failed: {error}"))?;
                    let mut error = None;
                    loop {
                        let job = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return Err("PNG writer queue was poisoned".to_owned()),
                        };
                        let Ok((path, image, timings)) = job else {
                            break;
                        };
                        let started = Instant::now();
                        let result = write_fast_png(&path, &image);
                        let elapsed = started.elapsed();
                        match timings.lock() {
                            Ok(mut timings) => timings.png.worker_time += elapsed,
                            Err(lock_error) => {
                                error.get_or_insert_with(|| {
                                    format!("PNG timing lock failed: {lock_error}")
                                });
                            }
                        }
                        match result {
                            Ok(()) => println!("{}", path.display()),
                            Err(write_error) => {
                                error.get_or_insert_with(|| {
                                    format!("cannot write PNG {}: {write_error}", path.display())
                                });
                            }
                        }
                    }
                    error.map_or(Ok(()), Err)
                })?;
            writer.workers.push(worker);
        }

        drop(ready_sender);
        for _ in 0..worker_count {
            ready_receiver
                .recv()
                .map_err(|error| format!("PNG worker failed to start: {error}"))?;
        }
        writer.startup_elapsed = startup_started.elapsed();
        Ok(writer)
    }

    fn write(
        &mut self,
        path: PathBuf,
        image: image::RgbaImage,
        timings: Arc<Mutex<CaptureTimings>>,
    ) -> Result<()> {
        {
            let mut capture_timings = timings
                .lock()
                .map_err(|error| format!("screenshot timing lock failed: {error}"))?;
            capture_timings.png.worker_count = self.worker_count;
            if !self.startup_recorded {
                capture_timings.png.worker_startup = self.startup_elapsed;
                self.startup_recorded = true;
            }
        }
        self.sender
            .as_ref()
            .ok_or("PNG writer is closed")?
            .send((path, image, Arc::clone(&timings)))
            .map_err(|error| format!("PNG writer stopped unexpectedly: {error}"))?;
        self.last_timings = Some(timings);
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        let started = Instant::now();
        self.sender.take();
        let mut error = None;
        for worker in self.workers.drain(..) {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(worker_error)) => {
                    error.get_or_insert(worker_error);
                }
                Err(_) => {
                    error.get_or_insert_with(|| "PNG writer thread panicked".into());
                }
            }
        }
        if let Some(timings) = self.last_timings.take() {
            timings
                .lock()
                .map_err(|lock_error| format!("screenshot timing lock failed: {lock_error}"))?
                .png
                .drain_time += started.elapsed();
        }
        error.map_or(Ok(()), |error| Err(error.into()))
    }
}

impl Drop for PngWriter {
    fn drop(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl CapturePipeline {
    pub fn new(backend: Backend, assets: RenderAssets) -> Self {
        Self {
            backend,
            assets: Arc::new(Mutex::new(assets)),
            pending: VecDeque::with_capacity(CAPTURE_READBACK_DEPTH),
            current_dimensions: None,
            png_writer: None,
        }
    }

    pub fn begin_resolution(
        &mut self,
        size: &Size,
        projection: ProjectionMatrix,
    ) -> Result<Duration> {
        if self.current_dimensions.is_some() {
            return Err("previous capture resolution was not finished".into());
        }
        let started = Instant::now();
        self.backend.resize(size.width, size.height);
        self.backend.set_default_projection(projection);
        self.current_dimensions = Some((size.width, size.height));
        Ok(started.elapsed())
    }

    fn write_png(
        &mut self,
        path: PathBuf,
        image: image::RgbaImage,
        timings: Arc<Mutex<CaptureTimings>>,
    ) -> Result<()> {
        if self.png_writer.is_none() {
            self.png_writer = Some(PngWriter::new()?);
        }
        self.png_writer
            .as_mut()
            .ok_or("PNG writer was not initialized")?
            .write(path, image, timings)
    }

    fn capture_next(&mut self) -> Result<()> {
        let Some(pending) = self.pending.pop_front() else {
            return Ok(());
        };
        let started = Instant::now();
        let image = self.backend.capture_frame()?;
        let readback_elapsed = started.elapsed();
        {
            let mut timings = pending
                .timings
                .lock()
                .map_err(|error| format!("screenshot timing lock failed: {error}"))?;
            timings.readback += readback_elapsed;
        }
        if image.dimensions() != pending.expected_dimensions {
            return Err(format!(
                "capture returned {:?}, expected {}x{}",
                image.dimensions(),
                pending.expected_dimensions.0,
                pending.expected_dimensions.1
            )
            .into());
        }
        self.write_png(pending.path, image, pending.timings)
    }

    fn drain_if_full(&mut self) -> Result<()> {
        if self.pending.len() >= CAPTURE_READBACK_DEPTH {
            self.capture_next()?;
        }
        Ok(())
    }

    pub fn run_producer<F>(
        &mut self,
        timer: &mut ScreenshotTimer,
        prepare: F,
    ) -> Result<ScreenshotBackendTimings>
    where
        F: FnOnce(&mut CaptureProducer) -> Result<()> + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(PREPARED_FRAME_QUEUE_DEPTH);
        let producer_assets = Arc::clone(&self.assets);
        let producer_thread = std::thread::Builder::new()
            .name("screenshot-frame-producer".into())
            .spawn(move || {
                let mut producer = CaptureProducer {
                    sender,
                    assets: producer_assets,
                    frame_queue_wait: Duration::ZERO,
                    texture_wait: Duration::ZERO,
                    png_queue_wait: Duration::ZERO,
                };
                let result = prepare(&mut producer).map_err(|error| error.to_string());
                let _ = producer.sender.send(ProducerCommand::Finished {
                    result,
                    frame_queue_wait: producer.frame_queue_wait,
                    texture_wait: producer.texture_wait,
                    png_queue_wait: producer.png_queue_wait,
                });
            })?;

        let mut acquire_us = 0u64;
        let mut gpu_wait_us = 0u64;
        let mut present_us = 0u64;
        let mut suboptimal_frames = 0u32;
        let mut texture_load = Duration::ZERO;
        let mut producer_result = None;
        let mut consumer_error: Option<String> = None;

        while let Ok(command) = receiver.recv() {
            match command {
                ProducerCommand::Frame {
                    frame,
                    path,
                    prepare,
                } => {
                    timer.record_prepare(prepare);
                    if consumer_error.is_some() {
                        continue;
                    }
                    let result = (|| {
                        let dimensions = self
                            .current_dimensions
                            .ok_or_else(|| "capture resolution was not started".to_owned())?;
                        self.backend.request_screenshot_offline();
                        let assets = self.assets.lock().map_err(|error| {
                            format!("capture pipeline asset lock failed: {error}")
                        })?;
                        let started = Instant::now();
                        let stats = self
                            .backend
                            .draw(&frame, assets.textures(), false)
                            .map_err(|error| error.to_string())?;
                        drop(assets);
                        timer
                            .record_render(started.elapsed())
                            .map_err(|error| error.to_string())?;
                        self.pending.push_back(PendingCapture {
                            path,
                            expected_dimensions: dimensions,
                            timings: Arc::clone(&timer.capture_timings),
                        });
                        self.drain_if_full().map_err(|error| error.to_string())?;
                        Ok(stats)
                    })();
                    match result {
                        Ok(stats) => {
                            acquire_us += u64::from(stats.acquire_us);
                            gpu_wait_us += u64::from(stats.gpu_wait_us);
                            present_us += u64::from(stats.present_us);
                            suboptimal_frames += u32::from(stats.present_stats.suboptimal);
                        }
                        Err(error) => consumer_error = Some(error),
                    }
                }
                ProducerCommand::LoadTextures { textures, response } => {
                    let started = Instant::now();
                    let result = if let Some(error) = consumer_error.as_ref() {
                        Err(error.clone())
                    } else {
                        (|| {
                            let mut assets = self.assets.lock().map_err(|error| {
                                format!("capture pipeline asset lock failed: {error}")
                            })?;
                            for (key, model) in textures {
                                load_texture(&mut assets, &mut self.backend, &key, model)
                                    .map_err(|error| error.to_string())?;
                            }
                            Ok(())
                        })()
                    };
                    texture_load += started.elapsed();
                    let _ = response.send(result);
                }
                ProducerCommand::Png {
                    path,
                    image,
                    prepare,
                } => {
                    timer.record_prepare(prepare);
                    if consumer_error.is_none() {
                        if let Err(error) =
                            self.write_png(path, image, Arc::clone(&timer.capture_timings))
                        {
                            consumer_error = Some(error.to_string());
                        }
                    }
                }
                ProducerCommand::Finished {
                    result,
                    frame_queue_wait,
                    texture_wait,
                    png_queue_wait,
                } => {
                    producer_result = Some(result);
                    timer.record_producer_waits(frame_queue_wait, texture_wait, png_queue_wait);
                    break;
                }
            }
        }

        producer_thread
            .join()
            .map_err(|_| "frame producer thread panicked")?;
        if producer_result.is_none() {
            return Err("frame producer stopped without reporting completion".into());
        }
        timer.record_texture_load(texture_load);
        self.current_dimensions = None;
        timer.finish_resolution();
        if let Some(error) = consumer_error {
            return Err(error.into());
        }
        producer_result
            .ok_or("frame producer stopped without reporting completion")?
            .map_err(std::io::Error::other)?;
        Ok(ScreenshotBackendTimings {
            acquire_us,
            gpu_wait_us,
            present_us,
            suboptimal_frames,
        })
    }

    pub fn finish(mut self) -> Result<()> {
        if self.current_dimensions.is_some() {
            return Err("cannot finish capture pipeline during a resolution".into());
        }
        while !self.pending.is_empty() {
            self.capture_next()?;
        }
        if let Some(writer) = self.png_writer.take() {
            writer.finish()?;
        }
        let mut assets = Arc::try_unwrap(self.assets)
            .map_err(|_| "capture pipeline still has shared asset references")?
            .into_inner()
            .map_err(|error| format!("capture pipeline asset lock failed: {error}"))?;
        self.backend.dispose_textures(&mut assets.take_textures());
        self.backend.cleanup();
        Ok(())
    }
}

impl CaptureProducer {
    pub fn with_assets<T>(&mut self, f: impl FnOnce(&mut RenderAssets) -> Result<T>) -> Result<T> {
        let mut assets = self
            .assets
            .lock()
            .map_err(|error| format!("capture pipeline asset lock failed: {error}"))?;
        f(&mut assets)
    }

    pub fn load_textures(&mut self, textures: Vec<(String, bool)>) -> Result<()> {
        if textures.is_empty() {
            return Ok(());
        }
        let started = Instant::now();
        let (response, receiver) = mpsc::sync_channel(1);
        let result: Result<()> = match self
            .sender
            .send(ProducerCommand::LoadTextures { textures, response })
        {
            Ok(()) => match receiver.recv() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(std::io::Error::other(error).into()),
                Err(error) => Err(format!("capture consumer stopped unexpectedly: {error}").into()),
            },
            Err(error) => Err(format!("capture consumer stopped unexpectedly: {error}").into()),
        };
        self.texture_wait += started.elapsed();
        result
    }

    pub fn submit(&mut self, frame: RenderFrame, path: PathBuf, prepare: Duration) -> Result<()> {
        let started = Instant::now();
        let result: Result<()> = self
            .sender
            .send(ProducerCommand::Frame {
                frame,
                path,
                prepare,
            })
            .map_err(|error| format!("capture consumer stopped unexpectedly: {error}").into());
        self.frame_queue_wait += started.elapsed();
        result
    }

    pub fn write_png(
        &mut self,
        path: PathBuf,
        image: image::RgbaImage,
        prepare: Duration,
    ) -> Result<()> {
        let started = Instant::now();
        let result: Result<()> = self
            .sender
            .send(ProducerCommand::Png {
                path,
                image,
                prepare,
            })
            .map_err(|error| format!("capture consumer stopped unexpectedly: {error}").into());
        self.png_queue_wait += started.elapsed();
        result
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
    assets: &'a RenderAssets,
    keys: RefCell<BTreeSet<String>>,
}

impl<'a> CaptureTextures<'a> {
    pub fn new(assets: &'a RenderAssets) -> Self {
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
    deadsync_simfile::app_runtime::init_paths(
        deadsync_simfile::app_runtime::ScanPaths {
            song_cache: dirs.song_cache_dir(),
            extra_songs: Vec::new(),
            extra_courses: Vec::new(),
            autogen_courses: dirs.courses_dir(),
            song_movies: Vec::new(),
            random_movies: Vec::new(),
            bg_animations: Vec::new(),
        },
        |_| Ok(0.0),
    )?;
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
    assets: &mut RenderAssets,
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

pub fn load_initial_assets(assets: &mut RenderAssets, backend: &mut Backend) -> Result<()> {
    deadsync_assets::load_initial_assets(
        assets,
        backend,
        deadsync_theme_simply_love::asset_manifest(),
    )?;
    Ok(())
}
