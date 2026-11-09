//! Production noteskin preparation using the shared Rust Lua engine.
use super::{
    COMPILER_VERSION, collect_loader_domain, compile_actor_files_with, noteskin_paths, path_is_lua,
    sort_compiled_loader_entries, uses_actor_var,
};
use crate::{
    Quantization, actor as noteskin_actor,
    compiled::{
        self as noteskin_compiled, CompiledActors, CompiledLoader, CompiledLoaderEntry,
        CompiledNoteskinBundle,
    },
    itg as noteskin_itg,
};
use deadlib_lua::{Error, HostCall, HostFunction, Root, Value as LuaValue, Vm};
use std::{fs, path::Path, sync::Arc};

type Result<T> = std::result::Result<T, String>;

/// Compile an isolated skin without consulting or populating the session cache.
/// An empty source hash is suitable for a song-lifetime, in-memory bundle.
pub fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
    player_options: &str,
) -> Result<CompiledNoteskinBundle> {
    let mut session = Session::new()?;
    session.install(game, data, player_options)?;
    let mut current = LuaValue::Nil;
    let mut colors = false;
    let scripts = noteskin_paths(data);
    if scripts.is_empty() {
        return Err("no NoteSkin.lua files were found in fallback chain".into());
    }
    for path in scripts {
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
        colors |= uses_actor_var(&content, "Color");
        let function = session
            .compile(&content, &path.to_string_lossy())
            .map_err(|err| format!("failed to compile '{}': {err}", path.display()))?;
        let arguments = if current == LuaValue::Nil {
            vec![]
        } else {
            vec![current]
        };
        current = session
            .call(function, &arguments)
            .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?;
        table(current)?;
    }
    let entries = session.entries(current, game, data, colors)?;
    let files = compile_actor_files_with(data, |content, button, color, decl| {
        session.scoped(|session| session.color_paths(content, button, color, decl))
    })?;
    Ok(CompiledNoteskinBundle {
        version: noteskin_compiled::CACHE_SCHEMA_VERSION,
        game: game.into(),
        skin: data.name.clone(),
        source_hash: source_hash.into(),
        loader: CompiledLoader {
            version: COMPILER_VERSION,
            game: game.into(),
            skin: data.name.clone(),
            entries,
        },
        actors: CompiledActors {
            version: COMPILER_VERSION,
            files,
        },
    })
}

fn error(error: Error) -> String {
    error.to_string()
}
fn table(value: LuaValue) -> Result<deadlib_lua::TableId> {
    match value {
        LuaValue::Table(id) => Ok(id),
        _ => Err("noteskin table expected".into()),
    }
}

// Values held by preparation are explicitly rooted. The entire session, its
// handles and Lua state are released after producing the native bundle.
struct Session {
    vm: Vm,
    roots: Vec<Root>,
    globals: LuaValue,
    get: LuaValue,
    set: LuaValue,
    next: LuaValue,
    setmetatable: LuaValue,
    setfenv: LuaValue,
    tonumber: LuaValue,
}

impl Session {
    fn new() -> Result<Self> {
        let mut vm = Vm::new();
        let globals = vm.global(b"_G");
        let get = vm.new_host_function(get_field, &[]).map_err(error)?;
        let set = vm.new_host_function(set_field, &[]).map_err(error)?;
        let next = vm.global(b"next");
        let setmetatable = vm.global(b"setmetatable");
        let setfenv = vm.global(b"setfenv");
        let tonumber = vm.global(b"tonumber");
        let mut session = Self {
            vm,
            roots: Vec::new(),
            globals,
            get,
            set,
            next,
            setmetatable,
            setfenv,
            tonumber,
        };
        for value in [globals, get, set, next, setmetatable, setfenv, tonumber] {
            session.keep(value)?;
        }
        Ok(session)
    }

    fn keep(&mut self, value: LuaValue) -> Result<LuaValue> {
        if !matches!(
            value,
            LuaValue::Nil | LuaValue::Boolean(_) | LuaValue::Number(_)
        ) {
            self.roots.push(self.vm.pin(value).map_err(error)?);
        }
        Ok(value)
    }

