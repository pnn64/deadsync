use super::*;
use deadsync_noteskin::Style;
use deadsync_profile as profile_data;

// Screen-owned component cache shared by every noteskin provider. Entry and
// neighboring-choice warmup use the ordinary runtime loader on one worker.
// Forty entries bound retained runtimes; one pending component owns decoded
// images, capped at 128 MiB. Source textures keep their native pixels and keys:
// the normal asset upload queue owns them after handoff, reuses shared textures,
// and retains GPU resources for the session, bounded by the installed asset set.
// Two images / 8 MiB are submitted per frame (one oversized image may progress).
// No file access or decoding occurs on a menu frame. Only unwanted entries are
// replaced; runtime Arcs drop at replacement/screen exit. Failures log once per
// entry. A cold preview stays empty until its actual textures have uploaded.
const PREVIEW_COUNT: usize = 40;
const PREVIEW_BYTES: usize = 128 * 1024 * 1024;

pub(super) struct SkinPreviews {
    entries: [Option<SkinPreview>; PREVIEW_COUNT],
    loading: Option<(
        usize,
        std::sync::mpsc::Receiver<Result<LoadedPreview, String>>,
    )>,
    pending: Option<(usize, LoadedPreview)>,
}

impl Default for SkinPreviews {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            loading: None,
            pending: None,
        }
    }
}

pub(super) struct SkinPreview {
    name: String,
    part: usize,
    pub skin: Option<Arc<Noteskin>>,
    pub textures: Vec<Arc<str>>,
    pub ready: bool,
    requested: bool,
}

struct LoadedPreview {
    skin: Arc<Noteskin>,
    textures: Vec<(Arc<str>, image::RgbaImage, deadlib_render_core::SamplerDesc)>,
}

impl SkinPreviews {
    pub fn get(&self, name: &str, part: usize) -> Option<&SkinPreview> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.name == name && entry.part == part)
    }

    // The stages are kept together to make reservation, upload, and readiness
    // ordering explicit: queued textures must never make a component drawable.
    pub fn update(
        &mut self,
        wanted: &[(&str, usize)],
        cached: &HashMap<String, Arc<Noteskin>>,
        cols: usize,
        assets: &mut AssetManager,
    ) {
        if let Some((index, receiver)) = &self.loading {
            match receiver.try_recv() {
                Ok(Ok(loaded)) => {
                    let entry = self.entries[*index]
                        .as_mut()
                        .expect("worker slot is reserved");
                    entry.textures = preview_textures(&loaded.skin, entry.part)
                        .into_iter()
                        .map(|(key, _)| key)
                        .collect();
                    entry.skin = Some(Arc::clone(&loaded.skin));
                    self.pending = Some((*index, loaded));
                    self.loading = None;
                }
                Ok(Err(error)) => {
                    log::warn!("Cannot load noteskin preview: {error}");
                    self.loading = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("Noteskin preview worker disconnected");
                    self.loading = None;
                }
            }
        }
        if let Some((index, loaded)) = &mut self.pending {
            let entry = self.entries[*index]
                .as_ref()
                .expect("upload slot is reserved");
            if wanted.contains(&(entry.name.as_str(), entry.part)) {
                let mut bytes = 0;
                for _ in 0..2 {
                    let Some((_, image, _)) = loaded.textures.last() else {
                        break;
                    };
                    let size = image.as_raw().len();
                    if bytes > 0 && bytes + size > 8 * 1024 * 1024 {
                        break;
                    }
                    let (key, image, sampler) = loaded.textures.pop().expect("image exists");
                    if !assets.has_uploaded_texture_key(&key)
                        && !assets.has_pending_texture_upload(&key)
                    {
                        bytes += size;
                        assets.queue_texture_upload_with_sampler(key.to_string(), image, sampler);
                    }
                }
                if loaded.textures.is_empty() {
                    self.pending = None;
                }
            } else {
                self.entries[*index] = None;
                self.pending = None;
            }
        }
        for &(name, part) in wanted {
            if self.get(name, part).is_some() {
                continue;
            }
            let Some(index) = self.entries.iter().enumerate().position(|(index, entry)| {
                !self
                    .loading
                    .as_ref()
                    .is_some_and(|(active, _)| *active == index)
                    && !self
                        .pending
                        .as_ref()
                        .is_some_and(|(active, _)| *active == index)
                    && entry
                        .as_ref()
                        .is_none_or(|entry| !wanted.contains(&(entry.name.as_str(), entry.part)))
            }) else {
                break;
            };
            let skin = cached.get(name).cloned().or_else(|| {
                self.entries
                    .iter()
                    .flatten()
                    .find(|entry| entry.name == name)
                    .and_then(|entry| entry.skin.clone())
            });
            let textures = skin
                .as_ref()
                .map(|skin| {
                    preview_textures(skin, part)
                        .into_iter()
                        .map(|(key, _)| key)
                        .collect()
                })
                .unwrap_or_default();
            self.entries[index] = Some(SkinPreview {
                name: name.to_string(),
                part,
                skin,
                textures,
                ready: false,
                requested: false,
            });
        }
        for entry in self.entries.iter_mut().flatten() {
            if !entry.ready
                && entry.skin.is_some()
                && entry
                    .textures
                    .iter()
                    .all(|key| assets.has_uploaded_texture_key(key))
            {
                entry.ready = true;
            }
        }
        if self.loading.is_some() || self.pending.is_some() {
            return;
        }
        let Some(index) = wanted.iter().find_map(|&(name, part)| {
            self.entries.iter().position(|entry| {
                entry.as_ref().is_some_and(|entry| {
                    entry.name == name && entry.part == part && !entry.ready && !entry.requested
                })
            })
        }) else {
            return;
        };
        let entry = self.entries[index].as_mut().expect("entry was found");
        entry.requested = true;
        let name = entry.name.clone();
        let part = entry.part;
        let skin = entry.skin.clone();
        let uploaded: Vec<_> = entry
            .textures
            .iter()
            .filter(|key| {
                assets.has_uploaded_texture_key(key) || assets.has_pending_texture_upload(key)
            })
            .cloned()
            .collect();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("skin-preview".into())
            .spawn(move || {
                let result = load_skin_preview(&name, part, cols, skin, &uploaded)
                    .map_err(|error| format!("{name} (part {part}): {error}"));
                let _ = sender.send(result);
            }) {
            Ok(_) => self.loading = Some((index, receiver)),
            Err(error) => log::warn!("Cannot start noteskin preview worker: {error}"),
        }
    }
}

