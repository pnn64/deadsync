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
    pub(super) pack_groups: Vec<String>,
    pub(super) pack_label: String,
    pub(super) only_missing_gs_scores: bool,
}

#[derive(Debug)]
pub(super) enum ScoreImportMsg {
    Progress(scores::ScoreImportProgress),
    Done(Result<scores::ScoreBulkImportSummary, String>),
}

#[derive(Clone, Debug, Default)]
pub(super) struct ScoreImportPackOption {
    pub(super) group: String,
    pub(super) display: String,
}

#[derive(Debug, Default)]
pub(super) struct ScoreImportPackPicker {
    pub(super) cursor: usize,
    pub(super) scroll_offset: usize,
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

const PACK_PICKER_VISIBLE_ROWS: usize = 12;
const PACK_PICKER_ROW_H: f32 = 22.0;

pub(super) fn build_score_import_pack_picker_actors(
    state: &State,
    active_color_index: i32,
) -> Vec<Actor> {
    let Some(picker) = state.score_import_pack_picker.as_ref() else {
        return Vec::new();
    };
    let options = &state.score_import_pack_options;
    let total = options.len();
    let selected_count = state.score_import_pack_selected.len();
    let cursor = picker.cursor.min(total.saturating_sub(1).max(0));
    let scroll = picker.scroll_offset.min(total.saturating_sub(1).max(0));
    let panel_w = widescale(420.0, 600.0);
    let panel_h = widescale(360.0, 420.0);
    let panel_cx = screen_width() * 0.5;
    let panel_cy = screen_height() * 0.5;
    let panel_top = panel_cy - panel_h * 0.5;
    let fill = color::decorative_rgba(active_color_index);

    let title_text = tr("OptionsScoreImport", "PackPickerTitle").to_string();
    let summary_text = if total == 0 {
        tr("OptionsScoreImport", "PackPickerNoPacks").to_string()
    } else if selected_count == 0 {
        format!(
            "{} ({})",
            tr("OptionsScoreImport", "AllPacks"),
            tr_fmt(
                "OptionsScoreImport",
                "PackPickerSelectedCount",
                &[("count", "0"), ("total", &total.to_string())]
            )
        )
    } else {
        tr_fmt(
            "OptionsScoreImport",
            "PackPickerSelectedCount",
            &[
                ("count", &selected_count.to_string()),
                ("total", &total.to_string()),
            ],
        )
        .to_string()
    };
    let hint_text = tr("OptionsScoreImport", "PackPickerHint").to_string();

    let mut out: Vec<Actor> = Vec::with_capacity(8 + PACK_PICKER_VISIBLE_ROWS);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.7):
        z(310)
    ));
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(panel_cx, panel_cy):
        zoomto(panel_w, panel_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(311)
    ));
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(panel_cx, panel_cy):
        zoomto(panel_w - 4.0, panel_h - 4.0):
        diffuse(0.04, 0.06, 0.09, 0.97):
        z(312)
    ));
    out.push(act!(text:
        font("miso"):
        settext(title_text):
        align(0.5, 0.0):
        xy(panel_cx, panel_top + 14.0):
        zoom(1.05):
        horizalign(center):
        diffuse(fill[0], fill[1], fill[2], 1.0):
        z(313)
    ));
    out.push(act!(text:
        font("miso"):
        settext(summary_text):
        align(0.5, 0.0):
        xy(panel_cx, panel_top + 38.0):
        zoom(0.9):
        maxwidth(panel_w - 30.0):
        horizalign(center):
        z(313)
    ));

    let list_top = panel_top + 64.0;
    let list_left = panel_cx - panel_w * 0.5 + 18.0;
    let list_right = panel_cx + panel_w * 0.5 - 18.0;
    let visible = PACK_PICKER_VISIBLE_ROWS.min(total);
    for i in 0..visible {
        let opt_idx = scroll + i;
        if opt_idx >= total {
            break;
        }
        let opt = &options[opt_idx];
        let row_y = list_top + (i as f32) * PACK_PICKER_ROW_H;
        let is_cursor = opt_idx == cursor;
        if is_cursor {
            out.push(act!(quad:
                align(0.0, 0.5):
                xy(list_left - 4.0, row_y + PACK_PICKER_ROW_H * 0.5):
                zoomto(list_right - list_left + 8.0, PACK_PICKER_ROW_H - 2.0):
                diffuse(fill[0] * 0.4, fill[1] * 0.4, fill[2] * 0.4, 0.85):
                z(313)
            ));
        }
        let checked = state
            .score_import_pack_selected
            .iter()
            .any(|key| key.eq_ignore_ascii_case(&opt.group));
        let mark = if checked { "[x]" } else { "[ ]" };
        let label = format!("{mark}  {}", opt.display);
        out.push(act!(text:
            font("miso"):
            settext(label):
            align(0.0, 0.5):
            xy(list_left, row_y + PACK_PICKER_ROW_H * 0.5):
            zoom(0.85):
            maxwidth(list_right - list_left):
            horizalign(left):
            z(314)
        ));
    }
    // Scroll indicators.
    if scroll > 0 {
        out.push(act!(text:
            font("miso"):
            settext("\u{25B2}"):
            align(0.5, 0.5):
            xy(panel_cx, list_top - 8.0):
            zoom(0.7):
            horizalign(center):
            z(314)
        ));
    }
    if scroll + visible < total {
        out.push(act!(text:
            font("miso"):
            settext("\u{25BC}"):
            align(0.5, 0.5):
            xy(panel_cx, list_top + (visible as f32) * PACK_PICKER_ROW_H + 6.0):
            zoom(0.7):
            horizalign(center):
            z(314)
        ));
    }

    out.push(act!(text:
        font("miso"):
        settext(hint_text):
        align(0.5, 1.0):
        xy(panel_cx, panel_cy + panel_h * 0.5 - 14.0):
        zoom(0.8):
        maxwidth(panel_w - 30.0):
        horizalign(center):
        z(313)
    ));
    out
}

