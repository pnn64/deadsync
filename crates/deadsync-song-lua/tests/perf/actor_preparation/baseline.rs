// Frozen from 0.5.1698 for behavioral and performance comparisons.
use super::*;

pub fn overlay_actor_tree_has_visual<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    root_index: usize,
) -> bool {
    overlay_actor_has_visual(&overlays[root_index].actor)
        || overlay_descendants_by_parent(overlays.len(), root_index, |index| {
            overlays
                .get(index)
                .and_then(|overlay| overlay.actor.parent_index)
        })
        .into_iter()
        .any(|index| overlay_actor_has_visual(&overlays[index].actor))
}
