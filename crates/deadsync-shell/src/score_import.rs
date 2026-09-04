use crate::latest_worker_value::LatestWorkerValue;
use deadsync_online::score_compat as scores;
use deadsync_score::{ScoreBulkImportSummary, ScoreImportProgress};
use deadsync_theme_simply_love::{
    SimplyLoveScoreImportEvent, SimplyLoveScoreImportProgress, SimplyLoveScoreImportRequest,
    SimplyLoveScoreImportSummary,
};
use smallvec::SmallVec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const PENDING_TERMINALS: usize = 8;
const TERMINALS_PER_FRAME: usize = 8;
const FRAME_EVENTS: usize = 2;
type ImportResult = Result<SimplyLoveScoreImportSummary, String>;

/// Shell-owned score-import worker, cancellation, and frame handoff.
///
/// The worker owns network and score-cache work. Replaceable progress lives in
/// one latest-value slot, while the reliable completion crosses an eight-entry
/// bounded queue. The frame thread uses a nonblocking progress sample and drains
/// at most eight stale/current terminals into two inline event slots. Progress
/// replacement destroys old detail strings on the worker; completion destroys
/// the slot on the frame thread. There is no gameplay miss path, eviction scan,
/// or heap allocation for the event container. `score_import_handoff` measures
/// worker publication and worst burst integration; the frame work bound is one
/// progress sample plus eight terminal probes.
pub(crate) struct Service {
    tx: mpsc::SyncSender<(u64, ImportResult)>,
    rx: mpsc::Receiver<(u64, ImportResult)>,
    progress: Arc<LatestWorkerValue<SimplyLoveScoreImportProgress>>,
    active: Option<(u64, Arc<AtomicBool>)>,
    active_jobs: usize,
    next_id: u64,
}

impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(PENDING_TERMINALS);
        Self {
            tx,
            rx,
            progress: Arc::new(LatestWorkerValue::default()),
            active: None,
            active_jobs: 0,
            next_id: 0,
        }
    }
}

impl Service {
    pub(crate) fn start(&mut self, request: SimplyLoveScoreImportRequest) {
        self.cancel();
        self.next_id = self.next_id.wrapping_add(1);
        let job_id = self.next_id;
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let tx = self.tx.clone();
        let progress = Arc::clone(&self.progress);
        self.active = Some((job_id, cancel));
        self.active_jobs += 1;
        progress.start(job_id);

        std::thread::spawn(move || {
            let SimplyLoveScoreImportRequest {
                endpoint,
                profile,
                pack_groups,
                only_missing_groovestats_scores,
            } = request;
            let profile_name = if profile.display_name.is_empty() {
                profile.id.as_str()
            } else {
                profile.display_name.as_str()
            };
            log::warn!(
                "{} score import starting for '{}' ({} pack groups, only_missing_gs={}). {}",
                endpoint.display_name(),
                profile_name,
                pack_groups.len(),
                if only_missing_groovestats_scores {
                    "yes"
                } else {
                    "no"
                },
                match endpoint {
                    deadsync_score::ScoreImportEndpoint::ArrowCloud =>
                        "Bulk-imported per pack at 3 requests/sec (up to 1000 charts per request).",
                    _ =>
                        "Hard-limited to 3 requests/sec. For many charts this can take more than one hour.",
                }
            );

            let mut runtime_profile = deadsync_profile::Profile::default();
            runtime_profile.display_name = profile.display_name;
            runtime_profile.groovestats_api_key = profile.groovestats_api_key;
            runtime_profile.groovestats_username = profile.groovestats_username;
            runtime_profile.arrowcloud_api_key = profile.arrowcloud_api_key;
            let result = scores::import_scores_for_profile(
                endpoint,
                profile.id,
                runtime_profile,
                pack_groups,
                only_missing_groovestats_scores,
                |sample| {
                    progress.publish(job_id, progress_view(sample));
                },
                || thread_cancel.load(Ordering::Relaxed),
            )
            .map(summary_view)
            .map_err(|error| error.to_string());
            let _ = tx.send((job_id, result));
        });
    }

