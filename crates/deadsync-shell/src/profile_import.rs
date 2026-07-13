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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

const PENDING_EVENTS: usize = 64;

/// Shell-owned ITGmania discovery, native folder selection, and import worker.
pub(crate) struct Service {
    tx: mpsc::SyncSender<SimplyLoveProfileImportEvent>,
    rx: mpsc::Receiver<SimplyLoveProfileImportEvent>,
    import_cancel: Option<Arc<AtomicBool>>,
}

impl Default for Service {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(PENDING_EVENTS);
        Self {
            tx,
            rx,
            import_cancel: None,
        }
    }
}

impl Service {
    pub(crate) fn discover(&self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let candidates = candidate_views(detect_itg_local_profiles());
            let _ = tx.send(SimplyLoveProfileImportEvent::Candidates {
                candidates,
                browsed_dir: None,
            });
        });
    }

    pub(crate) fn browse(&self, title: String) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let Some(dir) = rfd::FileDialog::new().set_title(title).pick_folder() else {
                let _ = tx.send(SimplyLoveProfileImportEvent::BrowseCanceled);
                return;
            };
            let candidates = candidate_views(detect_itg_profiles_from_game_dir(&dir));
            let _ = tx.send(SimplyLoveProfileImportEvent::Candidates {
                candidates,
                browsed_dir: Some(dir),
            });
        });
    }

    pub(crate) fn start(&mut self, dir: PathBuf) {
        self.cancel();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = import_itg_profile(
                &dir,
                |done, total, label| {
                    let _ = tx.send(SimplyLoveProfileImportEvent::Progress {
                        done,
                        total,
                        label: label.to_owned(),
                    });
                },
                || thread_cancel.load(Ordering::Relaxed),
            )
            .map(import_summary)
            .map_err(|error| error.to_string());
            let _ = tx.send(SimplyLoveProfileImportEvent::Finished(result));
        });
        self.import_cancel = Some(cancel);
    }

    pub(crate) fn cancel(&self) {
        if let Some(cancel) = &self.import_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<SimplyLoveProfileImportEvent> {
        let events = self.rx.try_iter().collect::<Vec<_>>();
        if events
            .iter()
            .any(|event| matches!(event, SimplyLoveProfileImportEvent::Finished(_)))
        {
            self.import_cancel = None;
        }
        events
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
}