    // Only native data may leave this scope. Lua values retained by scripts
    // remain reachable through ordinary Lua edges after temporary roots end.
    fn scoped<T>(&mut self, run: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let start = self.roots.len();
        let result = run(self);
        for root in self.roots.drain(start..) {
            self.vm.unpin(root).map_err(error)?;
        }
        result
    }
    fn string(&mut self, value: &str) -> Result<LuaValue> {
        let value = self.vm.try_intern(value.as_bytes()).map_err(error)?;
        self.keep(value)
    }
    fn text(&mut self, value: LuaValue) -> Result<String> {
        let id = self
            .vm
            .coerce_string(value)
            .map_err(error)?
            .ok_or_else(|| "noteskin string expected".to_owned())?;
        std::str::from_utf8(self.vm.string_bytes(id).expect("live string"))
            .map(str::to_owned)
            .map_err(|err| err.to_string())
    }
    fn optional_text(&mut self, value: LuaValue) -> Result<Option<String>> {
        if value == LuaValue::Nil {
            Ok(None)
        } else {
            self.text(value).map(Some)
        }
    }
    fn table(&mut self) -> Result<LuaValue> {
        let value = LuaValue::Table(self.vm.try_new_table().map_err(error)?);
        self.keep(value)
    }
    fn call(&mut self, function: LuaValue, args: &[LuaValue]) -> Result<LuaValue> {
        let value = self
            .vm
            .call(function, args)
            .map_err(error)?
            .first()
            .copied()
            .unwrap_or(LuaValue::Nil);
        self.keep(value)
    }
    fn field(&mut self, target: LuaValue, name: &str) -> Result<LuaValue> {
        let key = self.string(name)?;
        self.call(self.get, &[target, key])
    }
    fn put(&mut self, target: LuaValue, name: &str, value: LuaValue) -> Result<()> {
        let key = self.string(name)?;
        self.call(self.set, &[target, key, value])?;
        Ok(())
    }
    fn put_text(&mut self, target: LuaValue, name: &str, value: &str) -> Result<()> {
        let value = self.string(value)?;
        self.put(target, name, value)
    }
    fn raw(&mut self, target: LuaValue, index: usize) -> Result<LuaValue> {
        let value = self
            .vm
            .raw_table_get(table(target)?, LuaValue::Number(index as f64))
            .map_err(error)?;
        self.keep(value)
    }
    fn len(&self, target: LuaValue) -> Result<usize> {
        self.vm.raw_table_len(table(target)?).map_err(error)
    }
    fn compile(&mut self, source: &str, name: &str) -> Result<LuaValue> {
        let function = self
            .vm
            .try_load_source(source.as_bytes(), name.as_bytes())
            .map_err(error)?;
        self.keep(LuaValue::Function(function))
    }
    fn bind(
        &mut self,
        target: LuaValue,
        name: &str,
        binding: Binding,
        captures: &[LuaValue],
    ) -> Result<LuaValue> {
        let function = self
            .vm
            .new_host_function_with_data(dispatch, captures, binding)
            .map_err(error)?;
        self.keep(function)?;
        self.put(target, name, function)?;
        Ok(function)
    }
    fn callback(&mut self, function: HostFunction, captures: &[LuaValue]) -> Result<LuaValue> {
        let function = self
            .vm
            .new_host_function(function, captures)
            .map_err(error)?;
        self.keep(function)
    }

