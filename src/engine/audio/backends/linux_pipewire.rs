use super::super::{
    OutputBackendReady, OutputTelemetryClock, OutputTimingQuality, QueuedSfx, RenderState,
    internal, publish_output_timing, publish_output_timing_quality,
};
use crate::engine::host_time::now_nanos;
use log::{info, warn};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;
use std::io::Cursor;
use std::mem;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};

pub(crate) struct PipeWireOutputPrep {
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
}

impl PipeWireOutputPrep {
    pub(crate) fn ready(&self) -> OutputBackendReady {
        OutputBackendReady {
            device_sample_rate: self.sample_rate_hz,
            device_channels: self.channels,
            device_name: self.device_name.clone(),
            backend_name: "pipewire-shared",
            requested_output_mode: crate::config::AudioOutputMode::Shared,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Monotonic,
            timing_quality: OutputTimingQuality::Trusted,
        }
    }
}

pub(crate) struct PipeWireOutputStream {
    stop_sender: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for PipeWireOutputStream {
    fn drop(&mut self) {
        let _ = self.stop_sender.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct CallbackState {
    render: RenderState,
    format: spa::param::audio::AudioInfoRaw,
    fallback_rate_hz: u32,
    fallback_channels: usize,
    interleaved: Vec<f32>,
}

impl CallbackState {
    fn new(
        music_ring: Arc<internal::SpscRingI16>,
        sfx_receiver: Receiver<QueuedSfx>,
        sample_rate_hz: u32,
        channels: usize,
    ) -> Self {
        Self {
            render: RenderState::new(music_ring, sfx_receiver, channels),
            format: spa::param::audio::AudioInfoRaw::new(),
            fallback_rate_hz: sample_rate_hz.max(1),
            fallback_channels: channels.max(1),
            interleaved: Vec::new(),
        }
    }

    #[inline(always)]
    fn sample_rate_hz(&self) -> u32 {
        self.format.rate().max(self.fallback_rate_hz)
    }

    #[inline(always)]
    fn channels(&self) -> usize {
        (self.format.channels() as usize).max(self.fallback_channels)
    }

    fn render_into(&mut self, data: &mut [u8]) -> usize {
        let channels = self.channels();
        let stride = channels.saturating_mul(mem::size_of::<f32>());
        if stride == 0 {
            return 0;
        }
        let frames = data.len() / stride;
        let samples = frames.saturating_mul(channels);
        if self.interleaved.len() != samples {
            self.interleaved.resize(samples, 0.0);
        }
        let anchor_nanos = now_nanos();
        self.render
            .render_f32_host_nanos(&mut self.interleaved, anchor_nanos);
        for (src, chunk) in self.interleaved[..samples]
            .iter()
            .zip(data[..samples * mem::size_of::<f32>()].chunks_exact_mut(mem::size_of::<f32>()))
        {
            chunk.copy_from_slice(&src.to_le_bytes());
        }
        let period_ns = frames_to_nanos(self.sample_rate_hz(), frames as u32);
        publish_output_timing(
            self.sample_rate_hz(),
            period_ns,
            period_ns,
            frames as u32,
            0,
            frames as u32,
            period_ns,
        );
        publish_output_timing_quality(OutputTimingQuality::Trusted);
        samples * mem::size_of::<f32>()
    }
}

pub(crate) fn prepare(
    requested_device_name: Option<String>,
    sample_rate_hz: u32,
    channels: usize,
) -> Result<PipeWireOutputPrep, String> {
    let device_name = match requested_device_name {
        Some(name) if !name.is_empty() => {
            format!("PipeWire default sink (requested '{name}' unsupported)")
        }
        _ => "PipeWire default sink".to_string(),
    };
    Ok(PipeWireOutputPrep {
        device_name,
        sample_rate_hz: sample_rate_hz.max(1),
        channels: channels.clamp(1, 32),
    })
}

pub(crate) fn start(
    prep: PipeWireOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
) -> Result<PipeWireOutputStream, String> {
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let (stop_sender, stop_receiver) = pw::channel::channel::<()>();
    let thread = thread::Builder::new()
        .name("pipewire_out".to_string())
        .spawn(move || {
            let _ = render_thread(prep, music_ring, sfx_receiver, stop_receiver, ready_tx);
        })
        .map_err(|e| format!("failed to spawn PipeWire render thread: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(PipeWireOutputStream {
            stop_sender,
            thread: Some(thread),
        }),
        Ok(Err(err)) => {
            let _ = stop_sender.send(());
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = stop_sender.send(());
            let _ = thread.join();
            Err("PipeWire render thread exited during startup".to_string())
        }
    }
}

fn render_thread(
    prep: PipeWireOutputPrep,
    music_ring: Arc<internal::SpscRingI16>,
    sfx_receiver: Receiver<QueuedSfx>,
    stop_receiver: pw::channel::Receiver<()>,
    ready_tx: Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| format!("failed to create PipeWire main loop: {e}"))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| format!("failed to create PipeWire context: {e}"))?;
    let core = context
        .connect_rc(None)
        .map_err(|e| format!("failed to connect to PipeWire core: {e}"))?;
    let state = CallbackState::new(music_ring, sfx_receiver, prep.sample_rate_hz, prep.channels);

    let channels_prop = prep.channels.to_string();
    let stream = pw::stream::StreamBox::new(
        &core,
        "deadsync-audio",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::AUDIO_CHANNELS => channels_prop.as_str(),
        },
    )
    .map_err(|e| format!("failed to create PipeWire stream: {e}"))?;

    let _stop = stop_receiver.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(|_, _, old, new| {
            if let pw::stream::StreamState::Error(err) = &new {
                warn!("PipeWire stream state error after {old:?}: {err}");
            }
        })
        .param_changed(|_, state, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            if let Err(err) = state.format.parse(param) {
                warn!("PipeWire failed to parse negotiated audio format: {err}");
            }
        })
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let Some(slice) = data.data() else {
                return;
            };
            let written = state.render_into(slice);
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = (state.channels() * mem::size_of::<f32>()) as i32;
            *chunk.size_mut() = written as u32;
        })
        .register()
        .map_err(|e| format!("failed to register PipeWire stream listener: {e}"))?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(prep.sample_rate_hz);
    audio_info.set_channels(prep.channels as u32);
    let mut position = [0u32; spa::param::audio::MAX_CHANNELS];
    if prep.channels >= 2 {
        position[0] = pw::spa::sys::SPA_AUDIO_CHANNEL_FL;
        position[1] = pw::spa::sys::SPA_AUDIO_CHANNEL_FR;
    }
    audio_info.set_position(position);

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
            id: pw::spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map_err(|e| format!("failed to serialize PipeWire audio format: {e}"))?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values)
        .ok_or_else(|| "failed to build PipeWire format pod".to_string())?;
    let mut params = [pod];
    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| format!("failed to connect PipeWire stream: {e}"))?;

    info!(
        "PipeWire '{}' using {} Hz, {} ch shared output.",
        prep.device_name, prep.sample_rate_hz, prep.channels
    );
    publish_output_timing(prep.sample_rate_hz, 0, 0, 0, 0, 0, 0);
    publish_output_timing_quality(OutputTimingQuality::Trusted);
    if ready_tx.send(Ok(())).is_err() {
        return Ok(());
    }
    mainloop.run();
    Ok(())
}

#[inline(always)]
fn frames_to_nanos(sample_rate_hz: u32, frames: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    (u64::from(frames) * 1_000_000_000) / u64::from(sample_rate_hz.max(1))
}
