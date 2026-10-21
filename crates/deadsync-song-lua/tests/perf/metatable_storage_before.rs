use super::*;

pub fn install_actor_metatable(
    lua: &Lua,
    actor: &Table,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    crate::actor_classes::bind(lua, actor)?;
    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(crate::actor_classes::lookup)?)?;
    mt.set("__newindex", lua.create_function(crate::actor_classes::assign)?)?;
    mt.set(
        "__concat",
        lua.create_function(move |lua, (lhs, rhs): (Table, Table)| {
            // ActorDef.MergeTables makes a fresh definition and keeps the
            // original metatable. Bound methods must address that new table.
            let actor = clone_actor_def(lua, &lhs, install_methods, false)?;
            merge_actor_concat(lua, &actor, &rhs)?;
            actor.set_metatable(lhs.metatable())?;
            Ok(actor)
        })?,
    )?;
    mt.set(
        "__tostring",
        lua.create_function(|_, actor: Table| {
            let kind = actor
                .raw_get::<Option<String>>("__songlua_actor_type")?
                .unwrap_or_else(|| "Actor".into());
            let kind = match kind.as_str() {
                "Quad" => "Sprite",
                "WrapperState" => "ActorFrame",
                "TopScreen" => "ScreenGameplay",
                kind => kind,
            };
            // Luna::tostring_T exposes only type and opaque object identity.
            #[cfg(target_os = "windows")]
            let identity = format!("{:016X}", actor.to_pointer() as usize);
            #[cfg(not(target_os = "windows"))]
            let identity = format!("{:p}", actor.to_pointer());
            Ok(format!("{kind} ({identity})"))
        })?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}
