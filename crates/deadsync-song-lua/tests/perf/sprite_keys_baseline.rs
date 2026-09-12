// Frozen from e7501fe239507bde3eaa96cd30c4affaacad10db; only test visibility/formatting differs.

use super::*;

pub(super) fn read_sprite_states(actor: &Table) -> Result<Vec<crate::SongLuaSpriteState>, String> {
    let mut states = Vec::new();
    for index in 0.. {
        let frame_key = format!("Frame{index:04}");
        let Some(frame) = actor
            .get::<Option<u32>>(frame_key)
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        let delay = actor
            .get::<Option<f32>>(format!("Delay{index:04}"))
            .map_err(|err| err.to_string())?
            .unwrap_or(0.1);
        states.push(crate::SongLuaSpriteState { frame, delay });
    }
    Ok(states)
}