pub(super) fn pack_picker_step(state: &mut State, delta: i32) {
    let Some(picker) = state.score_import_pack_picker.as_mut() else {
        return;
    };
    let total = state.score_import_pack_options.len();
    if total == 0 {
        picker.cursor = 0;
        picker.scroll_offset = 0;
        return;
    }
    let cur = picker.cursor as i32;
    let next = (cur + delta).rem_euclid(total as i32) as usize;
    picker.cursor = next;
    if next < picker.scroll_offset {
        picker.scroll_offset = next;
    } else if next >= picker.scroll_offset + PACK_PICKER_VISIBLE_ROWS {
        picker.scroll_offset = next + 1 - PACK_PICKER_VISIBLE_ROWS;
    }
}

pub(super) fn pack_picker_page(state: &mut State, direction: i32) {
    let Some(picker) = state.score_import_pack_picker.as_mut() else {
        return;
    };
    let total = state.score_import_pack_options.len();
    if total == 0 {
        return;
    }
    let last = total - 1;
    let cur = picker.cursor as i32;
    let next = (cur + direction.signum() * PACK_PICKER_VISIBLE_ROWS as i32)
        .clamp(0, last as i32) as usize;
    picker.cursor = next;
    if next < picker.scroll_offset {
        picker.scroll_offset = next;
    } else if next >= picker.scroll_offset + PACK_PICKER_VISIBLE_ROWS {
        picker.scroll_offset = next + 1 - PACK_PICKER_VISIBLE_ROWS;
    }
}

pub(super) fn pack_picker_toggle_current(state: &mut State) -> bool {
    let Some(picker) = state.score_import_pack_picker.as_ref() else {
        return false;
    };
    let idx = picker.cursor;
    toggle_score_import_pack_at(state, idx)
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
    // Legacy helper retained for sync_pack flow which still uses the
    // single-choice cycler. Score import has its own picker model.
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

pub(super) fn score_import_pack_options() -> Vec<ScoreImportPackOption> {
    let cache = crate::game::song::get_song_cache();
    let mut packs: Vec<ScoreImportPackOption> = Vec::with_capacity(cache.len());
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
        let display = if pack.name.trim().is_empty() {
            group_name.to_string()
        } else {
            pack.name.trim().to_string()
        };
        packs.push(ScoreImportPackOption {
            group: group_name.to_string(),
            display,
        });
    }

    packs.sort_by(|a, b| {
        a.display
            .to_ascii_lowercase()
            .cmp(&b.display.to_ascii_lowercase())
            .then_with(|| a.group.cmp(&b.group))
    });
    packs
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
    let new_options = score_import_pack_options();
    // Prune selection set to only canonical group names that still exist.
    let new_groups_lc: HashSet<String> = new_options
        .iter()
        .map(|opt| opt.group.to_ascii_lowercase())
        .collect();
    state
        .score_import_pack_selected
        .retain(|key| new_groups_lc.contains(&key.to_ascii_lowercase()));
    // Normalize "all selected" back to empty so we have one canonical "all".
    if !state.score_import_pack_selected.is_empty()
        && state.score_import_pack_selected.len() >= new_options.len()
    {
        state.score_import_pack_selected.clear();
    }
    state.score_import_pack_options = new_options;
    let pack_count = state.score_import_pack_options.len();
    if let Some(picker) = state.score_import_pack_picker.as_mut() {
        let max_idx = pack_count.saturating_sub(1);
        picker.cursor = picker.cursor.min(max_idx);
    }
    // The Pack row's choices list is now a single dynamic summary entry,
    // so always pin its choice index to 0.
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .choice_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = 0;
    }
    if let Some(slot) = state.sub[SubmenuKind::ScoreImport]
        .cursor_indices
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = 0;
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

