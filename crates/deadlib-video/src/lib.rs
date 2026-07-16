use image::RgbaImage;
use log::warn;
use serde::Deserialize;
use std::{
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
}

pub struct LoadedVideo {
    pub info: Info,
    pub poster: RgbaImage,
    pub player: Player,
}

struct QueuedFrame {
    pts_sec: f32,
    image: RgbaImage,
}

pub struct VideoFrame {
    image: Option<RgbaImage>,
    recycle_tx: Option<SyncSender<Vec<u8>>>,
}

impl VideoFrame {
    pub fn into_upload_parts(mut self) -> (RgbaImage, SyncSender<Vec<u8>>) {
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
    recycle_rx: Receiver<Vec<u8>>,
    buffer_pool_misses: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

/// Asynchronous decoder with a bounded frame queue and reusable pixel pool.
///
/// The worker owns decode writes; the game thread uses a nonblocking receive and
/// the upload queue returns displayed buffers. Both channels live for one
/// `Player`. Startup preallocates one buffer per queued frame, one in-flight
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
    pub fn take_due_frame(&mut self, play_time_sec: f32) -> Option<VideoFrame> {
        let target = clamp_play_time(play_time_sec, self.info);
        let mut latest = None;
        loop {
            let frame = match self.next_frame.take() {
                Some(frame) => frame,
                None => match self.frame_rx.as_ref()?.try_recv() {
                    Ok(frame) => frame,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                },
            };
            if frame.pts_sec > target {
                self.next_frame = Some(frame);
                break;
            }
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
    let info = probe(path, looped)?;
    let poster = load_poster_with_info(path, info)?;
    let player = open_player_with_info(path, info)?;
    Ok(LoadedVideo {
        info,
        poster,
        player,
    })
}

pub fn load_poster(path: &Path) -> Result<RgbaImage, String> {
    load_poster_with_info(path, probe(path, false)?)
}

pub fn open_player(path: &Path, looped: bool) -> Result<Player, String> {
    open_player_with_info(path, probe(path, looped)?)
}

fn open_player_with_info(path: &Path, info: Info) -> Result<Player, String> {
    let max_frames = queue_capacity(info);
    let (frame_tx, frame_rx) = sync_channel(max_frames);
    // Besides the bounded queue, one buffer can be in the upload queue and one
    // can be held by the worker while it blocks on a full decoded-frame queue.
    let buffer_count = max_frames.saturating_add(2);
    let (recycle_tx, recycle_rx) = sync_channel(buffer_count);
    let frame_bytes = rgba_frame_bytes(info);
    for _ in 0..buffer_count {
        recycle_tx
            .try_send(vec![0; frame_bytes])
            .expect("new video recycle channel must have room");
    }
    let stop = Arc::new(AtomicBool::new(false));
    let buffer_pool_misses = Arc::new(AtomicU64::new(0));
    let child = Arc::new(Mutex::new(None));
    let worker = spawn_worker(
        path.to_path_buf(),
        info,
        DecoderWorker {
            frame_tx,
            recycle_tx: recycle_tx.clone(),
            recycle_rx,
            buffer_pool_misses: buffer_pool_misses.clone(),
            stop: stop.clone(),
            child: child.clone(),
        },
    )?;
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

fn spawn_worker(
    path: PathBuf,
    info: Info,
    worker: DecoderWorker,
) -> Result<JoinHandle<()>, String> {
    let mut child = decode_command(&path, info).spawn().map_err(|e| {
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
    Ok(thread::spawn(move || decode_loop(stdout, info, worker)))
}

fn decode_loop(mut stdout: ChildStdout, info: Info, worker: DecoderWorker) {
    let DecoderWorker {
        frame_tx,
        recycle_tx,
        recycle_rx,
        buffer_pool_misses,
        stop,
        child: child_slot,
    } = worker;
    let frame_bytes = rgba_frame_bytes(info);
    let frame_step = 1.0 / info.fps.max(1.0);
    let mut frame_index = 0u64;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut raw = take_frame_buffer(&recycle_rx, frame_bytes, &buffer_pool_misses);
        match read_frame(&mut stdout, &mut raw) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                warn!("Video decode stream read failed: {e}");
                break;
            }
        }
        let Some(image) = RgbaImage::from_raw(info.width, info.height, raw) else {
            warn!("Video decoder produced an invalid RGBA frame");
            break;
        };
        let pts_sec = frame_index as f32 * frame_step;
        frame_index = frame_index.saturating_add(1);
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

fn take_frame_buffer(
    recycle_rx: &Receiver<Vec<u8>>,
    frame_bytes: usize,
    buffer_pool_misses: &AtomicU64,
) -> Vec<u8> {
    let mut raw = match recycle_rx.try_recv() {
        Ok(raw) => raw,
        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
            buffer_pool_misses.fetch_add(1, Ordering::Relaxed);
            return vec![0; frame_bytes];
        }
    };
    if raw.len() == frame_bytes {
        raw
    } else {
        raw.resize(frame_bytes, 0);
        raw
    }
}

fn recycle_frame_buffer(recycle_tx: &SyncSender<Vec<u8>>, image: RgbaImage) {
    let _ = recycle_tx.try_send(image.into_raw());
}

fn load_poster_with_info(path: &Path, info: Info) -> Result<RgbaImage, String> {
    let frame_bytes = rgba_frame_bytes(info);
    let mut child = poster_command(path).spawn().map_err(|e| {
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
        .arg("stream=width,height,avg_frame_rate,r_frame_rate,duration:format=duration")
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
    let fps = parse_rate(stream.avg_frame_rate.as_deref())
        .or_else(|| parse_rate(stream.r_frame_rate.as_deref()))
        .unwrap_or(DEFAULT_FPS)
        .clamp(1.0, MAX_FPS);
    let duration_sec = parse_duration(stream.duration.as_deref()).or_else(|| {
        parse_duration(
            parsed
                .format
                .as_ref()
                .and_then(|fmt| fmt.duration.as_deref()),
        )
    });

    Ok(Info {
        width,
        height,
        fps,
        duration_sec,
        looped,
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
        .arg(format!("fps={:.6},format=rgba", info.fps))
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

fn poster_command(path: &Path) -> Command {
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
        .arg("format=rgba")
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
pub fn runtime_bin_dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|dir| dir.join("bin"))
}

/// True when both `ffmpeg` and `ffprobe` resolve, i.e. video playback
/// will work.
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
    bundled_tool_candidates(name)
        .into_iter()
        .map(|candidate| dir.join(candidate))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn bundled_tool_candidates(name: &str) -> [String; 2] {
    [format!("{name}.exe"), name.to_string()]
}

#[cfg(not(windows))]
fn bundled_tool_candidates(name: &str) -> [String; 2] {
    [name.to_string(), format!("{name}.exe")]
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
fn queue_capacity(info: Info) -> usize {
    let frame_bytes = rgba_frame_bytes(info).max(1);
    (FRAME_QUEUE_BYTES / frame_bytes).clamp(2, 24)
}

fn parse_rate(raw: Option<&str>) -> Option<f32> {
    let raw = raw?;
    if raw == "0/0" {
        return None;
    }
    let (num, den) = raw.split_once('/')?;
    let num = num.parse::<f64>().ok()?;
    let den = den.parse::<f64>().ok()?;
    if !num.is_finite() || !den.is_finite() || den <= 0.0 {
        return None;
    }
    let fps = (num / den) as f32;
    (fps.is_finite() && fps > 0.0).then_some(fps)
}

fn parse_duration(raw: Option<&str>) -> Option<f32> {
    let duration = raw?.parse::<f32>().ok()?;
    (duration.is_finite() && duration > 0.0).then_some(duration)
}

#[derive(Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        Info, Player, QueuedFrame, bundled_tool_candidates, clamp_play_time, decode_command,
        parse_duration, parse_rate, resolve_tool_path_in_dirs, take_frame_buffer,
    };
    use image::RgbaImage;
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
        let path = dir.join(&bundled_tool_candidates(name)[0]);
        fs::write(&path, []).unwrap();
        path
    }

    #[test]
    fn parse_rate_handles_fraction() {
        let fps = parse_rate(Some("30000/1001")).unwrap();
        assert!((fps - 29.97003).abs() < 0.001);
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
    fn take_due_frame_recycles_skipped_frames() {
        let (recycle_tx, recycle_rx) = sync_channel(2);
        let (frame_tx, frame_rx) = sync_channel(2);
        assert!(
            frame_tx
                .try_send(QueuedFrame {
                    pts_sec: 0.0,
                    image: RgbaImage::from_raw(1, 1, vec![1, 1, 1, 1]).unwrap(),
                })
                .is_ok()
        );
        assert!(
            frame_tx
                .try_send(QueuedFrame {
                    pts_sec: 1.0,
                    image: RgbaImage::from_raw(1, 1, vec![2, 2, 2, 2]).unwrap(),
                })
                .is_ok()
        );
        let mut player = Player {
            info: Info {
                width: 1,
                height: 1,
                fps: 30.0,
                duration_sec: None,
                looped: false,
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
        assert_eq!(recycle_rx.try_recv().unwrap(), vec![1, 1, 1, 1]);
        drop(frame);
        assert_eq!(recycle_rx.try_recv().unwrap(), vec![2, 2, 2, 2]);
    }

    #[test]
    fn take_frame_buffer_reuses_recycled_buffer() {
        let (recycle_tx, recycle_rx) = sync_channel(1);
        recycle_tx.try_send(vec![7, 7, 7, 7]).unwrap();

        let misses = AtomicU64::new(0);
        let raw = take_frame_buffer(&recycle_rx, 4, &misses);

        assert_eq!(raw, vec![7, 7, 7, 7]);
        assert_eq!(misses.load(Ordering::Relaxed), 0);
        assert!(recycle_rx.try_recv().is_err());
    }

    #[test]
    fn take_frame_buffer_counts_pool_exhaustion() {
        let (_recycle_tx, recycle_rx) = sync_channel(1);
        let misses = AtomicU64::new(0);

        let raw = take_frame_buffer(&recycle_rx, 4, &misses);

        assert_eq!(raw, vec![0; 4]);
        assert_eq!(misses.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn looped_decode_restarts_ffmpeg_while_once_holds_the_final_time() {
        let info = |looped| Info {
            width: 1,
            height: 1,
            fps: 30.0,
            duration_sec: Some(2.0),
            looped,
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
}
