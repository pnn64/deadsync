use super::*;
use deadsync_noteskin::Style;
use deadsync_profile as profile_data;

pub(super) fn load_noteskin_cached(skin: &str, cols_per_player: usize) -> Option<Arc<Noteskin>> {
    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    noteskin::load_itg_skin_cached(&style, skin).ok()
}

pub(super) fn build_noteskin_override_choices(noteskin_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(noteskin_names.len() + 1);
    choices.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
    if noteskin_names.is_empty() {
        choices.push(profile_data::NoteSkin::DEFAULT_NAME.to_string());
    } else {
        choices.extend(noteskin_names.iter().cloned());
    }
    choices
}

pub(super) fn build_tap_explosion_noteskin_choices(noteskin_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(noteskin_names.len() + 2);
    choices.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
    choices.push(tr("PlayerOptions", "NoTapExplosionLabel").to_string());
    if noteskin_names.is_empty() {
        choices.push(profile_data::NoteSkin::DEFAULT_NAME.to_string());
    } else {
        choices.extend(noteskin_names.iter().cloned());
    }
    choices
}

pub(super) fn build_noteskin_cache(
    cols_per_player: usize,
    initial_names: &[String],
) -> HashMap<String, Arc<Noteskin>> {
    let mut cache = HashMap::with_capacity(initial_names.len());
    for name in initial_names {
        if let Some(noteskin) = load_noteskin_cached(name, cols_per_player) {
            cache.insert(name.clone(), noteskin);
        }
    }
    cache
}

pub(super) fn preview_noteskin_names(
    mut names: Vec<String>,
    profiles: &[profile_data::Profile],
) -> Vec<String> {
    if !names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(profile_data::NoteSkin::DEFAULT_NAME))
    {
        names.push(profile_data::NoteSkin::DEFAULT_NAME.to_string());
    }
    for profile in profiles {
        push_noteskin_name_once(&mut names, &profile.noteskin);
        if let Some(skin) = profile.mine_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
        if let Some(skin) = profile.receptor_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
        if let Some(skin) = profile.tap_explosion_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
    }
    names
}

pub(super) fn init_noteskin_state(
    cols_per_player: usize,
    noteskin_names: &[String],
    profiles: &[profile_data::Profile; PLAYER_SLOTS],
    prewarm_catalog: bool,
) -> NoteskinState {
    if !prewarm_catalog {
        return NoteskinState {
            cache: HashMap::new(),
            previews: std::array::from_fn(|_| PlayerNoteskinPreviews::default()),
        };
    }

    let initial_names = preview_noteskin_names(noteskin_names.to_vec(), profiles);
    let mut cache = build_noteskin_cache(cols_per_player, &initial_names);
    let previews = std::array::from_fn(|i| {
        let profile_noteskin = &profiles[i].noteskin;
        PlayerNoteskinPreviews {
            base: cached_or_load_noteskin(&mut cache, profile_noteskin, cols_per_player),
            mine: resolved_noteskin_override_preview(
                &mut cache,
                profile_noteskin,
                profiles[i].mine_noteskin.as_ref(),
                cols_per_player,
            ),
            receptor: resolved_noteskin_override_preview(
                &mut cache,
                profile_noteskin,
                profiles[i].receptor_noteskin.as_ref(),
                cols_per_player,
            ),
            tap_explosion: resolved_tap_explosion_preview(
                &mut cache,
                profile_noteskin,
                profiles[i].tap_explosion_noteskin.as_ref(),
                cols_per_player,
            ),
        }
    });
    NoteskinState { cache, previews }
}

pub(super) fn push_noteskin_name_once(names: &mut Vec<String>, skin: &profile_data::NoteSkin) {
    if skin.is_none_choice() {
        return;
    }
    let skin_name = skin.as_str().to_string();
    if !names.iter().any(|name| name == &skin_name) {
        names.push(skin_name);
    }
}