fn preview_textures(skin: &Noteskin, part: usize) -> Vec<(Arc<str>, bool)> {
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

fn load_skin_preview(
    name: &str,
    part: usize,
    cols: usize,
    cached: Option<Arc<Noteskin>>,
    uploaded: &[Arc<str>],
) -> Result<LoadedPreview, String> {
    let skin = match cached {
        Some(skin) => skin,
        None => noteskin::load_itg_skin_cached(
            &Style {
                num_cols: cols,
                num_players: 1,
            },
            name,
        )?,
    };
    let mut textures = Vec::new();
    let mut bytes = 0;
    for (key, model) in preview_textures(&skin, part) {
        if uploaded.contains(&key) {
            continue;
        }
        let (image, sampler) = deadsync_assets::textures::decode_texture_key(&key, model)?;
        bytes += image.as_raw().len();
        if bytes > PREVIEW_BYTES {
            return Err("component exceeds 128 MiB of decoded textures".into());
        }
        textures.push((key, image, sampler));
    }
    Ok(LoadedPreview { skin, textures })
}

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
    NoteskinState {
        cache,
        components: SkinPreviews::default(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uploads_wait_for_residency_and_keep_native_keys() {
        crate::tests::init_paths();
        let skin = noteskin::load_itg_skin_cached(
            &Style {
                num_cols: 4,
                num_players: 1,
            },
            "cel",
        )
        .unwrap();
        let keys: Vec<Arc<str>> = (0..5)
            .map(|i| format!("fixture-native-{i}").into())
            .collect();
        let mut previews = SkinPreviews::default();
        previews.entries[0] = Some(SkinPreview {
            name: "fixture".into(),
            part: 6,
            skin: Some(Arc::clone(&skin)),
            textures: keys.clone(),
            ready: false,
            requested: true,
        });
        previews.pending = Some((
            0,
            LoadedPreview {
                skin,
                textures: keys
                    .iter()
                    .map(|key| {
                        (
                            Arc::clone(key),
                            image::RgbaImage::new(1024, 1024),
                            deadlib_render_core::SamplerDesc::default(),
                        )
                    })
                    .collect(),
            },
        ));
        let mut assets = AssetManager::new();
        for count in [2, 4, 5] {
            previews.update(&[("fixture", 6)], &HashMap::new(), 4, &mut assets);
            assert_eq!(
                keys.iter()
                    .filter(|key| assets.has_pending_texture_upload(key))
                    .count(),
                count
            );
            assert!(
                !previews.get("fixture", 6).unwrap().ready,
                "queued images are not resident textures"
            );
            assert!(previews.get("fixture", 0).is_none());
        }
        assert!(previews.pending.is_none());
    }
}
