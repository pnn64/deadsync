use deadsync_theme_simply_love::views::SimplyLoveApplyReplayGainEvent;
use log::info;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Shell-owned worker that runs a one-shot bulk ReplayGain (EBU R128) analysis
/// over the whole song library, driven by the Sound options "Apply ReplayGain"
/// action. Unlike the boot-time content reload pass, this is user-triggered and
/// reports progress to the Options screen. Cancellation reuses the crate-level
/// cooperative skip (`deadsync_audio_replaygain::request_skip_blocking_analysis`)
/// that also backs the startup skip, so both paths stop the same blocking pass.
#[derive(Default)]
pub(crate) struct Service {
    rx: Option<Receiver<SimplyLoveApplyReplayGainEvent>>,
}

impl Service {
    /// Spawn the analysis worker. No-op when a run is already in flight.
    pub(crate) fn start(&mut self) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        std::thread::spawn(move || {
            let paths = crate::content_reload::replaygain_music_paths(None);
            let total = paths.len();
            let _ = tx.send(SimplyLoveApplyReplayGainEvent::Started { total });
            if total == 0 {
                let _ = tx.send(SimplyLoveApplyReplayGainEvent::Finished {
                    done: 0,
                    total: 0,
                    cancelled: false,
                });
                return;
            }
            info!("Apply ReplayGain: analyzing loudness for {total} song(s)...");
            let mut last_done = 0usize;
            {
                let tx = &tx;
                let mut on_song = |done: usize, total: usize, path: &Path| {
                    last_done = done;
                    let (line2, line3) = crate::content_reload::cache_progress_lines(Some(path));
                    let _ = tx.send(SimplyLoveApplyReplayGainEvent::Progress {
                        done,
                        total,
                        line2,
                        line3,
                    });
                };
                deadsync_audio_replaygain::analyze_paths_blocking(paths, &mut on_song);
            }
            // `analyze_paths_blocking` only returns before every song is done
            // when a cooperative skip was requested, so a short count means the
            // run was cancelled.
            let cancelled = last_done < total;
            info!(
                "Apply ReplayGain: {} ({last_done}/{total} analyzed).",
                if cancelled { "cancelled" } else { "complete" }
            );
            let _ = tx.send(SimplyLoveApplyReplayGainEvent::Finished {
                done: last_done,
                total,
                cancelled,
            });
        });
    }

    pub(crate) fn poll(&mut self) -> Vec<SimplyLoveApplyReplayGainEvent> {
        let Some(rx) = self.rx.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    finished |= matches!(event, SimplyLoveApplyReplayGainEvent::Finished { .. });
                    events.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !finished {
                        events.push(SimplyLoveApplyReplayGainEvent::Finished {
                            done: 0,
                            total: 0,
                            cancelled: true,
                        });
                    }
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.rx = None;
        }
        events
    }
}
