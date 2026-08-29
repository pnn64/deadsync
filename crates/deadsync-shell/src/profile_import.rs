use crate::latest_worker_value::LatestWorkerValue;
use deadsync_import::app_runtime::{ImportSummary, import_itg_profile_dir};
use deadsync_import::detect::{
    ItgProfileCandidate, detect_itg_local_profiles, detect_itg_profiles_from_game_dir,
};
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_simfile::runtime_cache::get_song_cache;
use deadsync_theme_simply_love::{
    SimplyLoveItgImportSummary, SimplyLoveItgProfileCandidate, SimplyLoveProfileImportEvent,
};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const PENDING_TERMINALS: usize = 8;
const TERMINALS_PER_FRAME: usize = 8;
const FRAME_EVENTS: usize = TERMINALS_PER_FRAME + 1;

enum WorkerEvent {
    View(SimplyLoveProfileImportEvent),
    ImportFinished {
        import_id: u64,
        result: Result<SimplyLoveItgImportSummary, String>,
    },
}

/// Shell-owned `ITGmania` discovery, native folder selection, and import worker.
///
/// Progress is sampled through one latest-value slot because the screen replaces
/// its progress row on every update. Discovery and completion still cross a
/// reliable eight-entry queue and integration drains at most eight terminals
/// plus one progress sample per frame. Replaced progress strings are destroyed
/// on the worker, never as an unbounded application-frame burst.
pub(crate) struct Service {
    tx: mpsc::SyncSender<WorkerEvent>,
    rx: mpsc::Receiver<WorkerEvent>,
    progress: Arc<LatestWorkerValue<(usize, usize, String)>>,
    active_import: Option<(u64, Arc<AtomicBool>)>,
    active_jobs: usize,
    next_import_id: u64,
}

impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(PENDING_TERMINALS);
        Self {
            tx,
            rx,
            progress: Arc::new(LatestWorkerValue::default()),
            active_import: None,
            active_jobs: 0,
            next_import_id: 0,
        }
    }
}