    fn install(
        &mut self,
        game: &str,
        data: &noteskin_itg::NoteskinData,
        options: &str,
    ) -> Result<()> {
        let globals = self.globals;
        // Match the safe standard-library subset previously opened by Lua::new.
        // Removing both references also prevents require from returning debug.
        self.put(globals, "debug", LuaValue::Nil)?;
        let package = self.field(globals, "package")?;
        let loaded = self.field(package, "loaded")?;
        self.put(loaded, "debug", LuaValue::Nil)?;
        let denied = self.callback(deny_loadlib, &[])?;
        self.put(package, "loadlib", denied)?;
        let loaders = self.field(package, "loaders")?;
        let diagnostic = self.string("\n\tcan't load C modules in safe mode")?;
        let c_loader = self
            .vm
            .new_host_function_with_data(dispatch, &[diagnostic], Binding::Constant)
            .map_err(error)?;
        self.keep(c_loader)?;
        self.vm
            .raw_table_set(table(loaders)?, LuaValue::Number(3.0), c_loader)
            .map_err(error)?;
        self.vm
            .raw_table_set(table(loaders)?, LuaValue::Number(4.0), LuaValue::Nil)
            .map_err(error)?;
        let player = self.table()?;
        let options = self.string(options)?;
        self.bind(
            player,
            "GetPlayerOptionsString",
            Binding::Constant,
            &[options],
        )?;
        let current_game = self.table()?;
        let game = self.string(&game.trim().to_ascii_lowercase())?;
        self.bind(current_game, "GetName", Binding::Constant, &[game])?;
        let state = self.table()?;
        self.bind(state, "GetCurrentGame", Binding::Constant, &[current_game])?;
        self.bind(state, "GetPlayerState", Binding::Constant, &[player])?;
        self.put(globals, "GAMESTATE", state)?;
        let theme = self.table()?;
        let empty = self.string("")?;
        self.bind(theme, "GetThemeAuthor", Binding::Constant, &[empty])?;
        self.put(globals, "THEME", theme)?;

        let actor_mt = self.table()?;
        let methods = self.table()?;
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
            self.bind(methods, name, Binding::Command(name), &[])?;
        }
        self.put(actor_mt, "__index", methods)?;
        self.bind(actor_mt, "__concat", Binding::Identity, &[])?;
        let make_actor = self.callback(make_actor, &[actor_mt, self.setmetatable])?;
        let load_actor = self.bind(globals, "LoadActor", Binding::LoadActor, &[make_actor])?;
        self.bind(globals, "Var", Binding::Var, &[globals])?;
        self.bind(globals, "cmd", Binding::Constant, &[LuaValue::Nil])?;
        let noteskin = self.table()?;
        let data = Arc::new(data.clone());
        self.bind(noteskin, "GetMetric", Binding::Metric(data.clone()), &[])?;
        self.bind(noteskin, "GetPath", Binding::Path, &[])?;
        self.put(globals, "NOTESKIN", noteskin)?;
        let def = self.table()?;
        self.bind(def, "Actor", Binding::Actor, &[make_actor])?;
        self.bind(def, "Sprite", Binding::Sprite, &[make_actor, load_actor])?;
        self.put(globals, "Def", def)?;
        self.bind(globals, "loadfile", Binding::LoadFile(data), &[make_actor])?;
        Ok(())
    }

    fn normalize(
        &mut self,
        noteskin: LuaValue,
        buttons: &[String],
        elements: &[String],
    ) -> Result<()> {
        for (names, canonical) in [
            (
                &["RedirTable", "ButtonRedir", "ButtonRedirs", "Rotate"][..],
                buttons,
            ),
            (
                &[
                    "ElementRedir",
                    "ElementRedirs",
                    "PartsToRotate",
                    "Blank",
                    "bBlanks",
                ][..],
                elements,
            ),
        ] {
            for name in names {
                let target = self.field(noteskin, name)?;
                if target == LuaValue::Nil {
                    continue;
                }
                table(target)?;
                let mut existing = Vec::new();
                let mut key = LuaValue::Nil;
                loop {
                    let values = self
                        .vm
                        .call(self.next, &[target, key])
                        .map_err(error)?
                        .to_vec();
                    key = values.first().copied().unwrap_or(LuaValue::Nil);
                    if key == LuaValue::Nil {
                        break;
                    }
                    self.keep(key)?;
                    self.keep(values[1])?;
                    if let LuaValue::String(id) = key
                        && let Ok(text) =
                            std::str::from_utf8(self.vm.string_bytes(id).expect("live key"))
                    {
                        existing.push((text.to_owned(), values[1]));
                    }
                }
                for want in canonical {
                    if self.field(target, want)? != LuaValue::Nil {
                        continue;
                    }
                    if let Some((_, value)) = existing
                        .iter()
                        .find(|(have, _)| have.eq_ignore_ascii_case(want))
                    {
                        self.put(target, want, *value)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn entries(
        &mut self,
        noteskin: LuaValue,
        game: &str,
        data: &noteskin_itg::NoteskinData,
        colors: bool,
    ) -> Result<Vec<CompiledLoaderEntry>> {
        let (buttons, elements) = collect_loader_domain(game, data);
        self.normalize(noteskin, &buttons, &elements)?;
        let load = self.field(noteskin, "Load")?;
        if !matches!(load, LuaValue::Function(_) | LuaValue::Native(_)) {
            return Err("compiled noteskin is missing Load()".into());
        }
        let mut out = Vec::new();
        for button in &buttons {
            for element in &elements {
                self.put_text(self.globals, "__itg_button", button)?;
                self.put_text(self.globals, "__itg_element", element)?;
                self.put(self.globals, "__itg_sprite_only", LuaValue::Boolean(true))?;
                let default = out.len();
                let colors = if colors {
                    &Quantization::ALL[..]
                } else {
                    &Quantization::ALL[..1]
                };
                for color in colors {
                    let mut entry = self.scoped(|session| {
                        session.put_text(session.globals, "__itg_color", color.color_name())?;
                        let actor = session.call(load, &[]).map_err(|err| {
                            format!(
                                "Load() failed for '{button} {element}' ({}): {err}",
                                color.color_name()
                            )
                        })?;
                        session.read_entry(button, element, actor)
                    })?;
                    if *color == Quantization::Q4th {
                        out.push(entry);
                    } else if entry != out[default] {
                        entry.color = Some(*color);
                        out.push(entry);
                    }
                }
            }
        }
        self.put_text(self.globals, "__itg_color", "4th")?;
        sort_compiled_loader_entries(&mut out);
        Ok(out)
    }

    fn read_entry(
        &mut self,
        button: &str,
        element: &str,
        actor: LuaValue,
    ) -> Result<CompiledLoaderEntry> {
        table(actor)?;
        let blank = !matches!(
            self.field(actor, "__blank").unwrap_or(LuaValue::Nil),
            LuaValue::Nil | LuaValue::Boolean(false)
        );
        let value = self.field(actor, "__load_button")?;
        let load_button = self.optional_text(value)?.unwrap_or_else(|| button.into());
        let value = self.field(actor, "__load_element")?;
        let load_element = self.optional_text(value)?.unwrap_or_else(|| element.into());
        let mut rotations = [None; 3];
        for (rotation, key) in
            rotations
                .iter_mut()
                .zip(["BaseRotationX", "BaseRotationY", "BaseRotationZ"])
        {
            if let Ok(value) = self.field(actor, key)
                && let Ok(LuaValue::Number(n)) = self.call(self.tonumber, &[value])
                && n.is_finite()
                && n.trunc() >= i32::MIN as f64
                && n.trunc() <= i32::MAX as f64
            {
                *rotation = Some(n as i32);
            }
        }
        let init = self.field(actor, "InitCommand")?;
        let mut init_command = None;
        match init {
            LuaValue::Function(_) | LuaValue::Native(_) => {
                self.call(init, &[actor])?;
            }
            LuaValue::String(_) => {
                let text = self.text(init)?;
                if !text.trim().is_empty() {
                    init_command = Some(text);
                }
            }
            _ => {}
        }
        if init_command.is_none() {
            let commands = self.field(actor, "__loader_commands")?;
            if commands != LuaValue::Nil {
                let mut out = Vec::new();
                let mut index = 1;
                loop {
                    let value = self.raw(commands, index)?;
                    if value == LuaValue::Nil {
                        break;
                    }
                    let command = self.text(value)?;
                    if !command.trim().is_empty() {
                        out.push(command);
                    }
                    index += 1;
                }
                if !out.is_empty() {
                    init_command = Some(out.join(";"));
                }
            }
        }
        Ok(CompiledLoaderEntry {
            button: button.into(),
            element: element.into(),
            color: None,
            load_button,
            load_element,
            blank,
            rotation_x: rotations[0],
            rotation_y: rotations[1],
            rotation_z: rotations[2],
            init_command,
        })
    }

    fn color_paths(
        &mut self,
        content: &str,
        button: Option<&str>,
        color: Quantization,
        decl: &mut noteskin_actor::ItgLuaActorDecl,
    ) -> Result<()> {
        let env = self.table()?;
        let mt = self.table()?;
        self.put(mt, "__index", self.globals)?;
        self.call(self.setmetatable, &[env, mt])?;
        let original = self.field(self.globals, "Var")?;
        let color = self.string(color.color_name())?;
        let button = self.string(button.unwrap_or("Down"))?;
        self.bind(env, "Var", Binding::ColorVar, &[original, color, button])?;
        let def = self.table()?;
        let models = self.table()?;
        let sprites = self.table()?;
        self.bind(def, "Model", Binding::Capture, &[models])?;
        self.bind(def, "Sprite", Binding::Capture, &[sprites])?;
        self.bind(def, "ActorFrame", Binding::Identity, &[])?;
        self.bind(def, "Actor", Binding::Identity, &[])?;
        self.put(env, "Def", def)?;
        let function = self.compile(content, "=(noteskin actor)")?;
        self.call(self.setfenv, &[function, env])?;
        self.call(function, &[])?;
        if self.len(models)? != decl.models.len() || self.len(sprites)? != decl.sprites.len() {
            return Err("color-dependent actor structure is unsupported".into());
        }
        for (index, model) in decl.models.iter_mut().enumerate() {
            let props = self.raw(models, index + 1)?;
            let value = self.field(props, "Meshes")?;
            model.meshes_expr = self.path_expr(value)?;
            let value = self.field(props, "Materials")?;
            model.materials_expr = self.path_expr(value)?;
            let value = self.field(props, "Bones")?;
            model.bones_expr = self.path_expr(value)?;
            let value = self.field(props, "Texture")?;
            model.texture_expr = self.path_expr(value)?;
        }
        for (index, sprite) in decl.sprites.iter_mut().enumerate() {
            let props = self.raw(sprites, index + 1)?;
            let value = self.field(props, "Texture")?;
            if let Some(path) = self.path_expr(value)? {
                sprite.texture_expr = path;
            }
        }
        Ok(())
    }
    fn path_expr(&mut self, value: LuaValue) -> Result<Option<String>> {
        Ok(match value {
            LuaValue::Nil => None,
            LuaValue::String(_) => Some(format!("{:?}", self.text(value)?)),
            LuaValue::Table(_) => {
                let button = self.field(value, "load_button")?;
                let button = self.text(button)?;
                let element = self.field(value, "load_element")?;
                let element = self.text(element)?;
                Some(format!("NOTESKIN:GetPath({button:?},{element:?})"))
            }
            _ => return Err("invalid noteskin resource path".into()),
        })
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone)]
enum Binding {
    Constant,
    Identity,
    Actor,
    Sprite,
    LoadActor,
    Var,
    Path,
    Capture,
    ColorVar,
    Command(&'static str),
    Metric(Arc<noteskin_itg::NoteskinData>),
    LoadFile(Arc<noteskin_itg::NoteskinData>),
}
type HostResult = std::result::Result<(), Error>;
fn deny_loadlib(call: &mut HostCall<'_>) -> HostResult {
    Err(call.error(b"package.loadlib is disabled in safe mode"))
}
fn argument(call: &HostCall<'_>, index: usize) -> LuaValue {
    call.arguments()
        .get(index)
        .copied()
        .unwrap_or(LuaValue::Nil)
}
fn host_table(
    call: &HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<deadlib_lua::TableId, Error> {
    match value {
        LuaValue::Table(id) => Ok(id),
        _ => Err(call.error(b"noteskin table expected")),
    }
}
fn host_text(call: &mut HostCall<'_>, value: LuaValue) -> std::result::Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"noteskin string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live string"))
        .map(str::to_owned)
        .map_err(|_| call.error(b"invalid UTF-8 noteskin string"))
}
fn optional_host_text(
    call: &mut HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<Option<String>, Error> {
    if value == LuaValue::Nil {
        Ok(None)
    } else {
        host_text(call, value).map(Some)
    }
}
fn field(
    call: &mut HostCall<'_>,
    target: LuaValue,
    name: &str,
) -> std::result::Result<LuaValue, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(target, key)
}
fn put(call: &mut HostCall<'_>, target: LuaValue, name: &str, value: LuaValue) -> HostResult {
    let key = call.try_intern(name.as_bytes())?;
    call.table_set(target, key, value)
}
fn get_field(call: &mut HostCall<'_>) -> HostResult {
    let value = call.table_get(argument(call, 0), argument(call, 1))?;
    call.push(value)
}
fn set_field(call: &mut HostCall<'_>) -> HostResult {
    call.table_set(argument(call, 0), argument(call, 1), argument(call, 2))
}
fn make_actor(call: &mut HostCall<'_>) -> HostResult {
    let actor = LuaValue::Table(call.try_new_table()?);
    call.push(actor)?;
    put(call, actor, "__blank", argument(call, 0))?;
    for (index, name) in [(1, "__load_button"), (2, "__load_element")] {
        let value = argument(call, index);
        if value != LuaValue::Nil {
            put(call, actor, name, value)?;
        }
    }
    let mt = call.captures()[0];
    let setmetatable = call.captures()[1];
    call.call(setmetatable, &[actor, mt])?;
    Ok(())
}
fn deferred_actor(call: &mut HostCall<'_>) -> HostResult {
    let captures = [call.captures()[0], call.captures()[1], call.captures()[2]];
    call.call_and_return(
        captures[0],
        &[LuaValue::Boolean(false), captures[1], captures[2]],
    )
}
fn path_parts(
    call: &mut HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<(Option<String>, Option<String>), Error> {
    match value {
        LuaValue::String(_) => Ok((None, Some(host_text(call, value)?))),
        LuaValue::Table(_) => {
            let button = field(call, value, "load_button")?;
            let button = optional_host_text(call, button)?;
            let element = field(call, value, "load_element")?;
            Ok((button, optional_host_text(call, element)?))
        }
        _ => Ok((None, None)),
    }
}
fn lua_text(call: &mut HostCall<'_>, value: Option<&str>) -> std::result::Result<LuaValue, Error> {
    value.map_or(Ok(LuaValue::Nil), |text| call.try_intern(text.as_bytes()))
}
fn dispatch(call: &mut HostCall<'_>) -> HostResult {
    let binding = call
        .function_data::<Binding>()
        .expect("noteskin binding")
        .clone();
    let first = argument(call, 0);
    match binding {
        Binding::Constant => call.push(call.captures()[0]),
        Binding::Identity => {
            host_table(call, first)?;
            call.push(first)
        }
        Binding::Actor => call.call_and_return(
            call.captures()[0],
            &[LuaValue::Boolean(true), LuaValue::Nil, LuaValue::Nil],
        ),
        Binding::LoadActor => {
            let (button, element) = path_parts(call, first)?;
            let blank = element
                .as_deref()
                .is_some_and(|text| text.eq_ignore_ascii_case("_blank"));
            let button = lua_text(call, button.as_deref())?;
            let element = lua_text(call, element.as_deref())?;
            call.call_and_return(
                call.captures()[0],
                &[LuaValue::Boolean(blank), button, element],
            )
        }
        Binding::Sprite => {
            let make = call.captures()[0];
            let load = call.captures()[1];
            let texture = if matches!(first, LuaValue::Table(_)) {
                field(call, first, "Texture")?
            } else {
                LuaValue::Nil
            };
            let actor = if texture == LuaValue::Nil {
                call.call(
                    make,
                    &[LuaValue::Boolean(false), LuaValue::Nil, LuaValue::Nil],
                )?[0]
            } else {
                call.call(load, &[texture])?[0]
            };
            call.push(actor)?;
            if matches!(first, LuaValue::Table(_)) {
                for name in [
                    "InitCommand",
                    "BaseRotationX",
                    "BaseRotationY",
                    "BaseRotationZ",
                ] {
                    let value = field(call, first, name)?;
                    if value != LuaValue::Nil {
                        put(call, actor, name, value)?;
                    }
                }
            }
            Ok(())
        }
        Binding::Var => {
            let name = host_text(call, first)?;
            let globals = call.captures()[0];
            let value = match name.as_str() {
                "Button" => field(call, globals, "__itg_button")?,
                "Element" => field(call, globals, "__itg_element")?,
                "Color" => {
                    let value = field(call, globals, "__itg_color")?;
                    let text = optional_host_text(call, value)?.unwrap_or_else(|| "4th".into());
                    call.try_intern(text.as_bytes())?
                }
                "SpriteOnly" => {
                    let value = field(call, globals, "__itg_sprite_only").unwrap_or(LuaValue::Nil);
                    LuaValue::Boolean(!matches!(value, LuaValue::Nil | LuaValue::Boolean(false)))
                }
                "Player" => LuaValue::Number(0.),
                _ => LuaValue::Nil,
            };
            call.push(value)
        }
        Binding::Path => {
            host_table(call, first)?;
            let button = host_text(call, argument(call, 1))?;
            let element = host_text(call, argument(call, 2))?;
            let path = LuaValue::Table(call.try_new_table()?);
            call.push(path)?;
            let button = call.try_intern(button.as_bytes())?;
            put(call, path, "load_button", button)?;
            let element = call.try_intern(element.as_bytes())?;
            put(call, path, "load_element", element)
        }
        Binding::Metric(data) => {
            host_table(call, first)?;
            let section = host_text(call, argument(call, 1))?;
            let key = host_text(call, argument(call, 2))?;
            let value = data
                .metrics
                .get(&section, &key)
                .or_else(|| data.metrics.get("NoteDisplay", &key))
                .unwrap_or("");
            let value = call.try_intern(value.as_bytes())?;
            call.push(value)
        }
        Binding::Command(command) => {
            host_table(call, first)?;
            let mut text = command.to_owned();
            for value in call.arguments().iter().skip(1).copied() {
                text.push(',');
                match value {
                    LuaValue::Boolean(value) => text.push_str(if value { "true" } else { "false" }),
                    LuaValue::Number(value) => text.push_str(&value.to_string()),
                    LuaValue::String(id) => text.push_str(
                        std::str::from_utf8(call.string_bytes(id).expect("live argument"))
                            .map_err(|_| call.error(b"invalid UTF-8 noteskin string"))?,
                    ),
                    _ => {}
                }
            }
            let mut commands = field(call, first, "__loader_commands")?;
            if commands == LuaValue::Nil {
                commands = LuaValue::Table(call.try_new_table()?);
                put(call, first, "__loader_commands", commands)?;
            }
            let id = host_table(call, commands)?;
            let index = call.raw_table_len(id)? + 1;
            let text = call.try_intern(text.as_bytes())?;
            call.raw_table_set(id, LuaValue::Number(index as f64), text)?;
            call.push(first)
        }
        Binding::LoadFile(data) => {
            let (button, element) = match first {
                LuaValue::Table(_) => {
                    let (button, element) = path_parts(call, first)?;
                    let exists = element
                        .as_deref()
                        .and_then(|element| {
                            data.resolve_path(button.as_deref().unwrap_or(""), element)
                        })
                        .is_some_and(|path| path_is_lua(&path));
                    if !exists {
                        return call.push(LuaValue::Nil);
                    }
                    (button, element)
                }
                LuaValue::String(_) => {
                    if !path_is_lua(Path::new(&host_text(call, first)?)) {
                        return call.push(LuaValue::Nil);
                    }
                    (None, None)
                }
                _ => return call.push(LuaValue::Nil),
            };
            let button = lua_text(call, button.as_deref())?;
            let element = lua_text(call, element.as_deref())?;
            let function =
                call.new_host_function(deferred_actor, &[call.captures()[0], button, element])?;
            call.push(function)
        }
        Binding::Capture => {
            host_table(call, first)?;
            let output = host_table(call, call.captures()[0])?;
            let index = call.raw_table_len(output)? + 1;
            call.raw_table_set(output, LuaValue::Number(index as f64), first)?;
            call.push(first)
        }
        Binding::ColorVar => match host_text(call, first)?.as_str() {
            "Color" => call.push(call.captures()[1]),
            "Button" => call.push(call.captures()[2]),
            _ => call.call_and_return(call.captures()[0], &[first]),
        },
    }
}
