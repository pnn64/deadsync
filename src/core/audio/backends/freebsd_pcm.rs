use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, note_output_clock_fallback, note_output_underrun, publish_output_timing,
    publish_output_timing_quality,
};
use crate::core::host_time::now_nanos;
use libc::{c_int, c_ulong};
use log::info;
use std::fs::{self, File, OpenOptions};
use std::mem::size_of;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

const FREEBSD_PCM_FALLBACK_BUFFER_FRAMES: u32 = 1024;

const IOCPARM_MASK: u64 = 0x1fff;
const IOC_VOID: u64 = 0x2000_0000;
const IOC_OUT: u64 = 0x4000_0000;
const IOC_IN: u64 = 0x8000_0000;

const AFMT_S16_LE: c_int = 0x0000_0010;
const AFMT_S16_BE: c_int = 0x0000_0020;

#[cfg(target_endian = "little")]
const AFMT_S16_NE: c_int = AFMT_S16_LE;
#[cfg(target_endian = "big")]
const AFMT_S16_NE: c_int = AFMT_S16_BE;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AudioBufInfo {
    fragments: c_int,
    fragstotal: c_int,
    fragsize: c_int,
    bytes: c_int,
}

#[inline(always)]
const fn ioc(dir: u64, group: u8, num: u8, len: usize) -> c_ulong {
    (dir | (((len as u64) & IOCPARM_MASK) << 16) | ((group as u64) << 8) | (num as u64)) as c_ulong
}

#[inline(always)]
const fn ior<T>(group: u8, num: u8) -> c_ulong {
    ioc(IOC_OUT, group, num, size_of::<T>())
}

#[inline(always)]
const fn iowr<T>(group: u8, num: u8) -> c_ulong {
    ioc(IOC_IN | IOC_OUT, group, num, size_of::<T>())
}

#[inline(always)]
const fn io(group: u8, num: u8) -> c_ulong {
    ioc(IOC_VOID, group, num, 0)
}

const SNDCTL_DSP_RESET: c_ulong = io(b'P', 0);
const SNDCTL_DSP_SPEED: c_ulong = iowr::<c_int>(b'P', 2);
const SNDCTL_DSP_SETFMT: c_ulong = iowr::<c_int>(b'P', 5);
const SNDCTL_DSP_CHANNELS: c_ulong = iowr::<c_int>(b'P', 6);
const SNDCTL_DSP_GETOSPACE: c_ulong = ior::<AudioBufInfo>(b'P', 12);
const SNDCTL_DSP_GETODELAY: c_ulong = ior::<c_int>(b'P', 23);
const SNDCTL_DSP_GETBLKSIZE: c_ulong = iowr::<c_int>(b'P', 4);

pub(crate) struct FreeBsdPcmDevice {
    pub path: String,
    pub name: String,
    pub is_default: bool,
}

pub(crate) fn enumerate_output_devices() -> Vec<FreeBsdPcmDevice> {
    let Ok(entries) = fs::read_dir("/dev") else {
        return Vec::new();
    };
    let mut numbered = Vec::new();
    let mut has_alias = false;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == "dsp" {
            has_alias = true;
            continue;
        }
        let Some(index) = parse_dsp_index(name) else {
            continue;
        };
        numbered.push((index, name.to_string()));
    }
    numbered.sort_unstable_by_key(|(index, _)| *index);
    if numbered.is_empty() {
        return if has_alias {
            vec![FreeBsdPcmDevice {
                path: "/dev/dsp".to_string(),
                name: "FreeBSD PCM (/dev/dsp)".to_string(),
                is_default: true,
            }]
        } else {
            Vec::new()
        };
    }
    let default_index = numbered.first().map(|(index, _)| *index).unwrap_or(0);
    numbered
        .into_iter()
        .map(|(index, name)| FreeBsdPcmDevice {
            path: format!("/dev/{name}"),
            name: format!("FreeBSD PCM {index} (/dev/{name})"),
            is_default: index == default_index,
        })
        .collect()
}

