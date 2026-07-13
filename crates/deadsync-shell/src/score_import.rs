use deadsync_online::score_compat as scores;
use deadsync_profile::Profile;
use deadsync_score::{ScoreBulkImportSummary, ScoreImportProgress};
use deadsync_theme_simply_love::{
    SimplyLoveScoreImportEvent, SimplyLoveScoreImportProgress, SimplyLoveScoreImportRequest,
    SimplyLoveScoreImportSummary,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const PENDING_EVENTS: usize = 64;

/// Shell-owned score-import worker, cancellation state, and bounded event queue.
pub(crate) struct Service {
    tx: mpsc::SyncSender<(u64, SimplyLoveScoreImportEvent)>,
    rx: mpsc::Receiver<(u64, SimplyLoveScoreImportEvent)>,
    active: Option<(u64, Arc<AtomicBool>)>,
    next_id: u64,
}

impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(PENDING_EVENTS);
        Self {
            tx,
            rx,
            active: None,
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
        self.active = Some((job_id, cancel));

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

            let mut runtime_profile = Profile::default();
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
                |progress| {
                    let _ = tx.send((
                        job_id,
                        SimplyLoveScoreImportEvent::Progress(progress_view(progress)),
                    ));
                },
                || thread_cancel.load(Ordering::Relaxed),
            )
            .map(summary_view)
            .map_err(|error| error.to_string());
            let _ = tx.send((job_id, SimplyLoveScoreImportEvent::Finished(result)));
        });
    }

    pub(crate) fn cancel(&self) {
        if let Some((_, cancel)) = &self.active {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<SimplyLoveScoreImportEvent> {
        let active_id = self.active.as_ref().map(|(id, _)| *id);
        let mut finished = false;
        let events = self
            .rx
            .try_iter()
            .filter_map(|(job_id, event)| {
                if Some(job_id) != active_id {
                    return None;
                }
                finished |= matches!(event, SimplyLoveScoreImportEvent::Finished(_));
                Some(event)
            })
            .collect::<Vec<_>>();
        if finished {
            self.active = None;
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

fn summary_view(summary: ScoreBulkImportSummary) -> SimplyLoveScoreImportSummary {
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
}
