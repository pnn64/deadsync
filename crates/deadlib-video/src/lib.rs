pub use deadlib_render_core::{Yuv420Image, YuvConversion};
mod avi;
mod matroska;
mod mp4;
mod native;
mod packet;

use deadlib_image::RgbaImage;
use log::warn;
use serde::Deserialize;
use std::{
    ffi::OsString,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    },
    thread::{self, JoinHandle},
};

const DEFAULT_FPS: f32 = 30.0;
const MAX_FPS: f32 = 60.0;
const FRAME_QUEUE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_sec: Option<f32>,
    pub looped: bool,
    pub conversion: YuvConversion,
}

pub struct LoadedVideo {
    pub info: Info,
    pub poster: RgbaImage,
    pub player: Player,
}

struct QueuedFrame {
    pts_sec: f32,
    image: Yuv420Image,
}

pub struct VideoFrame {
    image: Option<Yuv420Image>,
    recycle_tx: Option<SyncSender<Vec<u8>>>,
}

impl VideoFrame {
    #[must_use]
    /// # Panics
    ///
    /// Panics if an internal state invariant is violated.
    pub fn into_upload_parts(mut self) -> (Yuv420Image, SyncSender<Vec<u8>>) {
        let image = self
            .image
            .take()
            .expect("video frame image must be present");
        let recycle_tx = self
            .recycle_tx
            .take()
            .expect("video frame recycler must be present");
        (image, recycle_tx)
    }
}

impl Drop for VideoFrame {
    fn drop(&mut self) {
        let (Some(image), Some(recycle_tx)) = (self.image.take(), self.recycle_tx.take()) else {
            return;
        };
        recycle_frame_buffer(&recycle_tx, image);
    }
}

struct DecoderWorker {
    frame_tx: SyncSender<QueuedFrame>,
    recycle_tx: SyncSender<Vec<u8>>,
    buffers: FrameBuffers,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

struct FrameBuffers {
    recycled: Receiver<Vec<u8>>,
    frame_bytes: usize,
    unallocated: usize,
    misses: Arc<AtomicU64>,
}

impl FrameBuffers {
    fn take(&mut self) -> Vec<u8> {
        match self.recycled.try_recv() {
            Ok(mut raw) => {
                if raw.len() != self.frame_bytes {
                    raw.resize(self.frame_bytes, 0);
                }
                raw
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                if self.unallocated == 0 {
                    self.misses.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.unallocated -= 1;
                }
                vec![0; self.frame_bytes]
            }
        }
    }
}

/// Asynchronous decoder with a bounded frame queue and reusable pixel pool.
///
/// The worker owns decode writes; the game thread uses a nonblocking receive and
/// the upload queue returns displayed buffers. Both channels live for one
/// `Player`. The pool grows on demand to cover the frame queue, one in-flight
/// upload, and one worker decode. Exhaustion falls back to one allocation and
/// increments `buffer_pool_misses`; there is no scan or eviction. Buffers are
/// destroyed with the player and outstanding uploads. Normal game-thread work is
/// bounded by the frames already waiting in the channel.
pub struct Player {
    info: Info,
    frame_rx: Option<Receiver<QueuedFrame>>,
    next_frame: Option<QueuedFrame>,
    recycle_tx: SyncSender<Vec<u8>>,
    buffer_pool_misses: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    worker: Option<JoinHandle<()>>,
}

impl Player {
    #[inline]
    pub fn take_due_frame(&mut self, play_time_sec: f32) -> Option<VideoFrame> {
        let target = clamp_play_time(play_time_sec, self.info);
        let mut latest = None;
        loop {
            let frame = match self.next_frame.as_ref() {
                Some(frame) if frame.pts_sec > target => break,
                Some(_) => self
                    .next_frame
                    .take()
                    .expect("buffered frame was just checked"),
                None => match self.frame_rx.as_ref()?.try_recv() {
                    Ok(frame) => {
                        if frame.pts_sec > target {
                            self.next_frame = Some(frame);
                            break;
                        }
                        frame
                    }
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                },
            };
            if let Some(image) = latest.take() {
                recycle_frame_buffer(&self.recycle_tx, image);
            }
            latest = Some(frame.image);
        }
        latest.map(|image| VideoFrame {
            image: Some(image),
            recycle_tx: Some(self.recycle_tx.clone()),
        })
    }