#[inline(always)]
fn parse_dsp_index(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("dsp")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub(crate) struct FreeBsdPcmOutputPrep {
    dsp_path: String,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
}

impl FreeBsdPcmOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "freebsd-pcm",
            requested_output_mode: crate::config::AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Monotonic,
            timing_quality: OutputTimingQuality::Degraded,
        }
    }
}

pub(crate) struct FreeBsdPcmOutputStream {
    stop_flag: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for FreeBsdPcmOutputStream {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn prepare(
    dsp_path: Option<String>,
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<FreeBsdPcmOutputPrep, String> {
    let dsp_path = dsp_path.unwrap_or_else(|| "/dev/dsp".to_string());
    let file = open_dsp(&dsp_path)?;
    let actual = configure_probe(file.as_raw_fd(), sample_rate_hz, channels.max(1))?;
    Ok(FreeBsdPcmOutputPrep {
        dsp_path,
        device_name,
        sample_rate_hz: actual.sample_rate_hz,
        channels: actual.channels,
        period_frames: actual.period_frames,
        buffer_frames: actual.buffer_frames,
    })
}

pub(crate) fn start(
    prep: FreeBsdPcmOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<FreeBsdPcmOutputStream, String> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_thread = stop_flag.clone();
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let thread = thread::Builder::new()
        .name("freebsd_pcm".to_string())
        .spawn(move || render_thread(prep, music_ring, sfx_receiver, stop_flag_thread, ready_tx))
        .map_err(|e| format!("failed to spawn FreeBSD PCM render thread: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(FreeBsdPcmOutputStream {
            stop_flag,
            thread: Some(thread),
        }),
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            Err("FreeBSD PCM render thread exited during startup".to_string())
        }
    }
}

struct FreeBsdPcmParams {
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
    bytes_per_frame: u32,
}

fn render_thread(
    prep: FreeBsdPcmOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: Arc<AtomicBool>,
    ready_tx: Sender<Result<(), String>>,
) {
    if let Err(err) = render_thread_inner(prep, music_ring, sfx_receiver, &stop_flag, &ready_tx) {
        let _ = ready_tx.send(Err(err));
    }
}

fn render_thread_inner(
    prep: FreeBsdPcmOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_flag: &AtomicBool,
    ready_tx: &Sender<Result<(), String>>,
) -> Result<(), String> {
    let file = open_dsp(&prep.dsp_path)?;
    let actual = configure_run(
        file.as_raw_fd(),
        prep.sample_rate_hz,
        prep.channels,
        prep.period_frames,
        prep.buffer_frames,
    )?;
    let period_ns = frames_to_nanos(actual.sample_rate_hz, actual.period_frames);
    let buffer_ns = frames_to_nanos(actual.sample_rate_hz, actual.buffer_frames);
    info!(
        "FreeBSD PCM '{}' using native OSS-compatible output on '{}'.",
        prep.device_name, prep.dsp_path
    );
    publish_output_timing_quality(OutputTimingQuality::Degraded);
    publish_output_timing(
        actual.sample_rate_hz,
        period_ns,
        buffer_ns,
        actual.buffer_frames,
        actual.period_frames,
        actual.period_frames,
        period_ns,
    );
    if ready_tx.send(Ok(())).is_err() {
        return Ok(());
    }

    let mut render = RenderState::new(music_ring, sfx_receiver, actual.channels);
    let mut mix = vec![0i16; actual.period_frames as usize * actual.channels];
    while !stop_flag.load(Ordering::Relaxed) {
        let timing_before = playback_timing(
            file.as_raw_fd(),
            actual.sample_rate_hz,
            actual.bytes_per_frame,
            actual.buffer_frames,
        );
        render.render_i16_host_nanos(&mut mix, timing_before.playback_host_nanos);
        write_all(file.as_raw_fd(), &mix, stop_flag, &prep.device_name)?;
        let timing_after = playback_timing(
            file.as_raw_fd(),
            actual.sample_rate_hz,
            actual.bytes_per_frame,
            actual.buffer_frames,
        );
        publish_output_timing_quality(worst_quality(
            timing_before.timing_quality,
            timing_after.timing_quality,
        ));
        publish_output_timing(
            actual.sample_rate_hz,
            period_ns,
            buffer_ns,
            timing_after.buffer_frames.max(actual.buffer_frames),
            timing_after.queued_frames,
            timing_after.queued_frames,
            timing_after.estimated_output_delay_ns,
        );
    }
    let _ = reset_dsp(file.as_raw_fd());
    Ok(())
}

fn configure_probe(
    fd: RawFd,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<FreeBsdPcmParams, String> {
    configure_device(
        fd,
        sample_rate_hz,
        channels,
        suggested_period_frames(sample_rate_hz),
        None,
    )
}

fn configure_run(
    fd: RawFd,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    buffer_frames: u32,
) -> Result<FreeBsdPcmParams, String> {
    configure_device(
        fd,
        sample_rate_hz,
        channels,
        period_frames,
        Some(buffer_frames),
    )
}

fn configure_device(
    fd: RawFd,
    sample_rate_hz: u32,
    channels: usize,
    period_frames: u32,
    _buffer_frames: Option<u32>,
) -> Result<FreeBsdPcmParams, String> {
    let mut format = AFMT_S16_NE;
    ioctl_mut(fd, SNDCTL_DSP_SETFMT, &mut format)
        .map_err(|e| format!("FreeBSD PCM format setup failed: {e}"))?;
    if format != AFMT_S16_NE {
        return Err("FreeBSD PCM device rejected native S16 format.".to_string());
    }

    let mut actual_channels = channels.max(1) as c_int;
    ioctl_mut(fd, SNDCTL_DSP_CHANNELS, &mut actual_channels)
        .map_err(|e| format!("FreeBSD PCM channel setup failed: {e}"))?;
    if actual_channels <= 0 {
        return Err("FreeBSD PCM returned an invalid channel count.".to_string());
    }

    let mut actual_rate = sample_rate_hz.max(1) as c_int;
    ioctl_mut(fd, SNDCTL_DSP_SPEED, &mut actual_rate)
        .map_err(|e| format!("FreeBSD PCM sample-rate setup failed: {e}"))?;
    if actual_rate <= 0 {
        return Err("FreeBSD PCM returned an invalid sample rate.".to_string());
    }

    let bytes_per_frame = (actual_channels as u32).saturating_mul(size_of::<i16>() as u32);
    let mut block_bytes = 0i32;
    let period_frames =
        if ioctl_mut(fd, SNDCTL_DSP_GETBLKSIZE, &mut block_bytes).is_ok() && block_bytes > 0 {
            bytes_to_frames(block_bytes as u32, bytes_per_frame).max(1)
        } else {
            period_frames.max(1)
        };
    let buffer_frames = query_ospace(fd, bytes_per_frame)
        .map(|(buffer, _)| buffer.max(period_frames))
        .unwrap_or_else(|| {
            period_frames
                .saturating_mul(4)
                .max(FREEBSD_PCM_FALLBACK_BUFFER_FRAMES)
        });

    Ok(FreeBsdPcmParams {
        sample_rate_hz: actual_rate as u32,
        channels: actual_channels as usize,
        period_frames,
        buffer_frames,
        bytes_per_frame,
    })
}

fn write_all(
    fd: RawFd,
    mix: &[i16],
    stop_flag: &AtomicBool,
    device_name: &str,
) -> Result<(), String> {
    let bytes = unsafe {
        std::slice::from_raw_parts(mix.as_ptr().cast::<u8>(), std::mem::size_of_val(mix))
    };
    let mut written = 0usize;
    while written < bytes.len() {
        if stop_flag.load(Ordering::Relaxed) {
            return Ok(());
        }
        let rc = unsafe {
            libc::write(
                fd,
                bytes[written..].as_ptr().cast(),
                bytes.len().saturating_sub(written),
            )
        };
        if rc > 0 {
            written = written.saturating_add(rc as usize);
            continue;
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINTR) | Some(libc::EAGAIN) => {
                thread::yield_now();
            }
            _ => {
                note_output_underrun();
                return Err(format!(
                    "FreeBSD PCM write failed for '{device_name}': {err}"
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PlaybackTiming {
    playback_host_nanos: u64,
    buffer_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
    timing_quality: OutputTimingQuality,
}

#[inline(always)]
fn playback_timing(
    fd: RawFd,
    sample_rate_hz: u32,
    bytes_per_frame: u32,
    buffer_frames_fallback: u32,
) -> PlaybackTiming {
    let delay_frames = query_odelay(fd, bytes_per_frame);
    let (buffer_frames, queued_frames_from_space) =
        query_ospace(fd, bytes_per_frame).unwrap_or((buffer_frames_fallback, 0));
    let queued_frames = delay_frames.unwrap_or(queued_frames_from_space);
    let delay_ns = frames_to_nanos(sample_rate_hz, queued_frames);
    let timing_quality = if delay_frames.is_some() || queued_frames_from_space != 0 {
        OutputTimingQuality::Degraded
    } else {
        note_output_clock_fallback();
        OutputTimingQuality::Fallback
    };
    PlaybackTiming {
        playback_host_nanos: now_nanos().saturating_add(delay_ns),
        buffer_frames: buffer_frames.max(buffer_frames_fallback),
        queued_frames,
        estimated_output_delay_ns: delay_ns,
        timing_quality,
    }
}

#[inline(always)]
fn query_ospace(fd: RawFd, bytes_per_frame: u32) -> Option<(u32, u32)> {
    let mut info = AudioBufInfo::default();
    ioctl_mut(fd, SNDCTL_DSP_GETOSPACE, &mut info).ok()?;
    if info.fragstotal <= 0 || info.fragsize <= 0 || info.bytes < 0 {
        return None;
    }
    let buffer_bytes = (info.fragstotal as u32).saturating_mul(info.fragsize as u32);
    let free_bytes = info.bytes as u32;
    let buffer_frames = bytes_to_frames(buffer_bytes, bytes_per_frame);
    let free_frames = bytes_to_frames(free_bytes.min(buffer_bytes), bytes_per_frame);
    Some((buffer_frames, buffer_frames.saturating_sub(free_frames)))
}

#[inline(always)]
fn query_odelay(fd: RawFd, bytes_per_frame: u32) -> Option<u32> {
    let mut delay_bytes = 0i32;
    ioctl_mut(fd, SNDCTL_DSP_GETODELAY, &mut delay_bytes).ok()?;
    if delay_bytes < 0 {
        return None;
    }
    Some(bytes_to_frames(delay_bytes as u32, bytes_per_frame))
}

#[inline(always)]
fn open_dsp(path: &str) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("failed to open FreeBSD PCM device '{path}': {e}"))
}

#[inline(always)]
fn reset_dsp(fd: RawFd) -> Result<(), String> {
    ioctl_none(fd, SNDCTL_DSP_RESET).map_err(|e| format!("FreeBSD PCM reset failed: {e}"))
}

#[inline(always)]
fn ioctl_none(fd: RawFd, req: c_ulong) -> Result<(), std::io::Error> {
    let rc = unsafe { libc::ioctl(fd, req) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[inline(always)]
fn ioctl_mut<T>(fd: RawFd, req: c_ulong, value: &mut T) -> Result<(), std::io::Error> {
    let rc = unsafe { libc::ioctl(fd, req, value) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[inline(always)]
fn bytes_to_frames(bytes: u32, bytes_per_frame: u32) -> u32 {
    if bytes_per_frame == 0 {
        return 0;
    }
    bytes / bytes_per_frame
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 || frames == 0 {
        return 0;
    }
    ((u128::from(frames) * 1_000_000_000u128) / u128::from(sample_rate_hz))
        .min((u64::MAX - 1) as u128) as u64
}

#[inline(always)]
const fn worst_quality(a: OutputTimingQuality, b: OutputTimingQuality) -> OutputTimingQuality {
    if (a as u8) >= (b as u8) { a } else { b }
}

#[inline(always)]
fn suggested_period_frames(sample_rate_hz: u32) -> u32 {
    let frames = sample_rate_hz.max(1) / 200;
    frames.clamp(128, FREEBSD_PCM_FALLBACK_BUFFER_FRAMES)
}