    pub(crate) fn cancel(&self) {
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Returns `None` without touching the queue when no import worker is active.
    /// Each worker sends exactly one terminal, so superseded jobs keep polling
    /// until their completion has been removed from the shared queue.
    pub(crate) fn poll(&mut self) -> Option<SmallVec<[SimplyLoveScoreImportEvent; FRAME_EVENTS]>> {
        if self.active_jobs == 0 {
            return None;
        }
        Some(self.drain_events())
    }

    fn drain_events(&mut self) -> SmallVec<[SimplyLoveScoreImportEvent; FRAME_EVENTS]> {
        let active_id = self.active.as_ref().map(|(id, _)| *id);
        let mut events = SmallVec::new();
        if let Some(job_id) = active_id
            && let Some(progress) = self.progress.take(job_id)
        {
            events.push(SimplyLoveScoreImportEvent::Progress(progress));
        }
        for _ in 0..TERMINALS_PER_FRAME {
            let Ok((job_id, result)) = self.rx.try_recv() else {
                break;
            };
            self.active_jobs = self.active_jobs.saturating_sub(1);
            if Some(job_id) != active_id {
                continue;
            }
            self.active = None;
            self.progress.finish(job_id);
            events.push(SimplyLoveScoreImportEvent::Finished(result));
            break;
        }
        events
    }
}

fn progress_view(progress: ScoreImportProgress) -> SimplyLoveScoreImportProgress {
    SimplyLoveScoreImportProgress {
        processed_charts: progress.processed_charts,
        total_charts: progress.total_charts,
        imported_scores: progress.imported_scores,
        missing_scores: progress.missing_scores,
        failed_requests: progress.failed_requests,
        detail: progress.detail,
    }
}

const fn summary_view(summary: ScoreBulkImportSummary) -> SimplyLoveScoreImportSummary {
    SimplyLoveScoreImportSummary {
        requested_charts: summary.requested_charts,
        imported_scores: summary.imported_scores,
        missing_scores: summary.missing_scores,
        failed_requests: summary.failed_requests,
        rate_limit_per_second: summary.rate_limit_per_second,
        elapsed_seconds: summary.elapsed_seconds,
        canceled: summary.canceled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_progress(done: usize, total: usize, detail: &str) -> SimplyLoveScoreImportProgress {
        SimplyLoveScoreImportProgress {
            processed_charts: done,
            total_charts: total,
            imported_scores: done.saturating_sub(2),
            missing_scores: done.min(2),
            failed_requests: 0,
            detail: detail.to_owned(),
        }
    }

    fn active_service(job_id: u64) -> Service {
        let mut service = Service::default();
        service.active = Some((job_id, Arc::new(AtomicBool::new(false))));
        service.active_jobs = 1;
        service.progress.start(job_id);
        service
    }

    #[test]
    fn score_import_summary_keeps_theme_visible_fields() {
        let view = summary_view(ScoreBulkImportSummary {
            requested_charts: 12,
            imported_scores: 8,
            missing_scores: 3,
            failed_requests: 1,
            rate_limit_per_second: 3,
            elapsed_seconds: 4.5,
            canceled: false,
        });
        assert_eq!(view.requested_charts, 12);
        assert_eq!(view.imported_scores, 8);
        assert_eq!(view.elapsed_seconds, 4.5);
        assert!(!view.canceled);
    }

    #[test]
    fn score_import_poll_only_runs_while_a_job_is_active() {
        let mut service = Service::default();
        assert!(service.poll().is_none());

        service.active = Some((7, Arc::new(AtomicBool::new(false))));
        service.active_jobs = 1;
        service.progress.start(7);
        assert!(service.poll().is_some_and(|events| events.is_empty()));
        service
            .tx
            .send((7, Err("finished".to_owned())))
            .expect("the service owns the matching receiver");
        let events = service.poll().expect("the job is active");

        assert!(matches!(
            events.as_slice(),
            [SimplyLoveScoreImportEvent::Finished(Err(reason))] if reason == "finished"
        ));
        assert!(service.poll().is_none());
    }

    #[test]
    fn score_import_progress_keeps_only_the_latest_sample() {
        let mut service = active_service(1);
        for done in 1..=64 {
            service
                .progress
                .publish(1, test_progress(done, 64, &"p".repeat(24)));
        }
        let events = service.poll().expect("the import is active");

        assert!(matches!(
            events.as_slice(),
            [SimplyLoveScoreImportEvent::Progress(progress)]
                if progress.processed_charts == 64 && progress.detail.len() == 24
        ));
        assert!(!events.spilled());
    }

    #[test]
    fn score_import_delivers_latest_progress_before_completion() {
        let mut service = active_service(1);
        for done in 1..=8 {
            service
                .progress
                .publish(1, test_progress(done, 8, "last visible"));
        }
        service
            .tx
            .send((
                1,
                Ok(SimplyLoveScoreImportSummary {
                    requested_charts: 8,
                    imported_scores: 6,
                    missing_scores: 2,
                    failed_requests: 0,
                    rate_limit_per_second: 3,
                    elapsed_seconds: 1.0,
                    canceled: false,
                }),
            ))
            .expect("the service owns the matching receiver");

        let events = service.poll().expect("the import is active");
        assert!(matches!(
            events.as_slice(),
            [
                SimplyLoveScoreImportEvent::Progress(progress),
                SimplyLoveScoreImportEvent::Finished(Ok(_)),
            ] if progress.processed_charts == 8
        ));
        assert!(!events.spilled());
        assert!(service.poll().is_none());
    }

    #[test]
    fn superseded_score_events_do_not_replace_or_finish_active_job() {
        let mut service = active_service(2);
        service.active_jobs = 2;
        service.progress.publish(1, test_progress(1, 2, "stale"));
        service.progress.publish(2, test_progress(3, 4, "current"));
        service
            .tx
            .send((1, Err("stale completion".to_owned())))
            .expect("the service owns the matching receiver");

        let events = service.poll().expect("the current import is active");
        assert!(matches!(
            events.as_slice(),
            [SimplyLoveScoreImportEvent::Progress(progress)]
                if progress.processed_charts == 3 && progress.detail == "current"
        ));
        assert!(service.active.is_some());
        assert_eq!(service.active_jobs, 1);
    }
}