pub(super) fn score_import_pack_summary(state: &State) -> String {
    let total = state.score_import_pack_options.len();
    let selected = state.score_import_pack_selected.len();
    if selected == 0 || (total > 0 && selected >= total) {
        return tr("OptionsScoreImport", "AllPacks").to_string();
    }
    if selected == 1 {
        // Find the single selected option's display name.
        if let Some(only_key) = state.score_import_pack_selected.iter().next() {
            let only_lc = only_key.to_ascii_lowercase();
            if let Some(opt) = state
                .score_import_pack_options
                .iter()
                .find(|opt| opt.group.to_ascii_lowercase() == only_lc)
            {
                return opt.display.clone();
            }
            return only_key.clone();
        }
    }
    tr_fmt(
        "OptionsScoreImport",
        "PackSummaryMulti",
        &[("count", &selected.to_string())],
    )
    .to_string()
}

pub(super) fn selected_score_import_pack_groups(state: &State) -> Vec<String> {
    if state.score_import_pack_selected.is_empty() {
        return Vec::new();
    }
    let selected_lc: HashSet<String> = state
        .score_import_pack_selected
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    state
        .score_import_pack_options
        .iter()
        .filter(|opt| selected_lc.contains(&opt.group.to_ascii_lowercase()))
        .map(|opt| opt.group.clone())
        .collect()
}

pub(super) fn open_score_import_pack_picker(state: &mut State) {
    if state.score_import_pack_picker.is_some() {
        return;
    }
    clear_navigation_holds(state);
    state.score_import_pack_picker = Some(ScoreImportPackPicker::default());
}

pub(super) fn close_score_import_pack_picker(state: &mut State) {
    if state.score_import_pack_picker.take().is_none() {
        return;
    }
    // Normalize: if every pack ended up selected, clear so "all" stays canonical.
    let total = state.score_import_pack_options.len();
    if total > 0 && state.score_import_pack_selected.len() >= total {
        state.score_import_pack_selected.clear();
    }
    clear_navigation_holds(state);
    // Pack row summary may have changed width; invalidate layout cache.
    state.submenu_layout_cache_kind.set(None);
    state.submenu_row_layout_cache.borrow_mut().clear();
}

pub(super) fn toggle_score_import_pack_at(state: &mut State, idx: usize) -> bool {
    let Some(opt) = state.score_import_pack_options.get(idx) else {
        return false;
    };
    let key = opt.group.clone();
    if !state.score_import_pack_selected.remove(&key) {
        state.score_import_pack_selected.insert(key);
    }
    true
}

pub(super) fn toggle_all_score_import_packs(state: &mut State) {
    let total = state.score_import_pack_options.len();
    if total == 0 {
        return;
    }
    if state.score_import_pack_selected.is_empty()
        || state.score_import_pack_selected.len() < total
    {
        // Select all (canonicalized to empty after picker closes via close_*).
        state.score_import_pack_selected = state
            .score_import_pack_options
            .iter()
            .map(|opt| opt.group.clone())
            .collect();
    } else {
        state.score_import_pack_selected.clear();
    }
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
    let pack_groups = selected_score_import_pack_groups(state);
    let pack_label = score_import_pack_summary(state);
    let only_missing_gs_scores = score_import_only_missing_gs_scores(state);
    Some(ScoreImportSelection {
        endpoint,
        profile: profile_cfg,
        pack_groups,
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
    let pack_groups = selection.pack_groups.clone();
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
            pack_groups,
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