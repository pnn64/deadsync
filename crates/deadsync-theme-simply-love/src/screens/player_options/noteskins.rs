use super::*;
use deadsync_profile as profile_data;

pub(super) fn preview_note_slots(
    skin: &Noteskin,
    part: NoteAnimPart,
    index: usize,
) -> &[SpriteSlot] {
    let layers = if part == NoteAnimPart::Lift {
        skin.lift_note_layers.get(index)
    } else {
        None
    };
    layers
        .or_else(|| skin.note_layers.get(index))
        .map(AsRef::as_ref)
        .or_else(|| skin.notes.get(index).map(std::slice::from_ref))
        .unwrap_or_default()
}

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
            let note_part = if part == 10 {
                NoteAnimPart::Lift
            } else {
                NoteAnimPart::Tap
            };
            // The row samples different quants. A lift icon uses only the first
            // sample; share the renderer's selection so readiness covers every draw.
            let arrows = render::preview_arrows(skin.column_xs.len());
            let count = if part == 10 { 1 } else { arrows.len() };
            for &(col, quant, _) in &arrows[..count] {
                let index = col * NUM_QUANTIZATIONS + quant as usize;
                for slot in preview_note_slots(skin, note_part, index) {
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
            for overlays in &skin.receptor_overlays {
                for overlay in overlays.iter() {
                    add(&overlay.slot);
                }
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
            let col = usize::from(skin.mine_layers.len() > 1);
            if let Some(layers) = skin.mine_layers.get(col) {
                for slot in layers.iter() {
                    add(slot);
                }
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
