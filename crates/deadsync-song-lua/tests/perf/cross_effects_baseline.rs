// Frozen from 619d10d4a340b2d5aa9cebe90ea38dc080059338; only test visibility/formatting differs.

use super::*;

pub(super) fn cross_actor_effects(
    capture: &SongLuaFunctionActionCapture,
    source_index: usize,
) -> Vec<(usize, Vec<SongLuaOverlayCommandBlock>, Option<f32>)> {
    let mut effects = capture
        .overlay_aux
        .iter()
        .filter(|(index, _)| *index != source_index)
        .map(|(index, aux)| (*index, (Vec::new(), Some(*aux))))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (index, blocks) in &capture.overlay_blocks {
        if *index != source_index {
            effects.entry(*index).or_default().0 = blocks.clone();
        }
    }
    effects
        .into_iter()
        .map(|(index, (blocks, aux))| (index, blocks, aux))
        .collect()
}
