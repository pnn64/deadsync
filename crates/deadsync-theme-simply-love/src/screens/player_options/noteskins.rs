use super::*;
use deadsync_noteskin::Style;
use deadsync_profile as profile_data;

pub(super) fn preview_textures(skin: &Noteskin, part: usize) -> Vec<(Arc<str>, bool)> {
    let mut textures: Vec<(Arc<str>, bool)> = Vec::new();
    let mut add = |slot: &SpriteSlot| {
        let key = slot.texture_key_shared();
        if let Some((_, model)) = textures.iter_mut().find(|(source, _)| *source == key) {
            *model |= slot.model.is_some();
        } else {
            textures.push((key, slot.model.is_some()));
        }
    };
    match part {
        0 | 10 => {
            let layers = if part == 10 {
                &skin.lift_note_layers
            } else {
                &skin.note_layers
            };
            for slot in layers.iter().flat_map(|layers| layers.iter()) {
                add(slot);
            }
        }
        1 => {
            for slot in &skin.receptor_off {
                add(slot);
            }
            for slot in skin
                .receptor_glow
                .iter()
                .chain(&skin.receptor_idle_glow_layers)
                .flatten()
            {
                add(slot);
            }
        }
        2 => {
            if let Some(slot) = &skin.hold.body_active {
                add(slot);
            }
        }
        3 => {
            if let Some(slot) = &skin.hold.body_inactive {
                add(slot);
            }
        }
        4 => {
            if let Some(slot) = &skin.roll.body_active {
                add(slot);
            }
        }
        5 => {
            if let Some(slot) = &skin.roll.body_inactive {
                add(slot);
            }
        }
        6 => {
            if let Some(explosion) = skin
                .tap_explosions
                .get("W1")
                .or_else(|| skin.tap_explosions.values().next())
            {
                for layer in explosion.layers.iter() {
                    add(&layer.slot);
                }
            }
        }
        7 => {
            if let Some(slot) = &skin.hold.explosion {
                add(slot);
            }
        }
        8 => {
            let col = usize::from(skin.mines.len() > 1 || skin.mine_frames.len() > 1);
            if let Some(slot) = skin.mines.get(col).and_then(Option::as_ref) {
                add(slot);
            }
            if let Some(slot) = skin.mine_frames.get(col).and_then(Option::as_ref) {
                add(slot);
            }
        }
        _ => {}
    }
    textures
}

pub(super) fn load_noteskin_cached(skin: &str, cols_per_player: usize) -> Option<Arc<Noteskin>> {
    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    match noteskin::load_itg_skin_cached(&style, skin) {
        Ok(skin) => Some(skin),
        Err(error) => {
            log::warn!("Cannot load noteskin preview '{skin}': {error}");
            None
        }
    }
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
    if initial_names.is_empty() {
        return cache;
    }
    // Parsing stays on bounded workers while the transition owns the load.
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    std::thread::scope(|scope| {
        let handles: Vec<_> = initial_names
            .chunks(initial_names.len().div_ceil(workers))
            .map(|names| {
                scope.spawn(move || {
                    names
                        .iter()
                        .filter_map(|name| {
                            load_noteskin_cached(name, cols_per_player)
                                .map(|skin| (name.clone(), skin))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            cache.extend(handle.join().expect("noteskin loader worker panicked"));
        }
    });
    cache
}

pub(super) fn preview_noteskin_names(
    mut names: Vec<String>,
    player_options: &[profile_data::PlayerOptionsData],
) -> Vec<String> {
    if !names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(profile_data::NoteSkin::DEFAULT_NAME))
    {
        names.push(profile_data::NoteSkin::DEFAULT_NAME.to_string());
    }
    for options in player_options {
        push_noteskin_name_once(&mut names, &options.noteskin);
        for skin in [
            &options.arrow_noteskin,
            &options.lift_noteskin,
            &options.hold_active_noteskin,
            &options.hold_inactive_noteskin,
            &options.roll_active_noteskin,
            &options.roll_inactive_noteskin,
            &options.hold_explosion_noteskin,
        ]
        .into_iter()
        .flatten()
        {
            push_noteskin_name_once(&mut names, skin);
        }
        if let Some(skin) = options.mine_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
        if let Some(skin) = options.receptor_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
        if let Some(skin) = options.tap_explosion_noteskin.as_ref() {
            push_noteskin_name_once(&mut names, skin);
        }
    }
    names
}

pub(super) fn init_noteskin_state(
    cols_per_player: usize,
    noteskin_names: &[String],
    player_options: &[profile_data::PlayerOptionsData; PLAYER_SLOTS],
    prewarm_catalog: bool,
) -> NoteskinState {
    let cache = if prewarm_catalog {
        let names = preview_noteskin_names(noteskin_names.to_vec(), player_options);
        build_noteskin_cache(cols_per_player, &names)
    } else {
        HashMap::new()
    };
    NoteskinState { cache }
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
