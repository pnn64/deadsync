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
    /// Smoothed `processed_charts` (eased toward the integer target each
    /// frame). Used for the bar fill and the displayed speed so progress
    /// doesn't jump in big steps when a bulk request lands.
    pub(super) displayed_done: f32,
    pub(super) imported_scores: usize,
    pub(super) missing_scores: usize,
    pub(super) failed_requests: usize,
    pub(super) detail_line: String,
    pub(super) done: bool,
    pub(super) done_message: String,
    pub(super) done_since: Option<Instant>,
    pub(super) started_at: Instant,
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
            displayed_done: 0.0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            detail_line: tr("OptionsScoreImport", "PreparingImport").to_string(),
            done: false,
            done_message: String::new(),
            done_since: None,
            started_at: Instant::now(),
            cancel_requested,
            rx,
        }
    }
}

/// Time constant (seconds) for the exponential ease applied to `displayed_done`.
/// Smaller = snappier, larger = smoother. ~0.4s feels close to instant on
/// chunks that arrive in <1s but visibly fills across multi-second waits.
const SCORE_IMPORT_PROGRESS_TAU: f32 = 0.4;

/// Format an ETA in seconds as a compact human string (e.g. ``45s``,
/// ``2m 13s``, ``1h 04m``). Returns ``--`` for absurdly large values.
fn format_eta(secs: u64) -> String {
    if secs >= 24 * 60 * 60 {
        return "--".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    let rem_s = secs % 60;
    if mins < 60 {
        return format!("{mins}m {rem_s:02}s");
    }
    let hours = mins / 60;
    let rem_m = mins % 60;
    format!("{hours}h {rem_m:02}m")
}

#[inline(always)]
pub(super) fn score_import_progress(
    score_import: &ScoreImportUiState,
) -> (usize, usize, f32) {
    let done = score_import.processed_charts;
    let mut total = score_import.total_charts;
    if total < done {
        total = done;
    }
    let smoothed = score_import
        .displayed_done
        .clamp(0.0, total.max(done) as f32);
    let mut progress = if total > 0 {
        (smoothed / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !score_import.done && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

pub(super) fn build_score_import_overlay_actors(
    score_import: &ScoreImportUiState,
    active_color_index: i32,
) -> Vec<Actor> {
    let (done, total, progress) = score_import_progress(score_import);
    let elapsed = score_import.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0 || done > 0;
    let speed_text = if show_speed_row {
        // Use the smoothed displayed value so the speed readout doesn't spike
        // every time a bulk chunk lands. Once `done` is set the worker has
        // stopped, so report 0 immediately rather than the historical average.
        let rate = if score_import.done {
            0.0
        } else {
            let smoothed_done = score_import.displayed_done.max(0.0);
            if elapsed > 0.0 {
                smoothed_done / elapsed
            } else {
                0.0
            }
        };
        let mut text = tr_fmt(
            "SelectMusic",
            "LoadingSpeed",
            &[("speed", &format!("{rate:.0}"))],
        )
        .to_string();
        if !score_import.done && total > 0 && rate > 0.0 {
            let remaining = total.saturating_sub(done) as f32;
            if remaining > 0.0 {
                let eta_secs = (remaining / rate).round() as u64;
                text = format!(
                    "{text}  \u{2022}  {}",
                    tr_fmt(
                        "OptionsScoreImport",
                        "ImportEta",
                        &[("eta", &format_eta(eta_secs))],
                    ),
                );
            }
        }
        text
    } else {
        String::new()
    };

    let header = if score_import.done {
        tr("OptionsScoreImport", "ImportComplete")
    } else {
        tr("OptionsScoreImport", "ImportingScores")
    };
    let line2 = format!(
        "{} \u{2022} {} \u{2022} {}",
        score_import.endpoint.display_name(),
        score_import.profile_name,
        score_import.pack_label,
    );
    let line3 = if score_import.done {
        score_import.done_message.clone()
    } else {
        score_import.detail_line.clone()
    };
    let stats_line = format!(
        "found={}  missing={}  failed={}",
        score_import.imported_scores,
        score_import.missing_scores,
        score_import.failed_requests,
    );

    let fill = color::decorative_rgba(active_color_index);
    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_width() * 0.5;
    let bar_cy = screen_height() * 0.5 + 34.0;
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    let mut out: Vec<Actor> = Vec::with_capacity(8);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));
    out.push(act!(text:
        font("miso"):
        settext(header):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));
    out.push(act!(text:
        font("miso"):
        settext(line2):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 74.0):
        zoom(0.95):
        maxwidth(screen_width() * 0.9):
        horizalign(center):
        z(301)
    ));
    if !line3.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(line3):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 50.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    out.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [actors::SizeSpec::Px(bar_w), actors::SizeSpec::Px(bar_h)],
        background: None,
        z: 301,
        children: bar_children,
    });

    if show_speed_row {
        out.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(301)
        ));
    }
    out.push(act!(text:
        font("miso"):
        settext(stats_line):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy + 60.0):
        zoom(0.85):
        horizalign(center):
        z(301)
    ));
    if score_import.done {
        out.push(act!(text:
            font("miso"):
            settext(tr("OptionsScoreImport", "PressStartToDismiss")):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy + 92.0):
            zoom(0.9):
            horizalign(center):
            z(301)
        ));
    }
    out
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
    let idx = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
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
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
        .get_mut(SCORE_IMPORT_ROW_PROFILE_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .cursor_indices
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
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .cursor_indices
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
    if let Some(slot) = state.sub[SubmenuKind::SyncPacks]
        .choice_indices
        .get_mut(SYNC_PACK_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state.sub[SubmenuKind::SyncPacks]
        .cursor_indices
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
    let pack_idx = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
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
    let profile_idx = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
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
        state.sub[SubmenuKind::ScoreImport]
            .choice_indices
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
        "{} score import starting for '{}' (pack: {}, only_missing_gs={}). {}",
        endpoint.display_name(),
        profile_name,
        pack_label,
        if only_missing_gs_scores { "yes" } else { "no" },
        match endpoint {
            scores::ScoreImportEndpoint::ArrowCloud =>
                "Bulk-imported per pack at 3 requests/sec (up to 1000 charts per request).",
            _ => "Hard-limited to 3 requests/sec. For many charts this can take more than one hour.",
        }
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

pub(super) fn poll_score_import_ui(score_import: &mut ScoreImportUiState, dt: f32) {
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

    // Ease the displayed progress toward the latest integer target. On `done`
    // we snap so the bar fills completely and the final speed readout matches
    // the summary's rate exactly.
    let target = score_import.processed_charts as f32;
    if score_import.done {
        score_import.displayed_done = target;
    } else if dt > 0.0 && SCORE_IMPORT_PROGRESS_TAU > 0.0 {
        let alpha = 1.0 - (-dt / SCORE_IMPORT_PROGRESS_TAU).exp();
        score_import.displayed_done += (target - score_import.displayed_done) * alpha;
    } else {
        score_import.displayed_done = target;
    }
    if score_import.displayed_done > target {
        score_import.displayed_done = target;
    }
}