pub(super) fn cached_noteskin(
    cache: &HashMap<String, Arc<Noteskin>>,
    skin: &profile_data::NoteSkin,
) -> Option<Arc<Noteskin>> {
    cache.get(skin.as_str()).cloned()
}

pub(super) fn fallback_noteskin(cache: &HashMap<String, Arc<Noteskin>>) -> Option<Arc<Noteskin>> {
    cache
        .get(profile_data::NoteSkin::DEFAULT_NAME)
        .cloned()
        .or_else(|| cache.values().next().cloned())
}

pub(super) fn cached_or_load_noteskin(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &profile_data::NoteSkin,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(ns) = cached_noteskin(cache, skin) {
        return Some(ns);
    }

    if let Some(loaded) = load_noteskin_cached(skin.as_str(), cols_per_player) {
        cache.insert(skin.as_str().to_string(), loaded.clone());
        return Some(loaded);
    }

    if let Some(ns) = fallback_noteskin(cache) {
        return Some(ns);
    }

    if !skin
        .as_str()
        .eq_ignore_ascii_case(profile_data::NoteSkin::DEFAULT_NAME)
        && let Some(loaded) =
            load_noteskin_cached(profile_data::NoteSkin::DEFAULT_NAME, cols_per_player)
    {
        cache.insert(
            profile_data::NoteSkin::DEFAULT_NAME.to_string(),
            loaded.clone(),
        );
        return Some(loaded);
    }

    fallback_noteskin(cache)
}

pub(super) fn cached_or_load_noteskin_exact(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &profile_data::NoteSkin,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(ns) = cached_noteskin(cache, skin) {
        return Some(ns);
    }

    let loaded = load_noteskin_cached(skin.as_str(), cols_per_player)?;
    cache.insert(skin.as_str().to_string(), loaded.clone());
    Some(loaded)
}

pub(super) fn resolved_noteskin_override_preview(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    noteskin: &profile_data::NoteSkin,
    override_noteskin: Option<&profile_data::NoteSkin>,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(override_noteskin) = override_noteskin
        && let Some(ns) = cached_or_load_noteskin_exact(cache, override_noteskin, cols_per_player)
    {
        return Some(ns);
    }

    cached_or_load_noteskin(cache, noteskin, cols_per_player)
}

pub(super) fn resolved_tap_explosion_preview(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    noteskin: &profile_data::NoteSkin,
    tap_explosion_noteskin: Option<&profile_data::NoteSkin>,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if tap_explosion_noteskin.is_some_and(profile_data::NoteSkin::is_none_choice) {
        return None;
    }

    resolved_noteskin_override_preview(cache, noteskin, tap_explosion_noteskin, cols_per_player)
}

pub(super) fn sync_noteskin_previews_for_player(
    noteskin: &mut NoteskinState,
    profile: &profile_data::Profile,
    player_idx: usize,
    cols_per_player: usize,
) {
    let noteskin_setting = profile.noteskin.clone();
    let mine_noteskin_setting = profile.mine_noteskin.clone();
    let receptor_noteskin_setting = profile.receptor_noteskin.clone();
    let tap_explosion_noteskin_setting = profile.tap_explosion_noteskin.clone();
    let previews = &mut noteskin.previews[player_idx];
    previews.base =
        cached_or_load_noteskin(&mut noteskin.cache, &noteskin_setting, cols_per_player);
    previews.mine = resolved_noteskin_override_preview(
        &mut noteskin.cache,
        &noteskin_setting,
        mine_noteskin_setting.as_ref(),
        cols_per_player,
    );
    previews.receptor = resolved_noteskin_override_preview(
        &mut noteskin.cache,
        &noteskin_setting,
        receptor_noteskin_setting.as_ref(),
        cols_per_player,
    );
    previews.tap_explosion = resolved_tap_explosion_preview(
        &mut noteskin.cache,
        &noteskin_setting,
        tap_explosion_noteskin_setting.as_ref(),
        cols_per_player,
    );
}
