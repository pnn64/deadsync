// Frozen from e54a02471; only visibility/imports differ.
use super::*;

pub(super) fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
    player_options: &str,
) -> Result<CompiledNoteskinBundle, String> {
    let scripts = noteskin_paths(data);
    let lua = Lua::new();
    install_host(&lua, game, data, player_options).map_err(|err| err.to_string())?;
    let (noteskin, color_variants) = load_noteskin_table(&lua, &scripts)?;
    Ok(CompiledNoteskinBundle {
        version: noteskin_compiled::CACHE_SCHEMA_VERSION,
        game: game.to_string(),
        skin: data.name.clone(),
        source_hash: source_hash.to_string(),
        loader: CompiledLoader {
            version: COMPILER_VERSION,
            game: game.to_string(),
            skin: data.name.clone(),
            entries: compile_entries(&lua, &noteskin, game, data, color_variants)?,
        },
        actors: CompiledActors {
            version: COMPILER_VERSION,
            files: compile_actor_files(&lua, data)?,
        },
    })
}

pub(super) fn install_host(
    lua: &Lua,
    game: &str,
    data: &noteskin_itg::NoteskinData,
    player_options: &str,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let player = lua.create_table()?;
    let options = player_options.to_owned();
    player.set(
        "GetPlayerOptionsString",
        lua.create_function(move |_, _: MultiValue| Ok(options.clone()))?,
    )?;
    let state = lua.create_table()?;
    let current_game = lua.create_table()?;
    let game_name = game.trim().to_ascii_lowercase();
    current_game.set(
        "GetName",
        lua.create_function(move |_, _: MultiValue| Ok(game_name.clone()))?,
    )?;
    state.set(
        "GetCurrentGame",
        lua.create_function(move |_, _: MultiValue| Ok(current_game.clone()))?,
    )?;
    state.set(
        "GetPlayerState",
        lua.create_function(move |_, _: MultiValue| Ok(player.clone()))?,
    )?;
    globals.set("GAMESTATE", state)?;
    let theme = lua.create_table()?;
    // DeadSync has no ThemeInfo.ini author; match its song-Lua compatibility API.
    theme.set(
        "GetThemeAuthor",
        lua.create_function(|_, _: MultiValue| Ok(""))?,
    )?;
    globals.set("THEME", theme)?;
    let actor_mt = lua.create_table()?;
    let actor_methods = lua.create_table()?;
    for name in [
        "x",
        "y",
        "z",
        "addx",
        "addy",
        "addz",
        "rotationx",
        "rotationy",
        "rotationz",
        "addrotationx",
        "addrotationy",
        "addrotationz",
        "zoom",
        "zoomx",
        "zoomy",
        "zoomz",
        "diffuse",
        "diffusealpha",
        "glow",
        "vertalign",
        "valign",
        "blend",
        "visible",
        "SetTextureFiltering",
    ] {
        let command = name.to_string();
        actor_methods.set(
            name,
            lua.create_function(move |lua, (actor, args): (Table, MultiValue)| {
                append_actor_command(lua, &actor, &command, args)?;
                Ok(actor)
            })?,
        )?;
    }
    actor_mt.set("__index", actor_methods)?;
    actor_mt.set(
        "__concat",
        lua.create_function(|_, (lhs, _rhs): (Table, Value)| Ok(lhs))?,
    )?;
    let make_actor = {
        let actor_mt = actor_mt;
        lua.create_function(
            move |lua, (blank, button, element): (bool, Option<String>, Option<String>)| {
                let actor = lua.create_table()?;
                actor.set("__blank", blank)?;
                if let Some(button) = button {
                    actor.set("__load_button", button)?;
                }
                if let Some(element) = element {
                    actor.set("__load_element", element)?;
                }
                let _ = actor.set_metatable(Some(actor_mt.clone()));
                Ok(actor)
            },
        )?
    };
    let load_actor = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, value: Value| -> mlua::Result<Table> {
            make_actor_for_path(&make_actor, value)
        })?
    };
    globals.set("LoadActor", load_actor)?;
    let var_fn = lua.create_function(|lua, name: String| {
        let globals = lua.globals();
        match name.as_str() {
            // ITGmania installs these thread variables only while requesting an
            // actor. NoteSkin.lua itself can read them before a request exists.
            "Button" => globals.get::<Value>("__itg_button"),
            "Element" => globals.get::<Value>("__itg_element"),
            "Color" => Ok(Value::String(
                lua.create_string(
                    globals
                        .get::<Option<String>>("__itg_color")?
                        .as_deref()
                        .unwrap_or("4th"),
                )?,
            )),
            "SpriteOnly" => Ok(Value::Boolean(
                globals.get::<bool>("__itg_sprite_only").unwrap_or(false),
            )),
            "Player" => Ok(Value::Integer(0)),
            _ => Ok(Value::Nil),
        }
    })?;
    globals.set("Var", var_fn)?;
    globals.set(
        "cmd",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    let noteskin = lua.create_table()?;
    let metrics = data.metrics.clone();
    noteskin.set(
        "GetMetric",
        lua.create_function(move |_, (_self, section, key): (Table, String, String)| {
            Ok(metrics
                .get(&section, &key)
                .or_else(|| metrics.get("NoteDisplay", &key))
                .unwrap_or("")
                .to_owned())
        })?,
    )?;
    noteskin.set(
        "GetPath",
        lua.create_function(|lua, (_self, button, element): (Table, String, String)| {
            let path = lua.create_table()?;
            path.set("load_button", button)?;
            path.set("load_element", element)?;
            Ok(path)
        })?,
    )?;
    globals.set("NOTESKIN", noteskin)?;
    let def = lua.create_table()?;
    let actor_fn = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, _value: Value| -> mlua::Result<Table> {
            make_actor.call((true, None::<String>, None::<String>))
        })?
    };
    def.set("Actor", actor_fn)?;
    let sprite_fn = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, value: Value| -> mlua::Result<Table> {
            let Value::Table(props) = value else {
                return make_actor.call((false, None::<String>, None::<String>));
            };
            let actor = match props.get::<Value>("Texture")? {
                Value::Nil => make_actor.call((false, None::<String>, None::<String>))?,
                texture => make_actor_for_path(&make_actor, texture)?,
            };
            copy_actor_fields(&props, &actor)?;
            Ok(actor)
        })?
    };
    def.set("Sprite", sprite_fn)?;
    globals.set("Def", def)?;
    let data = data.clone();
    let loadfile = {
        let make_actor = make_actor;
        lua.create_function(move |lua, value: Value| -> mlua::Result<Value> {
            let Some((button, element)) = loadfile_target(&data, value)? else {
                return Ok(Value::Nil);
            };
            let make_actor = make_actor.clone();
            let func = lua.create_function(move |_, _args: MultiValue| -> mlua::Result<Table> {
                make_actor.call((false, button.clone(), element.clone()))
            })?;
            Ok(Value::Function(func))
        })?
    };
    globals.set("loadfile", loadfile)?;
    Ok(())
}