    pub fn retire_async(self) {
        if let Err(e) = thread::Builder::new()
            .name("video-retire".to_owned())
            .spawn(move || drop(self))
        {
            warn!("Failed to spawn video retirement worker: {e}");
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn buffer_pool_misses(&self) -> u64 {
        self.buffer_pool_misses.load(Ordering::Relaxed)
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Disconnect a worker blocked on a full decoded-frame channel before
        // joining it. Worker-domain blocking is intentional; the game thread
        // only performs `try_recv`.
        self.frame_rx.take();
        if let Ok(mut child_slot) = self.child.lock()
            && let Some(child) = child_slot.as_mut()
        {
            let _ = child.kill();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(mut child_slot) = self.child.lock()
            && let Some(mut child) = child_slot.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        let misses = self.buffer_pool_misses();
        if misses > 0 {
            warn!("Video decoder pixel pool was exhausted {misses} times");
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn open(path: &Path, looped: bool) -> Result<LoadedVideo, String> {
    let native_failure = match native::Video::open(path, looped) {
        Ok(Some(video)) => {
            let info = video.info;
            let (poster_tx, poster_rx) = sync_channel(1);
            let player = open_native_player(video, path, Some(poster_tx))?;
            match poster_rx.recv() {
                Ok(Ok(poster)) => {
                    return Ok(LoadedVideo {
                        info,
                        poster,
                        player,
                    });
                }
                Ok(Err(error)) => error,
                Err(_) => "native video decoder stopped before its first picture".into(),
            }
        }
        Ok(None) => "unsupported native video format".into(),
        Err(error) => error,
    };
    let info = probe(path, looped).map_err(|error| format!("{native_failure}. {error}"))?;
    let poster = load_poster_with_info(path, info)?;
    let player = open_player_with_info(path, info)?;
    Ok(LoadedVideo {
        info,
        poster,
        player,
    })
}

pub fn load_poster(path: &Path) -> Result<RgbaImage, String> {
    if let Some(mut video) = native_video(path, false)
        && let Ok(poster) = video.poster()
    {
        return Ok(poster);
    }
    load_poster_with_info(path, probe(path, false)?)
}

pub fn open_player(path: &Path, looped: bool) -> Result<Player, String> {
    if let Some(video) = native_video(path, looped) {
        return open_native_player(video, path, None);
    }
    open_player_with_info(path, probe(path, looped)?)
}

/// Probe decoded texture dimensions without starting a decoder. Load-time only.
pub fn dimensions(path: &Path) -> Result<(u32, u32), String> {
    let info = match native_video(path, false) {
        Some(video) => video.info,
        None => probe(path, false)?,
    };
    Ok((info.width, info.height))
}

fn native_video(path: &Path, looped: bool) -> Option<native::Video> {
    match native::Video::open(path, looped) {
        Ok(video) => video,
        Err(error) => {
            log::debug!("Native video index for '{}': {error}", path.display());
            None
        }
    }
}

fn open_player_with_info(path: &Path, info: Info) -> Result<Player, String> {
    open_with_worker(info, |worker| {
        spawn_worker(path.to_path_buf(), info, worker)
    })
}

fn open_native_player(
    video: native::Video,
    path: &Path,
    poster: Option<SyncSender<Result<RgbaImage, String>>>,
) -> Result<Player, String> {
    let path = path.to_owned();
    open_with_worker(video.info, |worker| {
        thread::Builder::new()
            .name("video-decode".into())
            .spawn(move || decode_native(video, worker, poster, &path))
            .map_err(|error| format!("could not start native video decoder: {error}"))
    })
}

fn open_with_worker(
    info: Info,
    spawn: impl FnOnce(DecoderWorker) -> Result<JoinHandle<()>, String>,
) -> Result<Player, String> {
    let max_frames = queue_capacity(info);
    let (frame_tx, frame_rx) = sync_channel(max_frames);
    // Besides the bounded queue, one buffer can be in the upload queue and one
    // can be held by the worker while it blocks on a full decoded-frame queue.
    let buffer_count = max_frames.saturating_add(2);
    let (recycle_tx, recycle_rx) = sync_channel(buffer_count);
    let stop = Arc::new(AtomicBool::new(false));
    let buffer_pool_misses = Arc::new(AtomicU64::new(0));
    let child = Arc::new(Mutex::new(None));
    let worker = spawn(DecoderWorker {
        frame_tx,
        recycle_tx: recycle_tx.clone(),
        buffers: FrameBuffers {
            recycled: recycle_rx,
            frame_bytes: yuv420_frame_bytes(info),
            unallocated: buffer_count,
            misses: buffer_pool_misses.clone(),
        },
        stop: stop.clone(),
        child: child.clone(),
    })?;
    Ok(Player {
        info,
        frame_rx: Some(frame_rx),
        next_frame: None,
        recycle_tx,
        buffer_pool_misses,
        stop,
        child,
        worker: Some(worker),
    })
}

fn decode_native(
    mut video: native::Video,
    mut worker: DecoderWorker,
    mut poster: Option<SyncSender<Result<RgbaImage, String>>>,
    path: &Path,
) {
    let info = video.info;
    let frame_step = 1.0 / info.fps.max(1.0);
    let mut frame_index = 0u64;
    loop {
        if worker.stop.load(Ordering::Relaxed) {
            return;
        }
        let start = frame_index;
        let result = video.decode_one_pass(
            || worker.stop.load(Ordering::Relaxed),
            |frame, slots| {
                if let Some(sender) = poster.take() {
                    let image = native::to_rgba(frame, info);
                    let converted = image.is_ok();
                    if sender.send(image).is_err() || !converted {
                        return Ok(false);
                    }
                }
                for _ in 0..slots {
                    if worker.stop.load(Ordering::Relaxed) {
                        return Ok(false);
                    }
                    let mut raw = worker.buffers.take();
                    native::pack(frame, info, &mut raw)?;
                    let image = Yuv420Image::from_raw_with_conversion(
                        info.width,
                        info.height,
                        raw,
                        info.conversion,
                    )
                    .ok_or("native decoder produced an invalid YUV420 frame")?;
                    let pts_sec = frame_index as f32 * frame_step;
                    frame_index = frame_index.saturating_add(1);
                    if let Err(error) = worker.frame_tx.send(QueuedFrame { pts_sec, image }) {
                        recycle_frame_buffer(&worker.recycle_tx, error.0.image);
                        return Ok(false);
                    }
                }
                Ok(true)
            },
        );
        match result {
            Ok(true) if info.looped && frame_index != start => {}
            Ok(_) => {
                if let Some(sender) = poster.take() {
                    let _ = sender.send(Err("video has no displayed picture".into()));
                }
                return;
            }
            Err(error) => {
                if let Some(sender) = poster.take() {
                    let _ = sender.send(Err(error));
                } else if !worker.stop.load(Ordering::Relaxed) {
                    warn!(
                        "Native video decode failed for '{}': {error}",
                        path.display()
                    );
                    // Start from the same clock so FFmpeg's frame-rate filter
                    // and loop boundaries remain aligned with queued frames.
                    match open_ffmpeg_stream(path, info, &worker) {
                        Ok(stdout) => decode_loop(stdout, info, worker, frame_index),
                        Err(error) => warn!("Video compatibility decoder unavailable: {error}"),
                    }
                }
                return;
            }
        }
    }
}

fn spawn_worker(
    path: PathBuf,
    info: Info,
    worker: DecoderWorker,
) -> Result<JoinHandle<()>, String> {
    let stdout = open_ffmpeg_stream(&path, info, &worker)?;
    Ok(thread::spawn(move || decode_loop(stdout, info, worker, 0)))
}

fn open_ffmpeg_stream(
    path: &Path,
    info: Info,
    worker: &DecoderWorker,
) -> Result<ChildStdout, String> {
    let mut child = decode_command(path, info).spawn().map_err(|e| {
        format!(
            "failed to start ffmpeg decoder for '{}': {e}",
            path.display()
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        format!(
            "ffmpeg decoder for '{}' did not expose stdout",
            path.display()
        )
    })?;
    if let Ok(mut slot) = worker.child.lock() {
        *slot = Some(child);
    }
    Ok(stdout)
}

fn decode_loop(mut stdout: ChildStdout, info: Info, worker: DecoderWorker, skip_frames: u64) {
    let DecoderWorker {
        frame_tx,
        recycle_tx,
        mut buffers,
        stop,
        child: child_slot,
    } = worker;
    let frame_step = 1.0 / info.fps.max(1.0);
    let mut frame_index = 0u64;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut raw = buffers.take();
        match read_frame(&mut stdout, &mut raw) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                warn!("Video decode stream read failed: {e}");
                break;
            }
        }
        let Some(image) =
            Yuv420Image::from_raw_with_conversion(info.width, info.height, raw, info.conversion)
        else {
            warn!("Video decoder produced an invalid YUV420 frame");
            break;
        };
        let pts_sec = frame_index as f32 * frame_step;
        frame_index = frame_index.saturating_add(1);
        if frame_index <= skip_frames {
            recycle_frame_buffer(&recycle_tx, image);
            continue;
        }
        if let Err(error) = frame_tx.send(QueuedFrame { pts_sec, image }) {
            recycle_frame_buffer(&recycle_tx, error.0.image);
            break;
        }
    }

    if let Ok(mut slot) = child_slot.lock()
        && let Some(mut child) = slot.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn recycle_frame_buffer(recycle_tx: &SyncSender<Vec<u8>>, image: Yuv420Image) {
    let _ = recycle_tx.try_send(image.into_raw());
}

fn load_poster_with_info(path: &Path, info: Info) -> Result<RgbaImage, String> {
    let frame_bytes = rgba_frame_bytes(info);
    let mut child = poster_command(path, info).spawn().map_err(|e| {
        format!(
            "failed to start ffmpeg poster decode for '{}': {e}",
            path.display()
        )
    })?;
    let Some(mut stdout) = child.stdout.take() else {
        return Err(format!(
            "ffmpeg poster decode for '{}' did not expose stdout",
            path.display()
        ));
    };
    let mut raw = vec![0; frame_bytes];
    let read_ok = match read_frame(&mut stdout, &mut raw) {
        Ok(ok) => ok,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "failed to read poster frame from '{}': {e}",
                path.display()
            ));
        }
    };
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for poster decode '{}': {e}", path.display()))?;
    if !read_ok || !status.success() {
        return Err(format!(
            "failed to decode poster frame from '{}'",
            path.display()
        ));
    }
    RgbaImage::from_raw(info.width, info.height, raw).ok_or_else(|| {
        format!(
            "poster frame from '{}' did not match probed dimensions",
            path.display()
        )
    })
}

fn probe(path: &Path, looped: bool) -> Result<Info, String> {
    let output = tool_command("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height,avg_frame_rate,r_frame_rate,duration,color_space,color_range:format=duration")
        .arg("-of")
        .arg("json")
        .arg(path)
        .output()
        .map_err(|e| format!("failed to start ffprobe for '{}': {e}", path.display()))?;
    if !output.status.success() {
        return Err(format!("ffprobe failed for '{}'", path.display()));
    }

    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse ffprobe JSON for '{}': {e}", path.display()))?;
    let stream = parsed
        .streams
        .first()
        .ok_or_else(|| format!("no video stream found in '{}'", path.display()))?;
    let width = stream
        .width
        .ok_or_else(|| format!("ffprobe did not report width for '{}'", path.display()))?;
    let height = stream
        .height
        .ok_or_else(|| format!("ffprobe did not report height for '{}'", path.display()))?;
    let width = even_dimension(width)
        .ok_or_else(|| format!("video width is invalid for '{}'", path.display()))?;
    let height = even_dimension(height)
        .ok_or_else(|| format!("video height is invalid for '{}'", path.display()))?;
    let fps = parse_rate(stream.avg_frame_rate)
        .or_else(|| parse_rate(stream.r_frame_rate))
        .unwrap_or(DEFAULT_FPS)
        .clamp(1.0, MAX_FPS);
    let duration_sec = parse_duration(stream.duration)
        .or_else(|| parse_duration(parsed.format.as_ref().and_then(|fmt| fmt.duration)));
    let conversion = probe_conversion(stream, width, height);

    Ok(Info {
        width,
        height,
        fps,
        duration_sec,
        looped,
        conversion,
    })
}

fn decode_command(path: &Path, info: Info) -> Command {
    let mut cmd = tool_command("ffmpeg");
    cmd.arg("-v").arg("error");
    if info.looped {
        cmd.arg("-stream_loop").arg("-1");
    }
    cmd.arg("-i")
        .arg(path)
        .arg("-an")
        .arg("-sn")
        .arg("-vf")
        .arg(format!(
            "fps={:.6},pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv420p",
            info.fps
        ))
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd
}

fn poster_command(path: &Path, _info: Info) -> Command {
    let mut cmd = tool_command("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-an")
        .arg("-sn")
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("pad=ceil(iw/2)*2:ceil(ih/2)*2,format=rgba")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    cmd
}

fn read_frame(stdout: &mut ChildStdout, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match stdout.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partial rawvideo frame",
                ));
            }
            Ok(read) => filled += read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Absolute path of the runtime `bin/` directory the video tools load
/// from (`<current_dir>/bin`), where the in-app downloader installs them.
/// `None` only when the working directory can't be determined.
#[must_use]
pub fn runtime_bin_dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|dir| dir.join("bin"))
}

