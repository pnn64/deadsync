use crate::{MusicDecodeContext, MusicStream, OutputFormat};
use deadsync_audio::{Cut, MusicBlockWriter, activate_music_track, stop_music_track};
use log::error;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub enum StreamCommand {
    PlayMusic {
        path: PathBuf,
        cut: Cut,
        looping: bool,
        rate: f32,
        preserve_pitch: bool,
        generation: u64,
    },
    StopMusic {
        generation: u64,
    },
    SetMusicRate {
        rate: f32,
        generation: u64,
    },
    SetPreservePitch {
        enabled: bool,
        generation: u64,
    },
}

pub struct MusicStreamRuntime {
    music_stream: Option<MusicStream>,
    writer: Option<MusicBlockWriter>,
    output: OutputFormat,
}

impl MusicStreamRuntime {
    pub fn new(writer: MusicBlockWriter, output: OutputFormat) -> Self {
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
            StreamCommand::StopMusic { generation } => self.stop(generation),
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
        self.music_stream = Some(crate::spawn_music_decoder_thread(
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

    fn stop(&mut self, _generation: u64) {
        self.stop_decoder();
        stop_music_track();
    }

    fn set_rate(&mut self, rate: f32, generation: u64) {
        if let Some(stream) = &self.music_stream {
            let control = &stream.control;
            control.rate_bits.store(rate.to_bits(), Ordering::Release);
            control.generation.store(generation, Ordering::Release);
        }
    }

    fn set_preserve_pitch(&mut self, enabled: bool, generation: u64) {
        if let Some(stream) = &self.music_stream {
            let control = &stream.control;
            control.preserve_pitch.store(enabled, Ordering::Release);
            control.generation.store(generation, Ordering::Release);
        }
    }

    fn stop_decoder(&mut self) {
        if let Some(old) = self.music_stream.take() {
            old.control.stop_signal.store(true, Ordering::Relaxed);
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
