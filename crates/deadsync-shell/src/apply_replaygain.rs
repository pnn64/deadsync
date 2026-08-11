use deadsync_theme_simply_love::views::SimplyLoveApplyReplayGainEvent;
use log::info;
use smallvec::SmallVec;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

use crate::content_reload::{
    PROGRESS_EVENTS_PER_FRAME, PROGRESS_QUEUE_CAPACITY, ProgressGate, receive_ready, send_progress,
};

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
        let (tx, rx) = mpsc::sync_channel(PROGRESS_QUEUE_CAPACITY);
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
                let mut gate = ProgressGate::default();
                let mut on_song = |done: usize, total: usize, path: &Path| {
                    last_done = done;
                    if !gate.should_emit(done, total) {
                        return;
                    }
                    let (line2, line3) = crate::content_reload::cache_progress_lines(Some(path));
                    send_progress(
                        tx,
                        done,
                        total,
                        SimplyLoveApplyReplayGainEvent::Progress {
                            done,
                            total,
                            line2,
                            line3,
                        },
                    );
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

    pub(crate) fn poll(
        &mut self,
    ) -> SmallVec<[SimplyLoveApplyReplayGainEvent; PROGRESS_EVENTS_PER_FRAME]> {
        let Some(rx) = self.rx.as_ref() else {
            return SmallVec::new();
        };
        let batch = receive_ready(rx);
        let mut events = batch.events;
        let mut finished = events
            .iter()
            .any(|event| matches!(event, SimplyLoveApplyReplayGainEvent::Finished { .. }));
        if batch.disconnected {
            if !finished {
                events.push(SimplyLoveApplyReplayGainEvent::Finished {
                    done: 0,
                    total: 0,
                    cancelled: true,
                });
            }
            finished = true;
        }
        if finished {
            self.rx = None;
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaygain_progress_poll_is_bounded_before_terminal_event() {
        let (tx, rx) = mpsc::channel();
        for done in 1..=10 {
            tx.send(SimplyLoveApplyReplayGainEvent::Progress {
                done,
                total: 10,
                line2: "Pack".to_owned(),
                line3: format!("Song {done}"),
            })
            .unwrap();
        }
        let mut service = Service { rx: Some(rx) };
        assert_eq!(service.poll().len(), PROGRESS_EVENTS_PER_FRAME);
        assert_eq!(service.poll().len(), 10 - PROGRESS_EVENTS_PER_FRAME);

        tx.send(SimplyLoveApplyReplayGainEvent::Finished {
            done: 10,
            total: 10,
            cancelled: false,
        })
        .unwrap();
        assert!(matches!(
            service.poll().as_slice(),
            [SimplyLoveApplyReplayGainEvent::Finished { done: 10, .. }]
        ));
        assert!(service.rx.is_none());
    }
}