/// Whether the optional FFmpeg compatibility tools resolve.
#[must_use]
pub fn ffmpeg_available() -> bool {
    tool_is_available("ffmpeg") && tool_is_available("ffprobe")
}

/// True when `name` resolves in the runtime `bin/` or on `PATH`. The
/// `PATH` case is probed by spawning, since `resolve_tool_path` only
/// checks `bin/`.
fn tool_is_available(name: &str) -> bool {
    if resolve_tool_path(name).is_some() {
        return true;
    }
    let mut cmd = Command::new(name);
    cmd.arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    suppress_console(&mut cmd);
    cmd.status().map(|status| status.success()).unwrap_or(false)
}

fn tool_command(name: &str) -> Command {
    let mut cmd = resolve_tool_path(name)
        .map(Command::new)
        .unwrap_or_else(|| Command::new(name));
    suppress_console(&mut cmd);
    cmd
}

#[cfg(windows)]
fn suppress_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn suppress_console(_cmd: &mut Command) {}

fn resolve_tool_path(name: &str) -> Option<PathBuf> {
    let runtime_bin = std::env::current_dir().ok().map(|dir| dir.join("bin"));
    resolve_tool_path_in_dirs(name, runtime_bin.as_deref())
}

fn resolve_tool_path_in_dirs(name: &str, runtime_bin: Option<&Path>) -> Option<PathBuf> {
    tool_path_in_dir(name, runtime_bin)
}

