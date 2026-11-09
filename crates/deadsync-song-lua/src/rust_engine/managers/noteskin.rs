//! Song-side skin lookup and templates, using the application's existing resolver.
use super::fields::{array, object, set};
use super::*;
use crate::{SongLuaCompileContext, SongLuaNoteskinResolver};
use std::{path::Path, sync::Arc};

// Only Lua values in traced captures; resolver/configuration data owns no handles.
const GLOBALS: usize = 0;
const FILE: usize = 1;
const SOURCE: usize = 2;
const TEMPLATE: usize = 3;
const METATABLE: usize = 4;

#[derive(Clone, Copy)]
enum Op {
    Metric,
    Float,
    Integer,
    Boolean,
    Command,
    Path,
    Actor,
    Exists,
    Names,
}
#[derive(Clone)]
struct Binding {
    resolver: SongLuaNoteskinResolver,
    skin: Arc<str>,
    local_names: Arc<[String]>,
    operation: Op,
    explicit_skin: bool,
}

pub(in crate::rust_engine) fn install(
    vm: &mut Vm,
    context: &SongLuaCompileContext,
    resolver: SongLuaNoteskinResolver,
    file: Value,
    source: Value,
    template: Value,
) -> Result<(), Error> {
    let captures = [
        vm.get_global(b"_G")?,
        file,
        source,
        template,
        vm.get_global(b"setmetatable")?,
    ];
    // The native manager snapshots song-local names once, before sampled callbacks.
    let local_names = std::fs::read_dir(&context.song_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("metrics.ini").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>()
        .into();
    let initialize = vm.new_host_function_with_data(
        initialize,
        &captures,
        Binding {
            resolver,
            skin: crate::song_lua_default_noteskin_name(context).into(),
            local_names,
            operation: Op::Names,
            explicit_skin: false,
        },
    )?;
    vm.call(initialize, &[]).map(|_| ())
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap().clone();
    let owner = skin_table(call, &binding)?;
    let globals = table(call, call.captures()[GLOBALS])?;
    set(globals, "NOTESKIN", owner, call)
}

fn skin_table(call: &mut HostCall<'_>, binding: &Binding) -> Result<TableId, Error> {
    object(call, |call, owner| {
        for (name, operation) in [
            ("GetMetric", Op::Metric),
            ("GetMetricF", Op::Float),
            ("GetMetricI", Op::Integer),
            ("GetMetricB", Op::Boolean),
            ("GetMetricA", Op::Command),
            ("GetPath", Op::Path),
            ("LoadActor", Op::Actor),
            ("DoesNoteSkinExist", Op::Exists),
            ("GetNoteSkinNames", Op::Names),
        ] {
            for explicit_skin in [false, true] {
                if explicit_skin && matches!(operation, Op::Exists | Op::Names) {
                    continue;
                }
                let captures = match operation {
                    Op::Actor => call.captures().to_vec(),
                    Op::Command => vec![call.captures()[SOURCE]],
                    _ => Vec::new(),
                };
                let function = call.new_host_function_with_data(
                    method,
                    &captures,
                    Binding {
                        operation,
                        explicit_skin,
                        ..binding.clone()
                    },
                )?;
                let name = if explicit_skin {
                    format!("{name}ForNoteSkin")
                } else {
                    name.into()
                };
                set(owner, name, function, call)?;
            }
        }
        for name in ["HasVariants", "IsNoteSkinVariant"] {
            let function = call.new_host_function(super::constant, &[Value::Boolean(false)])?;
            set(owner, name, function, call)?;
        }
        let function = call.new_host_function(empty, &[])?;
        set(owner, "GetVariantNamesForNoteSkin", function, call)
    })
}

fn empty(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = call.try_new_table()?;
    call.push(Value::Table(result))
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live string"))
        .map(str::to_owned)
        .map_err(|e| call.error(e.to_string().as_bytes()))
}
fn argument(call: &mut HostCall<'_>, index: usize) -> Result<String, Error> {
    text(
        call,
        call.arguments().get(index).copied().unwrap_or(Value::Nil),
    )
}
fn function(call: &HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"function expected"))
    }
}

