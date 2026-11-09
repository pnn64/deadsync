//! Production Rust update startup controls for integration fixtures.
use super::*;
use crate::rust_engine::{RustSongRuntime, SongActivation, SongOverlayActors};

pub fn prepare_update_startup<K>(
    runtime: &mut RustSongRuntime,
    rows: &mut SongOverlayActors<K>,
    activation: &SongActivation,
    _context: &crate::SongLuaCompileContext,
    messages: &mut Vec<crate::SongLuaMessageEvent>,
) -> Result<(), String> {
    let mut changed = false;
    for row in &mut rows.actors {
        let deadlib_lua::Value::Table(id) = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?
        else {
            unreachable!()
        };
        let Some(startup) = activation.tweens.get(&id) else {
            continue;
        };
        if !startup.blocks.iter().any(|block| {
            (block.duration > 0.
                || block
                    .progress
                    .as_ref()
                    .is_some_and(|samples| samples.first().is_some_and(|sample| sample[0] > 0.)))
                && block.delta != crate::SongLuaOverlayStateDelta::default()
        }) {
            continue;
        }
        row.actor.initial_state =
            crate::overlay_state_after_blocks(startup.initial, &startup.blocks, 0.);
        row.actor
            .message_commands
            .push(crate::SongLuaOverlayMessageCommand {
                frame_advance: 0.,
                message: "__songlua_actor_startup".into(),
                blocks: startup.blocks.clone(),
            });
        changed = true;
    }
    if changed {
        messages.push(crate::SongLuaMessageEvent {
            beat: 0.,
            message: "__songlua_actor_startup".into(),
            persists: true,
        });
    }
    Ok(())
}
