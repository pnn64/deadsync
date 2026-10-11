use super::*;

pub fn preview_skin_models(skin: &Noteskin) -> PreviewSkinModels {
    let mut ids: Vec<u64> = Vec::new();
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot| {
        if slot.model.is_some() && !ids.contains(&slot.stable_id()) {
            ids.push(slot.stable_id());
        }
    });
    // Sized to the slots exactly, so registering them never grows it.
    let mut cache = ModelMeshCache::with_capacity(ids.len());
    noteskin_draw::for_each_field_slot(skin, SKIN_COLS, |slot| {
        if slot.model.is_some() {
            let _ = cache.prewarm_slot(slot);
        }
    });
    cache.seal();
    cache.reset_stats();
    PreviewSkinModels(cache)
}
