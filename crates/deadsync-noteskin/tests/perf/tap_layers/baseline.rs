// Frozen from 7754dc3f7 (0.5.1163); only imports/visibility/formatting differ.
use super::super::*;

pub(super) fn emit_tap_note_column<T: Clone>(
    mut layers: Vec<T>,
    quantizations: usize,
    layer_info: impl FnMut(&T) -> (bool, [f32; 2]),
    notes: &mut Vec<T>,
    note_layers: &mut Vec<Arc<[T]>>,
) -> Option<PreparedTapNoteColumn<T>> {
    sort_tap_note_layers(&mut layers, layer_info);
    let first = layers.first()?;
    let shared_layers = shared_tap_note_layers(&layers);
    notes.extend((0..quantizations).map(|_| first.clone()));
    note_layers.extend((0..quantizations).map(|_| Arc::clone(&shared_layers)));
    Some(PreparedTapNoteColumn { shared_layers })
}
