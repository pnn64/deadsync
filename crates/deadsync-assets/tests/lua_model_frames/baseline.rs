// Frozen from 7db83fdcf (0.5.1661).
use super::*;

pub(super) fn model_layer_from_slot_frame(
    slot: &crate::noteskin::SpriteSlot,
    frame_index: usize,
) -> Option<SongLuaOverlayModelLayer> {
    let model = slot.model.as_ref()?;
    if model.vertices.is_empty() {
        return None;
    }
    let uv_rect = slot.uv_for_frame_at(frame_index, 0.0);
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    let mut layer = SongLuaOverlayModelLayer::new(
        slot.texture_key_shared(),
        crate::noteskin::build_model_geometry(slot),
        model.size(),
        uv_scale,
        uv_offset,
        uv_tex_shift,
        slot.uv_velocity,
        slot.uv_cycle_seconds,
        song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
    );
    if let Some(texture) = &slot.model_additive {
        let frames = match texture.source.as_ref() {
            crate::noteskin::SpriteSource::Animated {
                frame_count,
                frame_durations,
                ..
            } => {
                let mut end = 0.0;
                (0..*frame_count)
                    .map(|frame| {
                        end += frame_durations
                            .as_ref()
                            .and_then(|delays| delays.get(frame))
                            .copied()
                            .unwrap_or(1.0);
                        (texture.uv_for_frame_at(frame, 0.0), end)
                    })
                    .collect::<Vec<_>>()
            }
            _ => vec![(texture.uv_for_frame_at(0, 0.0), 1.0)],
        };
        layer.additive = Some((texture.texture_key_shared(), frames.into()));
    }
    Some(layer)
}

pub(super) fn model_additive_frames(texture: &crate::noteskin::SpriteSlot) -> ModelAdditiveFrames {
    let frames = match texture.source.as_ref() {
        crate::noteskin::SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } => {
            let mut end = 0.0;
            (0..*frame_count)
                .map(|frame| {
                    end += frame_durations
                        .as_ref()
                        .and_then(|delays| delays.get(frame))
                        .copied()
                        .unwrap_or(1.0);
                    (texture.uv_for_frame_at(frame, 0.0), end)
                })
                .collect::<Vec<_>>()
        }
        _ => vec![(texture.uv_for_frame_at(0, 0.0), 1.0)],
    };
    frames.into()
}
