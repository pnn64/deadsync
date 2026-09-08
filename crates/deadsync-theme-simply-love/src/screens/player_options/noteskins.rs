use super::*;
use deadsync_noteskin::Style;
use deadsync_profile as profile_data;

// Two selected mines plus the eight visible picker entries. One worker loads at
// a time; misses never do I/O on the menu thread. Only unused entries are replaced.
// Runtime ownership ends at screen exit. Twenty fixed texture keys cap retained
// preview pixels at 20 MiB (plus GPU copies), reused across screens. Completion
// queues at most two uploads through the normal upload budget. Failed loads log
// once per entry; frames only poll one channel and inspect ten cached names.
const MINE_PREVIEW_COUNT: usize = PLAYER_SLOTS + search::SEARCH_MAX_RESULTS;

#[derive(Default)]
pub(super) struct MinePreviews {
    entries: [Option<MinePreview>; MINE_PREVIEW_COUNT],
    loading: Option<(usize, std::sync::mpsc::Receiver<Result<LoadedMine, String>>)>,
}

pub(super) struct MinePreview {
    name: String,
    pub skin: Option<Arc<Noteskin>>,
    pub textures: Vec<(Arc<str>, Arc<str>)>,
}

struct LoadedMine {
    skin: Arc<Noteskin>,
    textures: Vec<(Arc<str>, image::RgbaImage, deadlib_render_core::SamplerDesc)>,
}

impl MinePreviews {
    pub fn get(&self, name: &str) -> Option<&MinePreview> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.name == name)
    }

    pub fn update(
        &mut self,
        wanted: &[&str],
        bundled: &HashMap<String, Arc<Noteskin>>,
        cols: usize,
    ) {
        if let Some((index, receiver)) = &self.loading {
            match receiver.try_recv() {
                Ok(result) => {
                    let entry = self.entries[*index]
                        .as_mut()
                        .expect("loading slot is reserved");
                    match result {
                        Ok(loaded) => {
                            for (layer, (source, image, sampler)) in
                                loaded.textures.into_iter().enumerate()
                            {
                                let key: Arc<str> = format!("__menu_mine_{index}_{layer}").into();
                                deadlib_assets::register_generated_texture(&key, image, sampler);
                                entry.textures.push((source, key));
                            }
                            entry.skin = Some(loaded.skin);
                        }
                        Err(error) => log::warn!("Cannot preview mine '{}': {error}", entry.name),
                    }
                    self.loading = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("Mine preview worker disconnected");
                    self.loading = None;
                }
            }
        }
        let Some(&name) = wanted
            .iter()
            .find(|&&name| !bundled.contains_key(name) && self.get(name).is_none())
        else {
            return;
        };
        let Some(index) = self.entries.iter().position(|entry| {
            entry
                .as_ref()
                .is_none_or(|entry| !wanted.contains(&entry.name.as_str()))
        }) else {
            return;
        };
        self.entries[index] = Some(MinePreview {
            name: name.to_string(),
            skin: None,
            textures: Vec::with_capacity(2),
        });
        let name = name.to_string();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("mine-preview".into())
            .spawn(move || {
                let _ = sender.send(load_mine_preview(&name, cols));
            }) {
            Ok(_) => self.loading = Some((index, receiver)),
            Err(error) => log::warn!("Cannot start mine preview worker: {error}"),
        }
    }
}

fn load_mine_preview(name: &str, cols: usize) -> Result<LoadedMine, String> {
    let skin = noteskin::load_itg_skin_cached(
        &Style {
            num_cols: cols,
            num_players: 1,
        },
        name,
    )?;
    let col = usize::from(skin.mines.len() > 1 || skin.mine_frames.len() > 1);
    let mut textures = Vec::with_capacity(2);
    for slot in [skin.mines.get(col), skin.mine_frames.get(col)]
        .into_iter()
        .flatten()
        .flatten()
    {
        let key = slot.texture_key_shared();
        if textures.iter().any(|(source, _, _)| *source == key) {
            continue;
        }
        let (image, sampler) =
            deadsync_assets::textures::decode_preview_texture(&key, slot.model.is_some())?;
        textures.push((key, image, sampler));
    }
    Ok(LoadedMine { skin, textures })
}

pub(super) fn load_noteskin_cached(skin: &str, cols_per_player: usize) -> Option<Arc<Noteskin>> {
    // Catalog warmup uses pack atlases; live mines load separately on a worker.
    if noteskin::is_pack_skin(skin) {
        return None;
    }
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
        for skin in [&options.arrow_noteskin, &options.lift_noteskin]
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
    if !prewarm_catalog {
        return NoteskinState {
            cache: HashMap::new(),
            previews: std::array::from_fn(|_| PlayerNoteskinPreviews::default()),
        };
    }

    let initial_names = preview_noteskin_names(noteskin_names.to_vec(), player_options);
    let mut cache = build_noteskin_cache(cols_per_player, &initial_names);
    let previews = std::array::from_fn(|i| {
        let profile_noteskin = &player_options[i].noteskin;
        PlayerNoteskinPreviews {
            base: cached_or_load_noteskin(&mut cache, profile_noteskin, cols_per_player),
            receptor: resolved_noteskin_override_preview(
                &mut cache,
                profile_noteskin,
                player_options[i].receptor_noteskin.as_ref(),
                cols_per_player,
            ),
            tap_explosion: resolved_tap_explosion_preview(
                &mut cache,
                profile_noteskin,
                player_options[i].tap_explosion_noteskin.as_ref(),
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
    if noteskin::is_pack_skin(skin.as_str()) {
        return None;
    }
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
    options: &profile_data::PlayerOptionsData,
    player_idx: usize,
    cols_per_player: usize,
) {
    let noteskin_setting = options.noteskin.clone();
    let receptor_noteskin_setting = options.receptor_noteskin.clone();
    let tap_explosion_noteskin_setting = options.tap_explosion_noteskin.clone();
    let previews = &mut noteskin.previews[player_idx];
    previews.base =
        cached_or_load_noteskin(&mut noteskin.cache, &noteskin_setting, cols_per_player);
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