impl Service {
    pub(crate) fn discover(&mut self) {
        self.active_jobs += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let candidates = candidate_views(detect_itg_local_profiles());
            let _ = tx.send(WorkerEvent::View(
                SimplyLoveProfileImportEvent::Candidates {
                    candidates,
                    browsed_dir: None,
                },
            ));
        });
    }

    pub(crate) fn browse(&mut self, title: String) {
        self.active_jobs += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let Some(dir) = rfd::FileDialog::new().set_title(title).pick_folder() else {
                let _ = tx.send(WorkerEvent::View(
                    SimplyLoveProfileImportEvent::BrowseCanceled,
                ));
                return;
            };
            let candidates = candidate_views(detect_itg_profiles_from_game_dir(&dir));
            let _ = tx.send(WorkerEvent::View(
                SimplyLoveProfileImportEvent::Candidates {
                    candidates,
                    browsed_dir: Some(dir),
                },
            ));
        });
    }

    pub(crate) fn start(&mut self, dir: PathBuf) {
        self.cancel();
        self.next_import_id = self.next_import_id.wrapping_add(1).max(1);
        let import_id = self.next_import_id;
        self.active_jobs += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let tx = self.tx.clone();
        let progress = Arc::clone(&self.progress);
        progress.start(import_id);
        std::thread::spawn(move || {
            let result = import_itg_profile(
                &dir,
                |done, total, label| {
                    progress.publish(import_id, (done, total, label.to_owned()));
                },
                || thread_cancel.load(Ordering::Relaxed),
            )
            .map(import_summary)
            .map_err(|error| error.to_string());
            let _ = tx.send(WorkerEvent::ImportFinished { import_id, result });
        });
        self.active_import = Some((import_id, cancel));
    }

    pub(crate) fn cancel(&self) {
        if let Some((_, cancel)) = &self.active_import {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Returns `None` without probing the channel when discovery, browsing,
    /// and import workers are all inactive. Every worker sends exactly one
    /// terminal event, which keeps the activity count live until it is drained.
    pub(crate) fn poll(
        &mut self,
    ) -> Option<SmallVec<[SimplyLoveProfileImportEvent; FRAME_EVENTS]>> {
        if self.active_jobs == 0 {
            return None;
        }
        Some(self.drain_events())
    }

    fn drain_events(&mut self) -> SmallVec<[SimplyLoveProfileImportEvent; FRAME_EVENTS]> {
        let mut events = SmallVec::new();
        if let Some((import_id, _)) = self.active_import.as_ref()
            && let Some((done, total, label)) = self.progress.take(*import_id)
        {
            events.push(SimplyLoveProfileImportEvent::Progress { done, total, label });
        }
        for _ in 0..TERMINALS_PER_FRAME {
            let Ok(event) = self.rx.try_recv() else {
                break;
            };
            self.active_jobs = self.active_jobs.saturating_sub(1);
            match event {
                WorkerEvent::View(event) => events.push(event),
                WorkerEvent::ImportFinished { import_id, result } => {
                    if self
                        .active_import
                        .as_ref()
                        .is_some_and(|(active_id, _)| *active_id == import_id)
                    {
                        self.active_import = None;
                        self.progress.finish(import_id);
                        events.push(SimplyLoveProfileImportEvent::Finished(result));
                    }
                }
            }
        }
        events
    }
}

#[cfg(feature = "bench-support")]
pub struct BenchmarkProfileImportService(Service);

#[cfg(feature = "bench-support")]
impl BenchmarkProfileImportService {
    #[must_use]
    pub fn active() -> Self {
        let mut service = Service::default();
        let import_id = 1;
        service.active_jobs = 1;
        service.active_import = Some((import_id, Arc::new(AtomicBool::new(false))));
        service.progress.start(import_id);
        Self(service)
    }

    pub fn publish_progress(&self, done: usize, total: usize, label: &str) {
        self.0.progress.publish(1, (done, total, label.to_owned()));
    }

    #[must_use]
    pub fn with_progress_burst(events: usize, label_bytes: usize) -> Self {
        let service = Self::active();
        let label = "p".repeat(label_bytes);
        for done in 1..=events {
            service.publish_progress(done, events, &label);
        }
        service
    }

    pub fn poll(&mut self) -> Option<SmallVec<[SimplyLoveProfileImportEvent; FRAME_EVENTS]>> {
        self.0.poll()
    }
}

fn candidate_views(candidates: Vec<ItgProfileCandidate>) -> Vec<SimplyLoveItgProfileCandidate> {
    let existing = profile::scan_local_profiles()
        .into_iter()
        .map(|profile| (profile.id, profile.display_name))
        .collect::<HashMap<_, _>>();
    candidates
        .into_iter()
        .map(|candidate| SimplyLoveItgProfileCandidate {
            dir: candidate.dir,
            display_name: candidate.display_name,
            imported_as: candidate
                .source_guid
                .as_deref()
                .and_then(profile_data::profile_guid_from_itgmania_guid)
                .and_then(|guid| existing.get(&guid).cloned()),
        })
        .collect()
}

fn import_itg_profile<F, C>(
    dir: &Path,
    mut on_progress: F,
    should_cancel: C,
) -> Result<ImportSummary, deadsync_import::itg::ItgReadError>
where
    F: FnMut(usize, usize, &str),
    C: Fn() -> bool,
{
    let (base_singles, base_doubles) = profile::default_local_profile_options();
    let packs = get_song_cache();
    import_itg_profile_dir(
        dir,
        &base_singles,
        &base_doubles,
        &packs,
        |profile_guid| {
            profile::scan_local_profiles()
                .into_iter()
                .find(|profile| profile.id == profile_guid)
                .map(|profile| profile.display_name)
        },
        profile::create_local_profile_from_import,
        |profile_id, initials, mut entries| {
            scores::import_local_scores(
                profile_id,
                initials,
                &mut entries,
                |done, total| on_progress(done, total, ""),
                &should_cancel,
            )
        },
        |profile_id| {
            if let Err(error) = profile::delete_local_profile(profile_id) {
                log::warn!("Failed to delete canceled import profile {profile_id}: {error}");
            }
        },
        profile::write_imported_favorites,
        profile::write_imported_profile_stats,
        scores::import_itl_json,
    )
}

fn import_summary(summary: ImportSummary) -> SimplyLoveItgImportSummary {
    SimplyLoveItgImportSummary {
        profile_id: summary.profile_id,
        display_name: summary.display_name,
        scores_total: summary.scores_total,
        scores_imported: summary.scores_imported,
        charts_song_not_found: summary.charts_song_not_found,
        charts_chart_not_found: summary.charts_chart_not_found,
        scores_unmapped: summary.scores_unmapped,
        favorites_total: summary.favorites_total,
        favorites_imported: summary.favorites_imported,
        itl_entries_imported: summary.itl_entries_imported,
        simply_love_options_imported: summary.simply_love_options_imported,
        groovestats_imported: summary.groovestats_imported,
        arrowcloud_imported: summary.arrowcloud_imported,
        avatar_imported: summary.avatar_imported,
        canceled: summary.canceled,
        already_imported_as: summary.already_imported_as,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_summary_keeps_theme_visible_fields() {
        let summary = import_summary(ImportSummary {
            profile_id: "profile".to_owned(),
            display_name: "Player".to_owned(),
            scores_total: 5,
            scores_imported: 4,
            favorites_total: 3,
            favorites_imported: 2,
            groovestats_imported: true,
            ..ImportSummary::default()
        });
        assert_eq!(summary.profile_id, "profile");
        assert_eq!(summary.scores_imported, 4);
        assert_eq!(summary.favorites_imported, 2);
        assert!(summary.online_keys_imported());
    }

    #[test]
    fn profile_import_poll_only_runs_while_a_worker_is_active() {
        let mut service = Service::default();
        assert!(service.poll().is_none());

        service.active_jobs = 1;
        assert!(service.poll().is_some_and(|events| events.is_empty()));
        service
            .tx
            .send(WorkerEvent::View(
                SimplyLoveProfileImportEvent::BrowseCanceled,
            ))
            .expect("the service owns the matching receiver");
        assert!(matches!(
            service.poll().expect("the worker is active").as_slice(),
            [SimplyLoveProfileImportEvent::BrowseCanceled]
        ));
        assert!(service.poll().is_none());
    }

    #[test]
    fn profile_import_progress_keeps_only_the_latest_sample() {
        let mut service = Service::default();
        let import_id = 1;
        service.active_jobs = 1;
        service.active_import = Some((import_id, Arc::new(AtomicBool::new(false))));
        service.progress.start(import_id);
        for done in 1..=64 {
            service
                .progress
                .publish(import_id, (done, 64, "p".repeat(24)));
        }
        let events = service.poll().expect("the import is active");

        assert!(matches!(
            events.as_slice(),
            [SimplyLoveProfileImportEvent::Progress { done: 64, total: 64, label }]
                if label.len() == 24
        ));
    }

    #[test]
    fn superseded_import_progress_cannot_replace_the_active_sample() {
        let progress = LatestWorkerValue::default();
        progress.start(1);
        progress.publish(1, (1, 2, "old".to_owned()));
        progress.start(2);
        progress.publish(1, (2, 2, "stale".to_owned()));
        progress.publish(2, (3, 4, "current".to_owned()));

        assert!(matches!(
            progress.take(2),
            Some((3, 4, label))
                if label == "current"
        ));
    }

    #[test]
    fn current_import_delivers_latest_progress_before_completion() {
        let mut service = Service::default();
        let import_id = 3;
        service.active_jobs = 1;
        service.active_import = Some((import_id, Arc::new(AtomicBool::new(false))));
        service.progress.start(import_id);
        service
            .progress
            .publish(import_id, (8, 10, "last visible".to_owned()));
        service
            .tx
            .send(WorkerEvent::ImportFinished {
                import_id,
                result: Ok(SimplyLoveItgImportSummary::default()),
            })
            .expect("the service owns the matching receiver");

        let events = service.poll().expect("the import is active");
        assert!(matches!(
            events.as_slice(),
            [
                SimplyLoveProfileImportEvent::Progress {
                    done: 8,
                    total: 10,
                    ..
                },
                SimplyLoveProfileImportEvent::Finished(Ok(_)),
            ]
        ));
        assert!(service.poll().is_none());
    }

    #[test]
    fn superseded_completion_does_not_finish_the_active_import() {
        let mut service = Service::default();
        service.active_jobs = 2;
        service.active_import = Some((2, Arc::new(AtomicBool::new(false))));
        service.progress.start(2);
        service.progress.publish(2, (1, 2, "current".to_owned()));
        service
            .tx
            .send(WorkerEvent::ImportFinished {
                import_id: 1,
                result: Err("stale".to_owned()),
            })
            .expect("the service owns the matching receiver");

        let events = service.poll().expect("workers remain active");
        assert!(matches!(
            events.as_slice(),
            [SimplyLoveProfileImportEvent::Progress { label, .. }] if label == "current"
        ));
        assert!(service.active_import.is_some());
        assert_eq!(service.active_jobs, 1);
    }
}
