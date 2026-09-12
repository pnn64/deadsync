// Frozen from a82c06196b06e11cd809dd5cd8972ecc007d6999; only test visibility differs.

use super::*;

pub fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<(), String> {
    let indices: Vec<_> = (0..tracked_actors.len()).collect();
    reset_tracked_capture_tables_for_indices(lua, tracked_actors, &indices)
}

pub(super) fn reset_tracked_capture_tables_for_indices(
    lua: &Lua,
    tracked_actors: &[SongLuaTrackedActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(actor) = tracked_actors.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn collect_indexed_actor_capture_blocks(
    actors: &[(usize, Table)],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for (index, actor) in actors {
        flush_actor_capture(actor).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(actor)?;
        if !blocks.is_empty() {
            out.push((*index, blocks));
        }
    }
    Ok(out)
}

pub fn collect_tracked_capture_blocks_for_indices(
    tracked_actors: &[SongLuaTrackedActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let actors = indices
        .iter()
        .filter_map(|&index| {
            tracked_actors
                .get(index)
                .map(|actor| (index, actor.table.clone()))
        })
        .collect::<Vec<_>>();
    collect_indexed_actor_capture_blocks(&actors)
}
