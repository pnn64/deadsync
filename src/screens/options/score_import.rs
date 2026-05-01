use super::*;

#[derive(Clone, Debug)]
pub(super) struct ScoreImportProfileConfig {
    pub(super) id: String,
    pub(super) display_name: String,
    pub(super) gs_api_key: String,
    pub(super) gs_username: String,
    pub(super) ac_api_key: String,
}

#[derive(Clone, Debug)]
pub(super) struct ScoreImportSelection {
    pub(super) endpoint: scores::ScoreImportEndpoint,
    pub(super) profile: ScoreImportProfileConfig,
    pub(super) pack_group: Option<String>,
    pub(super) pack_label: String,
    pub(super) only_missing_gs_scores: bool,
}

#[derive(Debug)]
pub(super) enum ScoreImportMsg {
    Progress(scores::ScoreImportProgress),
    Done(Result<scores::ScoreBulkImportSummary, String>),
}

pub(super) struct ScoreImportUiState {
    pub(super) endpoint: scores::ScoreImportEndpoint,
    pub(super) profile_name: String,
    pub(super) pack_label: String,
    pub(super) total_charts: usize,
    pub(super) processed_charts: usize,
    pub(super) imported_scores: usize,
    pub(super) missing_scores: usize,
    pub(super) failed_requests: usize,
    pub(super) detail_line: String,
    pub(super) done: bool,
    pub(super) done_message: String,
    pub(super) done_since: Option<Instant>,
    pub(super) cancel_requested: Arc<AtomicBool>,
    pub(super) rx: std::sync::mpsc::Receiver<ScoreImportMsg>,
}