fn tool_path_in_dir(name: &str, dir: Option<&Path>) -> Option<PathBuf> {
    let dir = dir?;
    let primary = primary_tool_path(dir, name);
    if primary.is_file() {
        return Some(primary);
    }
    let fallback = fallback_tool_path(dir, name);
    fallback.is_file().then_some(fallback)
}

#[cfg(windows)]
fn primary_tool_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(exe_tool_name(name))
}

#[cfg(not(windows))]
fn primary_tool_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(windows)]
fn fallback_tool_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(not(windows))]
fn fallback_tool_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(exe_tool_name(name))
}

fn exe_tool_name(name: &str) -> OsString {
    let mut candidate = OsString::with_capacity(name.len() + 4);
    candidate.push(name);
    candidate.push(".exe");
    candidate
}

#[inline(always)]
fn clamp_play_time(play_time_sec: f32, info: Info) -> f32 {
    let play_time_sec = play_time_sec.max(0.0);
    match info.duration_sec {
        Some(duration) if !info.looped && duration.is_finite() && duration > 0.0 => {
            play_time_sec.min(duration)
        }
        _ => play_time_sec,
    }
}

#[inline(always)]
fn rgba_frame_bytes(info: Info) -> usize {
    usize::try_from(info.width)
        .ok()
        .and_then(|w| {
            usize::try_from(info.height)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .and_then(|px| px.checked_mul(4))
        .unwrap_or(0)
}

#[inline(always)]
fn yuv420_frame_bytes(info: Info) -> usize {
    Yuv420Image::packed_byte_len(info.width, info.height).unwrap_or(0)
}

#[inline(always)]
fn even_dimension(value: u32) -> Option<u32> {
    (value > 0)
        .then_some(value)
        .and_then(|value| value.checked_add(value % 2))
}

#[inline(always)]
fn queue_capacity(info: Info) -> usize {
    let frame_bytes = yuv420_frame_bytes(info).max(1);
    (FRAME_QUEUE_BYTES / frame_bytes).clamp(2, 24)
}

fn parse_rate(raw: Option<&str>) -> Option<f32> {
    let raw = raw?;
    if raw == "0/0" {
        return None;
    }
    let (num, den) = raw.split_once('/')?;
    let (num, den) = match (parse_ascii_u64(num), parse_ascii_u64(den)) {
        (Some(num), Some(den)) => (num as f64, den as f64),
        _ => (num.parse::<f64>().ok()?, den.parse::<f64>().ok()?),
    };
    if !num.is_finite() || !den.is_finite() || den <= 0.0 {
        return None;
    }
    let fps = (num / den) as f32;
    (fps.is_finite() && fps > 0.0).then_some(fps)
}

fn parse_ascii_u64(raw: &str) -> Option<u64> {
    (!raw.is_empty()).then_some(())?;
    raw.bytes().try_fold(0_u64, |value, byte| {
        let digit = byte.checked_sub(b'0').filter(|digit| *digit <= 9)?;
        value.checked_mul(10)?.checked_add(u64::from(digit))
    })
}

fn parse_duration(raw: Option<&str>) -> Option<f32> {
    let duration = raw?.parse::<f32>().ok()?;
    (duration.is_finite() && duration > 0.0).then_some(duration)
}

fn probe_conversion(stream: &ProbeStream, _width: u32, _height: u32) -> YuvConversion {
    let coeffs = match stream.color_space {
        Some("bt709" | "smpte240m") => [1.5748, -0.187_324, -0.468_124, 1.8556],
        Some("bt2020nc" | "bt2020c") => [1.4746, -0.164_553, -0.571_353, 1.8814],
        Some("fcc" | "bt470bg" | "smpte170m" | "smpte428") => {
            [1.402, -0.344_136, -0.714_136, 1.772]
        }
        _ => [1.402, -0.344_136, -0.714_136, 1.772],
    };
    if matches!(stream.color_range, Some("pc" | "jpeg")) {
        YuvConversion::full(coeffs)
    } else {
        YuvConversion::limited(coeffs)
    }
}

#[derive(Deserialize)]
struct ProbeOutput<'a> {
    #[serde(borrow)]
    streams: Vec<ProbeStream<'a>>,
    #[serde(borrow)]
    format: Option<ProbeFormat<'a>>,
}

#[derive(Deserialize)]
struct ProbeStream<'a> {
    width: Option<u32>,
    height: Option<u32>,
    #[serde(borrow)]
    avg_frame_rate: Option<&'a str>,
    #[serde(borrow)]
    r_frame_rate: Option<&'a str>,
    #[serde(borrow)]
    duration: Option<&'a str>,
    #[serde(borrow)]
    color_space: Option<&'a str>,
    #[serde(borrow)]
    color_range: Option<&'a str>,
}

