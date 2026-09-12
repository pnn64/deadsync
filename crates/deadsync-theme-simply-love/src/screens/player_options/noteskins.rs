use super::*;
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
            // Rows show quarter notes; a component icon shows only the left
            // column. Use the same layer/fallback selection as draw_noteskin_note.
            let cols = if part == 10 {
                1
            } else if matches!(skin.column_xs.len(), 5 | 10) {
                5
            } else {
                4
            };
            for col in 0..cols {
                let index = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
                let slots = layers
                    .get(index)
                    .or_else(|| skin.note_layers.get(index))
                    .map(AsRef::as_ref)
                    .or_else(|| skin.notes.get(index).map(std::slice::from_ref))
                    .unwrap_or_default();
                for slot in slots {
                    add(slot);
                }
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

pub(super) fn ready_preview<'a>(
    state: &'a State,
    name: &str,
    part: usize,
) -> Option<&'a Arc<Noteskin>> {
    if state
        .noteskin
        .ready_parts
        .get(name)
        .is_some_and(|parts| parts & (1 << part) == 0)
    {
        return None;
    }
    state.noteskin.cache.get(name)
}

pub(super) fn request_preview(state: &State, name: &str, part: usize) {
    request_preview_priority(state, name, part, NoteskinPreviewPriority::Visible);
}

pub(super) fn request_preview_priority(
    state: &State,
    name: &str,
    part: usize,
    priority: NoteskinPreviewPriority,
) {
    let mut requests = state.noteskin.requests.borrow_mut();
    if let Some(request) = requests
        .iter_mut()
        .find(|request| request.name.as_ref() == name)
    {
        request.parts |= 1 << part;
        request.priority = request.priority.min(priority);
    } else {
        requests.push(NoteskinPreviewRequest {
            name: Arc::from(name),
            parts: 1 << part,
            priority,
        });
    }
}