impl ScoreImportUiState {
    pub(super) fn new(
        endpoint: scores::ScoreImportEndpoint,
        profile_name: String,
        pack_label: String,
        cancel_requested: Arc<AtomicBool>,
        rx: std::sync::mpsc::Receiver<ScoreImportMsg>,
    ) -> Self {
        Self {
            endpoint,
            profile_name,
            pack_label,
            total_charts: 0,
            processed_charts: 0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            detail_line: tr("OptionsScoreImport", "PreparingImport").to_string(),
            done: false,
            done_message: String::new(),
            done_since: None,
            cancel_requested,
            rx,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ScoreImportConfirmState {
    pub(super) selection: ScoreImportSelection,
    pub(super) active_choice: u8, // 0 = Yes, 1 = No
}

#[inline(always)]
pub(super) const fn score_import_endpoint_from_choice_index(
    idx: usize,
) -> scores::ScoreImportEndpoint {
    match idx {
        1 => scores::ScoreImportEndpoint::BoogieStats,
        2 => scores::ScoreImportEndpoint::ArrowCloud,
        _ => scores::ScoreImportEndpoint::GrooveStats,
    }
}

#[inline(always)]
pub(super) fn score_import_selected_endpoint(state: &State) -> scores::ScoreImportEndpoint {
    let idx = state
        .sub[SubmenuKind::ScoreImport].choice_indices
        .get(SCORE_IMPORT_ROW_ENDPOINT_INDEX)
        .copied()
        .unwrap_or(0);
    score_import_endpoint_from_choice_index(idx)
}

pub(super) fn installed_pack_options(all_label: &str) -> (Vec<String>, Vec<Option<String>>) {
    let cache = crate::game::song::get_song_cache();
    let mut packs: Vec<(String, String)> = Vec::with_capacity(cache.len());
    let mut seen_groups: HashSet<String> = HashSet::with_capacity(cache.len());

    for pack in cache.iter() {
        let group_name = pack.group_name.trim();
        if group_name.is_empty() {
            continue;
        }
        let group_key = group_name.to_ascii_lowercase();
        if !seen_groups.insert(group_key) {
            continue;
        }
        let display_name = if pack.name.trim().is_empty() {
            group_name.to_string()
        } else {
            pack.name.trim().to_string()
        };
        packs.push((display_name, group_name.to_string()));
    }

    packs.sort_by(|a, b| {
        a.0.to_ascii_lowercase()
            .cmp(&b.0.to_ascii_lowercase())
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut choices = Vec::with_capacity(packs.len() + 1);
    let mut filters = Vec::with_capacity(packs.len() + 1);
    choices.push(all_label.to_string());
    filters.push(None);
    for (display_name, group_name) in packs {
        choices.push(display_name);
        filters.push(Some(group_name));
    }
    (choices, filters)
}

pub(super) fn score_import_pack_options() -> (Vec<String>, Vec<Option<String>>) {
    installed_pack_options(&tr("OptionsScoreImport", "AllPacks"))
}

pub(super) fn sync_pack_options() -> (Vec<String>, Vec<Option<String>>) {
    installed_pack_options(&tr("OptionsSyncPack", "AllPacks"))
}

pub(super) fn load_score_import_profiles() -> Vec<ScoreImportProfileConfig> {
    let mut profiles = Vec::new();
    for summary in profile::scan_local_profiles() {
        let profile_dir = dirs::app_dirs().profiles_root().join(summary.id.as_str());
        let mut gs = SimpleIni::new();
        let mut ac = SimpleIni::new();
        let gs_api_key = if gs.load(profile_dir.join("groovestats.ini")).is_ok() {
            gs.get("GrooveStats", "ApiKey")
                .map_or_else(String::new, |v| v.trim().to_string())
        } else {
            String::new()
        };
        let gs_username = if gs_api_key.is_empty() {
            String::new()
        } else {
            gs.get("GrooveStats", "Username")
                .map_or_else(String::new, |v| v.trim().to_string())
        };
        let ac_api_key = if ac.load(profile_dir.join("arrowcloud.ini")).is_ok() {
            ac.get("ArrowCloud", "ApiKey")
                .map_or_else(String::new, |v| v.trim().to_string())
        } else {
            String::new()
        };
        profiles.push(ScoreImportProfileConfig {
            id: summary.id,
            display_name: summary.display_name.trim().to_string(),
            gs_api_key,
            gs_username,
            ac_api_key,
        });
    }
    profiles.sort_by(|a, b| {
        let al = a.display_name.to_ascii_lowercase();
        let bl = b.display_name.to_ascii_lowercase();
        al.cmp(&bl).then_with(|| a.id.cmp(&b.id))
    });
    profiles
}

pub(super) fn score_import_profile_eligible(
    endpoint: scores::ScoreImportEndpoint,
    profile_cfg: &ScoreImportProfileConfig,
) -> bool {
    match endpoint {
        scores::ScoreImportEndpoint::GrooveStats | scores::ScoreImportEndpoint::BoogieStats => {
            !profile_cfg.gs_api_key.is_empty() && !profile_cfg.gs_username.is_empty()
        }
        scores::ScoreImportEndpoint::ArrowCloud => !profile_cfg.ac_api_key.is_empty(),
    }
}

pub(super) fn refresh_score_import_profile_options(state: &mut State) {
    state.score_import_profile_choices.clear();
    state.score_import_profile_ids.clear();

    let endpoint = score_import_selected_endpoint(state);
    for profile_cfg in &state.score_import_profiles {
        if !score_import_profile_eligible(endpoint, profile_cfg) {
            continue;
        }
        let label = if profile_cfg.display_name.is_empty() {
            profile_cfg.id.clone()
        } else {
            format!("{} ({})", profile_cfg.display_name, profile_cfg.id)
        };
        state.score_import_profile_choices.push(label);
        state
            .score_import_profile_ids
            .push(Some(profile_cfg.id.clone()));
    }
    if state.score_import_profile_choices.is_empty() {
        state
            .score_import_profile_choices
            .push(tr("OptionsScoreImport", "NoEligibleProfiles").to_string());
        state.score_import_profile_ids.push(None);
    }

    let max_idx = state.score_import_profile_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub[SubmenuKind::ScoreImport].choice_indices
        .get_mut(SCORE_IMPORT_ROW_PROFILE_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub[SubmenuKind::ScoreImport].cursor_indices
        .get_mut(SCORE_IMPORT_ROW_PROFILE_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

pub(super) fn refresh_score_import_pack_options(state: &mut State) {
    let (choices, filters) = score_import_pack_options();
    state.score_import_pack_choices = choices;
    state.score_import_pack_filters = filters;
    let max_idx = state.score_import_pack_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub[SubmenuKind::ScoreImport].choice_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub[SubmenuKind::ScoreImport].cursor_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

pub(super) fn refresh_sync_pack_options(state: &mut State) {
    let (choices, filters) = sync_pack_options();
    state.sync_pack_choices = choices;
    state.sync_pack_filters = filters;
    let max_idx = state.sync_pack_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub[SubmenuKind::SyncPacks].choice_indices
        .get_mut(SYNC_PACK_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub[SubmenuKind::SyncPacks].cursor_indices
        .get_mut(SYNC_PACK_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

pub(super) fn refresh_score_import_options(state: &mut State) {
    state.score_import_profiles = load_score_import_profiles();
    refresh_score_import_profile_options(state);
    refresh_score_import_pack_options(state);
}

pub(super) fn refresh_null_or_die_options(state: &mut State) {
    refresh_sync_pack_options(state);
}

pub(super) fn selected_score_import_pack_group(state: &State) -> Option<String> {
    let pack_idx = state
        .sub[SubmenuKind::ScoreImport].choice_indices
        .get(SCORE_IMPORT_ROW_PACK_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.score_import_pack_filters.len().saturating_sub(1));
    state
        .score_import_pack_filters
        .get(pack_idx)
        .cloned()
        .flatten()
}

pub(super) fn selected_score_import_profile(state: &State) -> Option<ScoreImportProfileConfig> {
    let profile_idx = state
        .sub[SubmenuKind::ScoreImport].choice_indices
        .get(SCORE_IMPORT_ROW_PROFILE_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.score_import_profile_ids.len().saturating_sub(1));
    let profile_id = state
        .score_import_profile_ids
        .get(profile_idx)
        .cloned()
        .flatten()?;
    state
        .score_import_profiles
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
}

pub(super) fn score_import_only_missing_gs_scores(state: &State) -> bool {
    yes_no_from_choice(
        state
            .sub[SubmenuKind::ScoreImport].choice_indices
            .get(SCORE_IMPORT_ROW_ONLY_MISSING_INDEX)
            .copied()
            .unwrap_or_else(|| yes_no_choice_index(false)),
    )
}

pub(super) fn selected_score_import_selection(state: &State) -> Option<ScoreImportSelection> {
    let endpoint = score_import_selected_endpoint(state);
    let profile_cfg = selected_score_import_profile(state)?;
    if !score_import_profile_eligible(endpoint, &profile_cfg) {
        return None;
    }
    let pack_group = selected_score_import_pack_group(state);
    let pack_label = pack_group
        .as_ref()
        .cloned()
        .unwrap_or_else(|| tr("OptionsScoreImport", "AllPacks").to_string());
    let only_missing_gs_scores = score_import_only_missing_gs_scores(state);
    Some(ScoreImportSelection {
        endpoint,
        profile: profile_cfg,
        pack_group,
        pack_label,
        only_missing_gs_scores,
    })
}

pub(super) fn begin_score_import(state: &mut State, selection: ScoreImportSelection) {
    if state.score_import_ui.is_some() {
        return;
    }
    clear_navigation_holds(state);
    let mut profile_cfg = profile::Profile::default();
    profile_cfg
        .display_name
        .clone_from(&selection.profile.display_name);
    profile_cfg
        .groovestats_api_key
        .clone_from(&selection.profile.gs_api_key);
    profile_cfg
        .groovestats_username
        .clone_from(&selection.profile.gs_username);
    profile_cfg
        .arrowcloud_api_key
        .clone_from(&selection.profile.ac_api_key);

    let endpoint = selection.endpoint;
    let profile_id = selection.profile.id.clone();
    let profile_name = if selection.profile.display_name.is_empty() {
        selection.profile.id.clone()
    } else {
        selection.profile.display_name.clone()
    };
    let pack_group = selection.pack_group.clone();
    let pack_label = selection.pack_label.clone();
    let only_missing_gs_scores = selection.only_missing_gs_scores;

    log::warn!(
        "{} score import starting for '{}' (pack: {}, only_missing_gs={}). Hard-limited to 3 requests/sec. For many charts this can take more than one hour.",
        endpoint.display_name(),
        profile_name,
        pack_label,
        if only_missing_gs_scores { "yes" } else { "no" }
    );

    let cancel_requested = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel_requested);
    let (tx, rx) = std::sync::mpsc::channel::<ScoreImportMsg>();
    state.score_import_ui = Some(ScoreImportUiState::new(
        endpoint,
        profile_name.clone(),
        pack_label,
        cancel_requested,
        rx,
    ));

    std::thread::spawn(move || {
        let result = scores::import_scores_for_profile(
            endpoint,
            profile_id,
            profile_cfg,
            pack_group,
            only_missing_gs_scores,
            |progress| {
                let _ = tx.send(ScoreImportMsg::Progress(progress));
            },
            || cancel_for_thread.load(Ordering::Relaxed),
        );
        let done_msg = result.map_err(|e| e.to_string());
        let _ = tx.send(ScoreImportMsg::Done(done_msg));
    });
}

pub(super) fn begin_score_import_from_confirm(state: &mut State) {
    let Some(confirm) = state.score_import_confirm.take() else {
        return;
    };
    begin_score_import(state, confirm.selection);
}

pub(super) fn poll_score_import_ui(score_import: &mut ScoreImportUiState) {
    while let Ok(msg) = score_import.rx.try_recv() {
        match msg {
            ScoreImportMsg::Progress(progress) => {
                score_import.total_charts = progress.total_charts;
                score_import.processed_charts = progress.processed_charts;
                score_import.imported_scores = progress.imported_scores;
                score_import.missing_scores = progress.missing_scores;
                score_import.failed_requests = progress.failed_requests;
                score_import.detail_line = progress.detail;
            }
            ScoreImportMsg::Done(result) => {
                score_import.done = true;
                score_import.done_since = Some(Instant::now());
                score_import.done_message = match result {
                    Ok(summary) => {
                        if summary.canceled {
                            format!(
                                "Canceled: requested={}, imported={}, missing={}, failed={} (elapsed {:.1}s)",
                                summary.requested_charts,
                                summary.imported_scores,
                                summary.missing_scores,
                                summary.failed_requests,
                                summary.elapsed_seconds
                            )
                        } else {
                            format!(
                                "Complete: requested={}, imported={}, missing={}, failed={}, rate={} req/s (elapsed {:.1}s)",
                                summary.requested_charts,
                                summary.imported_scores,
                                summary.missing_scores,
                                summary.failed_requests,
                                summary.rate_limit_per_second,
                                summary.elapsed_seconds
                            )
                        }
                    }
                    Err(e) => tr_fmt(
                        "OptionsScoreImport",
                        "ImportFailed",
                        &[("error", &e.to_string())],
                    )
                    .to_string(),
                };
            }
        }
    }
}
