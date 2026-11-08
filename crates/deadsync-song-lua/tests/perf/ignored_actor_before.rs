// Helpers frozen from 1c744eebf; parent visibility only.
use super::*;

pub(super) fn install_actor_size_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.0)
        })?,
    )?;
    actor.set(
        "GetZoomedWidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let width = actor_base_size(&actor)?.0;
                Ok(width
                    * actor_zoom_axis(
                        lua,
                        &actor,
                        "__songlua_state_zoom_x",
                        "__songlua_state_basezoom_x",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetNumStates",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(i64::from(actor_sprite_frame_count(&actor)?))
        })?,
    )?;
    actor.set(
        "GetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.1)
        })?,
    )?;
    actor.set(
        "GetZoomedHeight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let height = actor_base_size(&actor)?.1;
                Ok(height
                    * actor_zoom_axis(
                        lua,
                        &actor,
                        "__songlua_state_zoom_y",
                        "__songlua_state_basezoom_y",
                    )?)
            }
        })?,
    )?;
    Ok(())
}
