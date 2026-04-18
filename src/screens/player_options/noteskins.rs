use super::*;

#[inline(always)]
pub(super) fn noteskin_cols_per_player(play_style: crate::game::profile::PlayStyle) -> usize {
    match play_style {
        crate::game::profile::PlayStyle::Double => 8,
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Versus => 4,
    }
}

pub(super) fn load_noteskin_cached(skin: &str, cols_per_player: usize) -> Option<Arc<Noteskin>> {
    let style = noteskin::Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    noteskin::load_itg_skin_cached(&style, skin).ok()
}

pub(super) fn discover_noteskin_names() -> Vec<String> {
    noteskin::discover_itg_skins("dance")
}

pub(super) fn build_noteskin_override_choices(noteskin_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(noteskin_names.len() + 1);
    choices.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
    if noteskin_names.is_empty() {
        choices.push(crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
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
        choices.push(crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
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

pub(super) fn push_noteskin_name_once(
    names: &mut Vec<String>,
    skin: &crate::game::profile::NoteSkin,
) {
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
    skin: &crate::game::profile::NoteSkin,
) -> Option<Arc<Noteskin>> {
    cache.get(skin.as_str()).cloned()
}

pub(super) fn fallback_noteskin(cache: &HashMap<String, Arc<Noteskin>>) -> Option<Arc<Noteskin>> {
    cache
        .get(crate::game::profile::NoteSkin::DEFAULT_NAME)
        .cloned()
        .or_else(|| cache.values().next().cloned())
}

pub(super) fn cached_or_load_noteskin(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &crate::game::profile::NoteSkin,
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
        .eq_ignore_ascii_case(crate::game::profile::NoteSkin::DEFAULT_NAME)
        && let Some(loaded) = load_noteskin_cached(
            crate::game::profile::NoteSkin::DEFAULT_NAME,
            cols_per_player,
        )
    {
        cache.insert(
            crate::game::profile::NoteSkin::DEFAULT_NAME.to_string(),
            loaded.clone(),
        );
        return Some(loaded);
    }

    fallback_noteskin(cache)
}

pub(super) fn cached_or_load_noteskin_exact(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &crate::game::profile::NoteSkin,
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
    noteskin: &crate::game::profile::NoteSkin,
    override_noteskin: Option<&crate::game::profile::NoteSkin>,
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
    noteskin: &crate::game::profile::NoteSkin,
    tap_explosion_noteskin: Option<&crate::game::profile::NoteSkin>,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if tap_explosion_noteskin.is_some_and(crate::game::profile::NoteSkin::is_none_choice) {
        return None;
    }

    resolved_noteskin_override_preview(cache, noteskin, tap_explosion_noteskin, cols_per_player)
}

pub(super) fn sync_noteskin_previews_for_player(state: &mut State, player_idx: usize) {
    let cols_per_player = noteskin_cols_per_player(crate::game::profile::get_session_play_style());
    let noteskin_setting = state.player_profiles[player_idx].noteskin.clone();
    let mine_noteskin_setting = state.player_profiles[player_idx].mine_noteskin.clone();
    let receptor_noteskin_setting = state.player_profiles[player_idx].receptor_noteskin.clone();
    let tap_explosion_noteskin_setting = state.player_profiles[player_idx]
        .tap_explosion_noteskin
        .clone();
    state.noteskin[player_idx] = cached_or_load_noteskin(
        &mut state.noteskin_cache,
        &noteskin_setting,
        cols_per_player,
    );
    state.mine_noteskin[player_idx] = resolved_noteskin_override_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        mine_noteskin_setting.as_ref(),
        cols_per_player,
    );
    state.receptor_noteskin[player_idx] = resolved_noteskin_override_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        receptor_noteskin_setting.as_ref(),
        cols_per_player,
    );
    state.tap_explosion_noteskin[player_idx] = resolved_tap_explosion_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        tap_explosion_noteskin_setting.as_ref(),
        cols_per_player,
    );
}