pub(super) fn actor_loader_command(actor: &Table) -> Result<Option<String>, String> {
    let value = actor
        .get::<Value>("InitCommand")
        .map_err(|err| err.to_string())?;
    match value {
        Value::Function(f) => {
            f.call::<()>(actor.clone()).map_err(|err| err.to_string())?;
        }
        Value::String(s) => {
            let command = s.to_str().map_err(|err| err.to_string())?.to_string();
            if !command.trim().is_empty() {
                return Ok(Some(command));
            }
        }
        _ => {}
    }
    let Some(commands) = actor
        .get::<Option<Table>>("__loader_commands")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(commands.raw_len());
    for command in commands.sequence_values::<String>() {
        let command = command.map_err(|err| err.to_string())?;
        if !command.trim().is_empty() {
            out.push(command);
        }
    }
    Ok((!out.is_empty()).then(|| out.join(";")))
}

fn compile_entries(
    lua: &Lua,
    noteskin: &Table,
    game: &str,
    data: &noteskin_itg::NoteskinData,
    color_variants: bool,
) -> Result<Vec<CompiledLoaderEntry>, String> {
    let (buttons, elements) = collect_loader_domain(game, data);
    normalize_noteskin_tables(noteskin, &buttons, &elements)
        .map_err(|err| format!("failed to normalize noteskin loader tables: {err}"))?;
    let load = noteskin
        .get::<Function>("Load")
        .map_err(|err| format!("compiled noteskin is missing Load(): {err}"))?;
    let globals = lua.globals();
    let mut out = Vec::with_capacity(buttons.len() * elements.len());
    for button in &buttons {
        for element in &elements {
            globals
                .set("__itg_button", button.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_element", element.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_sprite_only", true)
                .map_err(|err| err.to_string())?;
            let default_index = out.len();
            let colors = if color_variants {
                &Quantization::ALL[..]
            } else {
                &Quantization::ALL[..1]
            };
            for color in colors {
                globals
                    .set("__itg_color", color.color_name())
                    .map_err(|err| err.to_string())?;
                let actor = load.call::<Table>(()).map_err(|err| {
                    format!(
                        "Load() failed for '{button} {element}' ({}): {err}",
                        color.color_name()
                    )
                })?;
                let mut entry = read_entry(button, element, &actor)?;
                if *color == Quantization::Q4th {
                    out.push(entry);
                } else if entry != out[default_index] {
                    entry.color = Some(*color);
                    out.push(entry);
                }
            }
        }
    }
    globals
        .set("__itg_color", "4th")
        .map_err(|err| err.to_string())?;
    sort_compiled_loader_entries(&mut out);
    Ok(out)
}

fn read_entry(button: &str, element: &str, actor: &Table) -> Result<CompiledLoaderEntry, String> {
    let blank = actor.get::<bool>("__blank").unwrap_or(false);
    let load_button = actor
        .get::<Option<String>>("__load_button")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| button.to_string());
    let load_element = actor
        .get::<Option<String>>("__load_element")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| element.to_string());
    let rotation_x = actor.get::<Option<i32>>("BaseRotationX").unwrap_or(None);
    let rotation_y = actor.get::<Option<i32>>("BaseRotationY").unwrap_or(None);
    let rotation_z = actor.get::<Option<i32>>("BaseRotationZ").unwrap_or(None);
    let init_command = actor_loader_command(actor)?;
    Ok(CompiledLoaderEntry {
        button: button.to_string(),
        element: element.to_string(),
        color: None,
        load_button,
        load_element,
        blank,
        rotation_x,
        rotation_y,
        rotation_z,
        init_command,
    })
}