#[derive(Deserialize)]
struct ProbeFormat<'a> {
    #[serde(borrow)]
    duration: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::{
        FrameBuffers, Info, Player, ProbeOutput, ProbeStream, QueuedFrame, Yuv420Image,
        YuvConversion, clamp_play_time, decode_command, fallback_tool_path, parse_duration,
        parse_rate, primary_tool_path, probe_conversion, resolve_tool_path_in_dirs,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc::sync_channel,
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "deadlib-video-{name}-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_tool(dir: &Path, name: &str) -> PathBuf {
        let path = primary_tool_path(dir, name);
        fs::write(&path, []).unwrap();
        path
    }

    #[test]
    fn parse_rate_handles_fraction() {
        let fps = parse_rate(Some("30000/1001")).unwrap();
        assert!((fps - 29.97003).abs() < 0.001);
    }

    #[test]
    fn parse_rate_preserves_general_float_fallback() {
        assert_eq!(parse_rate(Some("29.97/1.0")), Some(29.97));
        let numerator = "18446744073709551616".parse::<f64>().unwrap();
        assert_eq!(
            parse_rate(Some("18446744073709551616/1")),
            Some(numerator as f32)
        );
    }

    #[test]
    fn parse_rate_rejects_zero() {
        assert!(parse_rate(Some("0/0")).is_none());
    }

    #[test]
    fn parse_duration_rejects_negative() {
        assert!(parse_duration(Some("-1")).is_none());
    }

    #[test]
    fn resolve_tool_path_prefers_runtime_bin() {
        let runtime = TempDir::new("runtime");
        let runtime_bin = runtime.path().join("bin");
        fs::create_dir_all(&runtime_bin).unwrap();
        let runtime_tool = write_tool(&runtime_bin, "ffmpeg");
        let resolved = resolve_tool_path_in_dirs("ffmpeg", Some(runtime_bin.as_path()));
        assert_eq!(resolved.as_deref(), Some(runtime_tool.as_path()));
    }

    #[test]
    fn resolve_tool_path_returns_none_without_runtime_bin_tool() {
        let runtime = TempDir::new("runtime-miss");
        let runtime_bin = runtime.path().join("bin");
        fs::create_dir_all(&runtime_bin).unwrap();
        let resolved = resolve_tool_path_in_dirs("ffprobe", Some(runtime_bin.as_path()));
        assert!(resolved.is_none());
    }

    #[test]
    fn resolve_tool_path_preserves_platform_fallback() {
        let runtime = TempDir::new("runtime-fallback");
        let runtime_bin = runtime.path().join("bin");
        fs::create_dir_all(&runtime_bin).unwrap();
        let fallback = fallback_tool_path(&runtime_bin, "ffprobe");
        fs::write(&fallback, []).unwrap();

        let resolved = resolve_tool_path_in_dirs("ffprobe", Some(runtime_bin.as_path()));

        assert_eq!(resolved.as_deref(), Some(fallback.as_path()));
    }

    #[test]
    fn take_due_frame_recycles_skipped_frames() {
        let (recycle_tx, recycle_rx) = sync_channel(2);
        let (frame_tx, frame_rx) = sync_channel(2);
        assert!(
            frame_tx
                .try_send(QueuedFrame {
                    pts_sec: 0.0,
                    image: Yuv420Image::from_raw(2, 2, vec![1; 6]).unwrap(),
                })
                .is_ok()
        );
        assert!(
            frame_tx
                .try_send(QueuedFrame {
                    pts_sec: 1.0,
                    image: Yuv420Image::from_raw(2, 2, vec![2; 6]).unwrap(),
                })
                .is_ok()
        );
        let mut player = Player {
            info: Info {
                width: 2,
                height: 2,
                fps: 30.0,
                duration_sec: None,
                looped: false,
                conversion: YuvConversion::BT709_LIMITED,
            },
            frame_rx: Some(frame_rx),
            next_frame: None,
            recycle_tx,
            buffer_pool_misses: Arc::new(AtomicU64::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
            worker: None,
        };

        let frame = player.take_due_frame(1.0).unwrap();

        assert!(player.frame_rx.as_ref().unwrap().try_recv().is_err());
        assert_eq!(recycle_rx.try_recv().unwrap(), vec![1; 6]);
        drop(frame);
        assert_eq!(recycle_rx.try_recv().unwrap(), vec![2; 6]);
    }

    #[test]
    fn future_frame_poll_keeps_the_buffered_frame_in_place() {
        let (recycle_tx, _recycle_rx) = sync_channel(1);
        let (_frame_tx, frame_rx) = sync_channel(1);
        let mut player = Player {
            info: Info {
                width: 2,
                height: 2,
                fps: 30.0,
                duration_sec: None,
                looped: false,
                conversion: YuvConversion::BT709_LIMITED,
            },
            frame_rx: Some(frame_rx),
            next_frame: Some(QueuedFrame {
                pts_sec: 1.0,
                image: Yuv420Image::from_raw(2, 2, vec![3; 6]).unwrap(),
            }),
            recycle_tx,
            buffer_pool_misses: Arc::new(AtomicU64::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
            worker: None,
        };

        assert!(player.take_due_frame(0.5).is_none());
        assert!(player.take_due_frame(0.75).is_none());
        assert_eq!(player.next_frame.as_ref().unwrap().pts_sec, 1.0);
        assert_eq!(player.next_frame.as_ref().unwrap().image.as_raw(), &[3; 6]);
    }

    #[test]
    fn frame_pool_reuses_recycled_buffer() {
        let (recycle_tx, recycle_rx) = sync_channel(1);
        recycle_tx.try_send(vec![7, 7, 7, 7]).unwrap();

        let mut buffers = FrameBuffers {
            recycled: recycle_rx,
            frame_bytes: 4,
            unallocated: 1,
            misses: Arc::new(AtomicU64::new(0)),
        };
        let raw = buffers.take();

        assert_eq!(raw, vec![7, 7, 7, 7]);
        assert_eq!(buffers.misses.load(Ordering::Relaxed), 0);
        assert_eq!(buffers.unallocated, 1);
        assert!(buffers.recycled.try_recv().is_err());
    }

    #[test]
    fn frame_pool_allocates_on_demand_and_counts_exhaustion() {
        let (_recycle_tx, recycle_rx) = sync_channel(1);
        let mut buffers = FrameBuffers {
            recycled: recycle_rx,
            frame_bytes: 4,
            unallocated: 1,
            misses: Arc::new(AtomicU64::new(0)),
        };
        assert_eq!(buffers.take(), vec![0; 4]);
        assert_eq!(buffers.misses.load(Ordering::Relaxed), 0);
        assert_eq!(buffers.take(), vec![0; 4]);
        assert_eq!(buffers.misses.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn looped_decode_restarts_ffmpeg_while_once_holds_the_final_time() {
        let info = |looped| Info {
            width: 2,
            height: 2,
            fps: 30.0,
            duration_sec: Some(2.0),
            looped,
            conversion: YuvConversion::BT709_LIMITED,
        };
        let args = |looped| {
            decode_command(Path::new("banner.mp4"), info(looped))
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };

        assert!(!args(false).iter().any(|arg| arg == "-stream_loop"));
        assert!(
            args(true)
                .windows(2)
                .any(|args| args == ["-stream_loop", "-1"])
        );
        assert_eq!(clamp_play_time(3.0, info(false)), 2.0);
        assert_eq!(clamp_play_time(3.0, info(true)), 3.0);
    }

    #[test]
    fn raw_mpeg_one_shot_plays_past_the_bitrate_estimate() {
        use std::time::{Duration, Instant};

        // Eight pictures at 25 fps. Its variable-bitrate sequence header would
        // make a duration derived from file size stop in the first frame.
        let video = include_bytes!("../../deadlib-video-mpeg2/tests/fixtures/progressive.m1v");
        let dir = TempDir::new("raw-mpeg-clock");
        let path = dir.path().join("variable-rate.mpg");
        fs::write(&path, video).unwrap();
        let native = super::native::Video::open(&path, false).unwrap().unwrap();
        assert_eq!(native.info.duration_sec, Some(8.0 / 25.0));
        let mut player = super::open_native_player(native, &path, None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        for index in 0..8 {
            while player.take_due_frame(index as f32 / 25.0 + 0.005).is_none() {
                assert!(
                    Instant::now() < deadline,
                    "missing native MPEG frame {index}"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn decode_outputs_planar_yuv420_without_matrix_conversion() {
        let info = Info {
            width: 640,
            height: 480,
            fps: 30.0,
            duration_sec: None,
            looped: false,
            conversion: YuvConversion::BT709_LIMITED,
        };
        let args = decode_command(Path::new("banner.mp4"), info)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg.contains("format=yuv420p")));
        assert!(!args.iter().any(|arg| arg.contains("out_color_matrix")));
        assert!(args.windows(2).any(|args| args == ["-pix_fmt", "yuv420p"]));
    }

    #[test]
    fn probe_conversion_uses_metadata_then_bt601_fallback() {
        let stream = |space: Option<&'static str>, range: Option<&'static str>| ProbeStream {
            width: None,
            height: None,
            avg_frame_rate: None,
            r_frame_rate: None,
            duration: None,
            color_space: space,
            color_range: range,
        };
        let bt601 = probe_conversion(&stream(Some("smpte170m"), Some("tv")), 1920, 1080);
        assert_eq!(bt601.coeffs[0], 1.402);
        let fallback = probe_conversion(&stream(None, None), 1920, 1080);
        assert_eq!(fallback.coeffs[0], 1.402);
        let full = probe_conversion(&stream(Some("bt709"), Some("pc")), 640, 480);
        assert_eq!(full.levels, [1.0, 0.0, 1.0, -128.0 / 255.0]);
    }

    #[test]
    fn probe_metadata_borrows_json_strings() {
        let json = br#"{
            "streams": [{
                "width": 1920,
                "height": 1080,
                "avg_frame_rate": "60000/1001",
                "r_frame_rate": "60/1",
                "duration": "12.5",
                "color_space": "bt709",
                "color_range": "tv"
            }],
            "format": { "duration": "12.6" }
        }"#;
        let parsed: ProbeOutput<'_> = serde_json::from_slice(json).unwrap();
        let rate = parsed.streams[0].avg_frame_rate.unwrap();

        assert_eq!(rate, "60000/1001");
        assert!(json.as_ptr_range().contains(&rate.as_ptr()));
    }
}
