use super::Cut;
use super::{MusicDecodeContext, MusicStream, OutputFormat};
use crate::OutputPlan;
use deadlib_audio_core::{MusicBlockWriter, activate_music_track, stop_music_track};
use deadlib_audio_core::{OutputBackendReady, PlayedMapReader, SfxSender};
use log::{error, info};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

/// Commands for one resolved stream; playback policy belongs to the caller.
#[derive(Debug)]
pub enum StreamCommand {
    PlayMusic {
        path: PathBuf,
        cut: Cut,
        looping: bool,
        rate: f32,
        preserve_pitch: bool,
        generation: u64,
    },
    /// Stops the worker after the caller invalidates the shared timeline generation.
    StopMusic,
    SetMusicRate {
        rate: f32,
        generation: u64,
    },
    SetPreservePitch {
        enabled: bool,
        generation: u64,
    },
}

/// Manager-thread owner of the decoder and its recyclable block writer.
pub struct MusicStreamRuntime {
    music_stream: Option<MusicStream>,
    writer: Option<MusicBlockWriter>,
    output: OutputFormat,
}

impl MusicStreamRuntime {
    pub const fn new(writer: MusicBlockWriter, output: OutputFormat) -> Self {
        Self {
            music_stream: None,
            writer: Some(writer),
            output,
        }
    }

    pub fn handle(&mut self, command: StreamCommand) {
        match command {
            StreamCommand::PlayMusic {
                path,
                cut,
                looping,
                rate,
                preserve_pitch,
                generation,
            } => self.play(path, cut, looping, rate, preserve_pitch, generation),
            StreamCommand::StopMusic => self.stop(),
            StreamCommand::SetMusicRate { rate, generation } => self.set_rate(rate, generation),
            StreamCommand::SetPreservePitch {
                enabled,
                generation,
            } => self.set_preserve_pitch(enabled, generation),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn play(
        &mut self,
        path: PathBuf,
        cut: Cut,
        looping: bool,
        rate: f32,
        preserve_pitch: bool,
        generation: u64,
    ) {
        self.stop_decoder();
        activate_music_track();

        let Some(writer) = self.writer.take() else {
            error!("Music decoder writer was lost after a worker panic.");
            stop_music_track();
            return;
        };
        self.music_stream = Some(super::spawn_music_decoder_thread(
            path,
            cut,
            looping,
            rate,
            preserve_pitch,
            writer,
            MusicDecodeContext {
                output: self.output,
                generation,
            },
        ));
    }

    fn stop(&mut self) {
        self.stop_decoder();
        stop_music_track();
    }

    fn set_rate(&self, rate: f32, generation: u64) {
        if let Some(stream) = &self.music_stream {
            let control = &stream.control;
            control.rate_bits.store(rate.to_bits(), Ordering::Release);
            control.generation.store(generation, Ordering::Release);
            control.wake.notify();
        }
    }

    fn set_preserve_pitch(&self, enabled: bool, generation: u64) {
        if let Some(stream) = &self.music_stream {
            let control = &stream.control;
            control.preserve_pitch.store(enabled, Ordering::Release);
            control.generation.store(generation, Ordering::Release);
            control.wake.notify();
        }
    }

    fn stop_decoder(&mut self) {
        if let Some(old) = self.music_stream.take() {
            old.control.stop_signal.store(true, Ordering::Release);
            old.control.wake.notify();
            match old.thread.join() {
                Ok(writer) => self.writer = Some(writer),
                Err(_) => error!("Music decoder thread panicked; its transport writer was lost."),
            }
        }
    }
}

impl Drop for MusicStreamRuntime {
    fn drop(&mut self) {
        self.stop_decoder();
    }
}

/// Output transports returned once the manager has opened the device.
///
/// The application keeps the command sender and sole SFX producer. The manager
/// owns the output session until all command senders have been dropped.
pub struct StreamReady {
    pub command_sender: Sender<StreamCommand>,
    pub backend_ready: OutputBackendReady,
    pub sfx_sender: SfxSender,
    pub played_map: PlayedMapReader,
}

/// Opens output on the manager thread and starts its stream-command queue.
///
/// Returns an error if device startup fails or the manager exits before opening.
pub fn start(output_plan: OutputPlan) -> Result<StreamReady, String> {
    let (command_sender, command_receiver) = channel();
    let (ready_sender, ready_receiver) = channel();
    thread::spawn(move || {
        audio_manager_thread(command_sender, command_receiver, ready_sender, output_plan);
    });
    let ready = ready_receiver
        .recv()
        .map_err(|_| "audio manager thread exited before reporting ready".to_string())??;
    let backend = &ready.backend_ready;
    info!(
        "Audio runtime initialized ({} Hz, {} ch, backend={} req={} fallback={} clock={} quality={} device='{}').",
        backend.device_sample_rate,
        backend.device_channels,
        backend.backend_name,
        backend.requested_output_mode.as_str(),
        backend.fallback_from_native,
        backend.timing_clock,
        backend.timing_quality,
        backend.device_name
    );
    deadlib_audio_core::publish_output_backend_ready(backend.clone());
    Ok(ready)
}

fn audio_manager_thread(
    command_sender: Sender<StreamCommand>,
    command_receiver: Receiver<StreamCommand>,
    ready_sender: Sender<Result<StreamReady, String>>,
    output_plan: OutputPlan,
) {
    let opened = match output_plan.open() {
        Ok(output) => output,
        Err(err) => {
            let _ = ready_sender.send(Err(err));
            return;
        }
    };
    let (_session, ready, sfx_sender, stream_handle) = opened.into_parts();
    let deadlib_audio_core::AudioStreamHandle { writer, played_map } = stream_handle;
    let stream_output = OutputFormat {
        sample_rate_hz: ready.device_sample_rate,
        channels: ready.device_channels,
    };
    if ready_sender
        .send(Ok(StreamReady {
            command_sender,
            backend_ready: ready,
            sfx_sender,
            played_map,
        }))
        .is_err()
    {
        drop(_session);
        drop(writer);
        return;
    }

    let mut music_runtime = MusicStreamRuntime::new(writer, stream_output);
    while let Ok(command) = command_receiver.recv() {
        music_runtime.handle(command);
    }
    // Stop and join the render callback while the recycle consumer still owns
    // the pool. Pooled blocks are then destroyed here on the manager thread.
    drop(_session);
    drop(music_runtime);
}
