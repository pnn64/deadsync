use mlua::{Function, Lua, MultiValue, Table, Value};
use std::path::Path;

use crate::{
    SongLuaCompileContext, SongLuaNoteskinResolver, file_path_string, is_song_lua_image_path,
    song_lua_default_noteskin_name,
};

pub type SongLuaActorFactory = fn(&Lua, &'static str) -> mlua::Result<Table>;

pub fn create_noteskin_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    resolver: SongLuaNoteskinResolver,
    create_actor: SongLuaActorFactory,
) -> mlua::Result<Table> {
    noteskin_table_for_skin(
        lua,
        resolver,
        create_actor,
        song_lua_default_noteskin_name(context),
    )
}

fn noteskin_table_for_skin(
    lua: &Lua,
    resolver: SongLuaNoteskinResolver,
    create_actor: SongLuaActorFactory,
    default_noteskin: String,
) -> mlua::Result<Table> {
    let noteskin = lua.create_table()?;

    let default_metric_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetric",
        lua.create_function(
            move |lua, (_self, element, value): (Table, String, String)| {
                let Some(metric) = resolver.metric(&default_metric_skin, &element, &value) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricForNoteSkin",
        lua.create_function(
            move |lua, (_self, element, value, skin): (Table, String, String, String)| {
                let Some(metric) = resolver.metric(&skin, &element, &value) else {
                    return Ok(Value::Nil);
                };
                Ok(Value::String(lua.create_string(&metric)?))
            },
        )?,
    )?;

    let default_metric_f_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricF",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver
                .metric_f(&default_metric_f_skin, &element, &value)
                .unwrap_or(0.0_f32))
        })?,
    )?;
    noteskin.set(
        "GetMetricFForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver
                    .metric_f(&skin, &element, &value)
                    .unwrap_or(0.0_f32))
            },
        )?,
    )?;

    let default_metric_i_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricI",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver.metric_i(&default_metric_i_skin, &element, &value))
        })?,
    )?;
    noteskin.set(
        "GetMetricIForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver.metric_i(&skin, &element, &value))
            },
        )?,
    )?;

    let default_metric_b_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricB",
        lua.create_function(move |_, (_self, element, value): (Table, String, String)| {
            Ok(resolver
                .metric_b(&default_metric_b_skin, &element, &value)
                .unwrap_or(false))
        })?,
    )?;
    noteskin.set(
        "GetMetricBForNoteSkin",
        lua.create_function(
            move |_, (_self, element, value, skin): (Table, String, String, String)| {
                Ok(resolver.metric_b(&skin, &element, &value).unwrap_or(false))
            },
        )?,
    )?;

    let default_metric_a_skin = default_noteskin.clone();
    noteskin.set(
        "GetMetricA",
        lua.create_function(
            move |lua, (_self, element, value): (Table, String, String)| {
                song_lua_noteskin_metric_a(lua, resolver, &default_metric_a_skin, &element, &value)
            },
        )?,
    )?;
    noteskin.set(
        "GetMetricAForNoteSkin",
        lua.create_function(
            move |lua, (_self, element, value, skin): (Table, String, String, String)| {
                song_lua_noteskin_metric_a(lua, resolver, &skin, &element, &value)
            },
        )?,
    )?;

    let default_path_skin = default_noteskin.clone();
    noteskin.set(
        "GetPath",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                let path = resolver.path_string(&default_path_skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;
    noteskin.set(
        "GetPathForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                let path = resolver.path_string(&skin, &button, &element);
                Ok(Value::String(lua.create_string(&path)?))
            },
        )?,
    )?;

    let default_load_skin = default_noteskin;
    noteskin.set(
        "LoadActor",
        lua.create_function(
            move |lua, (_self, button, element): (Table, String, String)| {
                song_lua_noteskin_actor(
                    lua,
                    resolver,
                    create_actor,
                    &default_load_skin,
                    &button,
                    &element,
                )
            },
        )?,
    )?;
    noteskin.set(
        "LoadActorForNoteSkin",
        lua.create_function(
            move |lua, (_self, button, element, skin): (Table, String, String, String)| {
                song_lua_noteskin_actor(lua, resolver, create_actor, &skin, &button, &element)
            },
        )?,
    )?;

    noteskin.set(
        "DoesNoteSkinExist",
        lua.create_function(move |_, (_self, skin): (Table, String)| Ok(resolver.exists(&skin)))?,
    )?;
    noteskin.set(
        "GetNoteSkinNames",
        lua.create_function(move |lua, _args: MultiValue| {
            let names = resolver.names();
            let table = lua.create_table()?;
            for (idx, name) in names.into_iter().enumerate() {
                table.raw_set(idx + 1, name)?;
            }
            Ok(table)
        })?,
    )?;
    noteskin.set(
        "HasVariants",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "IsNoteSkinVariant",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    noteskin.set(
        "GetVariantNamesForNoteSkin",
        lua.create_function(|lua, _args: MultiValue| lua.create_table())?,
    )?;
    Ok(noteskin)
}