fn method(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap().clone();
    let resolver = binding.resolver;
    if matches!(binding.operation, Op::Names) {
        let result = array(call, resolver.names())?;
        return call.push(Value::Table(result));
    }
    // These are typed native methods: the first argument must always be a table.
    table(
        call,
        call.arguments().first().copied().unwrap_or(Value::Nil),
    )?;
    let first = argument(call, 1)?;
    if matches!(binding.operation, Op::Exists) {
        return call.push(Value::Boolean(
            binding
                .local_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&first))
                || resolver.exists(&first),
        ));
    }
    let second = argument(call, 2)?;
    let skin = if binding.explicit_skin {
        argument(call, 3)?
    } else {
        binding.skin.to_string()
    };
    let result = match binding.operation {
        Op::Metric => match resolver.metric(&skin, &first, &second) {
            Some(value) => call.try_intern(value.as_bytes())?,
            None => Value::Nil,
        },
        Op::Float => Value::Number(resolver.metric_f(&skin, &first, &second).unwrap_or(0.) as f64),
        Op::Integer => Value::Number(resolver.metric_i(&skin, &first, &second) as f64),
        Op::Boolean => Value::Boolean(resolver.metric_b(&skin, &first, &second).unwrap_or(false)),
        Op::Path => call.try_intern(resolver.path_string(&skin, &first, &second).as_bytes())?,
        Op::Command => {
            let raw = resolver.metric(&skin, &first, &second).unwrap_or_default();
            let source = if let Some(source) = raw.strip_prefix('%') {
                format!("return {source}")
            } else {
                crate::preprocess_lua_cmd_syntax(&format!("return cmd({raw})"))
                    .map_err(|e| call.error(e.as_bytes()))?
            };
            let source = call.try_intern(source.as_bytes())?;
            let loaded = call.call(call.captures()[0], &[source])?;
            let value = loaded.first().copied().unwrap_or(Value::Nil);
            if value == Value::Nil {
                let error = loaded.get(1).copied().unwrap_or(Value::Nil);
                return Err(call.raise(error));
            }
            let result = call
                .call(value, &[])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            function(call, result)?
        }
        Op::Actor => actor(call, &binding, &skin, &first, &second)?,
        Op::Exists | Op::Names => unreachable!(),
    };
    call.push(result)
}

fn actor(
    call: &mut HostCall<'_>,
    binding: &Binding,
    skin: &str,
    button: &str,
    element: &str,
) -> Result<Value, Error> {
    let resolved = binding.resolver.resolve_path(skin, button, element);
    let actor = if let Some(path) = resolved.as_ref().filter(|path| {
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    }) {
        let env = object(call, |call, env| {
            let meta = object(call, |call, meta| {
                set(meta, "__index", call.captures()[GLOBALS], call)
            })?;
            call.call(
                call.captures()[METATABLE],
                &[Value::Table(env), Value::Table(meta)],
            )?;
            let manager = skin_table(
                call,
                &Binding {
                    skin: skin.into(),
                    local_names: Arc::from([]),
                    ..binding.clone()
                },
            )?;
            set(env, "NOTESKIN", manager, call)?;
            let fallback = super::super::field(call, call.captures()[GLOBALS], "Var")?;
            function(call, fallback)?;
            let var = call.new_host_function_with_data(
                variable,
                &[fallback],
                [button.to_owned(), element.to_owned()],
            )?;
            set(env, "Var", var, call)
        })?;
        call.with_roots(&[Value::Table(env)], |call| {
            // Resolver paths are exact files, not another song-relative search.
            let path =
                std::path::absolute(path).map_err(|e| call.error(e.to_string().as_bytes()))?;
            let path = call.try_intern(crate::file_path_string(&path).as_bytes())?;
            let loader = call.call(call.captures()[FILE], &[path, Value::Table(env)])?[0];
            let result = call
                .call(loader, &[])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            table(call, result)?;
            Ok(result)
        })?
    } else {
        let image = resolved
            .as_ref()
            .is_some_and(|path| crate::is_song_lua_image_path(path));
        let declaration = object(call, |call, declaration| {
            set(
                declaration,
                "Class",
                if image { "Sprite" } else { "Actor" },
                call,
            )
        })?;
        let result = call.call(call.captures()[TEMPLATE], &[Value::Table(declaration)])?[0];
        call.with_roots(&[result], |call| {
            if image {
                let path = crate::file_path_string(resolved.as_deref().unwrap_or(Path::new("")));
                let path = call.try_intern(path.as_bytes())?;
                super::super::set(call, result, "Texture", path)?;
            }
            Ok(result)
        })?
    };
    call.with_roots(&[actor], |call| {
        for (key, value) in [
            ("__songlua_noteskin_name", skin.trim().to_ascii_lowercase()),
            ("__songlua_noteskin_button", button.to_owned()),
            ("__songlua_noteskin_element", element.to_owned()),
        ] {
            let value = call.try_intern(value.as_bytes())?;
            super::super::set(call, actor, key, value)?;
        }
        Ok(actor)
    })
}

fn variable(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = argument(call, 0)?;
    let values = call.function_data::<[String; 2]>().unwrap();
    let selected = match name.as_str() {
        "Button" => Some(&values[0]),
        "Element" => Some(&values[1]),
        _ => None,
    };
    let result = if let Some(value) = selected {
        let value = value.clone();
        call.try_intern(value.as_bytes())?
    } else {
        let name = call.try_intern(name.as_bytes())?;
        call.call(call.captures()[0], &[name])?
            .first()
            .copied()
            .unwrap_or(Value::Nil)
    };
    call.push(result)
}