fn song_lua_noteskin_metric_a(
    lua: &Lua,
    resolver: SongLuaNoteskinResolver,
    skin: &str,
    element: &str,
    value: &str,
) -> mlua::Result<Function> {
    let raw = resolver.metric(skin, element, value).unwrap_or_default();
    if let Some(source) = raw.strip_prefix('%') {
        return lua.load(format!("return {source}")).eval();
    }
    let source = crate::preprocess_lua_cmd_syntax(&format!("return cmd({raw})"))
        .map_err(mlua::Error::external)?;
    lua.load(source).eval()
}

fn song_lua_noteskin_actor(
    lua: &Lua,
    resolver: SongLuaNoteskinResolver,
    create_actor: SongLuaActorFactory,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<Table> {
    let resolved = resolver.resolve_path(skin, button, element);
    if let Some(path) = resolved.as_ref().filter(|path| {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
    }) {
        // Each template captures its own noteskin and Var environment. Nested
        // loads and later commands must not inherit another template's button.
        let env = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.set("__index", lua.globals())?;
        env.set_metatable(Some(meta))?;
        env.set(
            "NOTESKIN",
            noteskin_table_for_skin(lua, resolver, create_actor, skin.to_owned())?,
        )?;
        let var_button = button.to_owned();
        let var_element = element.to_owned();
        let fallback: Function = lua.globals().get("Var")?;
        env.set(
            "Var",
            lua.create_function(move |lua, name: String| match name.as_str() {
                "Button" => Ok(Value::String(lua.create_string(&var_button)?)),
                "Element" => Ok(Value::String(lua.create_string(&var_element)?)),
                _ => fallback.call::<Value>(name),
            })?,
        )?;
        let actor: Table = crate::lua_util::load_script_file_with_env(
            lua,
            path,
            path.parent().unwrap_or(Path::new(".")),
            Some(env),
        )?
        .call(())?;
        tag_song_lua_noteskin_actor(&actor, skin, button, element)?;
        return Ok(actor);
    }
    let sprite_path = resolved
        .as_ref()
        .filter(|path| is_song_lua_image_path(path));
    let actor = create_actor(
        lua,
        if sprite_path.is_some() {
            "Sprite"
        } else {
            "Actor"
        },
    )?;
    tag_song_lua_noteskin_actor(&actor, skin, button, element)?;
    if let Some(path) = sprite_path {
        actor.set("Texture", file_path_string(path))?;
    }
    Ok(actor)
}

fn tag_song_lua_noteskin_actor(
    actor: &Table,
    skin: &str,
    button: &str,
    element: &str,
) -> mlua::Result<()> {
    actor.set("__songlua_noteskin_name", skin.trim().to_ascii_lowercase())?;
    actor.set("__songlua_noteskin_button", button)?;
    actor.set("__songlua_noteskin_element", element)
}
