//! Song file loading through the shared Rust VM. Actor/media loading is separate.
use deadlib_lua::{Error, ErrorKind, HostCall, IndexView, Root, TableId, Value, Vm};
use std::{
    fs,
    path::{Path, PathBuf},
};

const GLOBALS: usize = 0;
const LOADSTRING: usize = 1;
const GETFENV: usize = 2;
const SETFENV: usize = 3;
const SETMETATABLE: usize = 4;
const REGISTRY: usize = 5;
const TARGET: &str = "__songlua_env_target";
const CONTEXT: [&str; 3] = [
    "__songlua_script_dir",
    "__songlua_current_script_path",
    "__songlua_current_chunk_env",
];

#[derive(Clone, Copy)]
enum Kind {
    File,
    DoFile,
    Entry,
    String,
}

struct Binding {
    song_dir: PathBuf,
    kind: Kind,
}

pub(super) fn install(vm: &mut Vm, song_dir: &Path) -> Result<Root, Error> {
    let globals = vm.get_global(b"_G")?;
    let loadstring = vm.get_global(b"loadstring")?;
    let getfenv = vm.get_global(b"getfenv")?;
    let Value::Table(debug) = vm.get_global(b"debug")? else {
        unreachable!("fresh debug library")
    };
    let key = vm.try_intern(b"setfenv")?;
    let setfenv = vm.raw_table_get(debug, key)?;
    let setmetatable = vm.get_global(b"setmetatable")?;
    let registry = Value::Table(vm.try_new_table()?);
    let registry_root = vm.pin(registry)?;
    let captures = [
        globals,
        loadstring,
        getfenv,
        setfenv,
        setmetatable,
        registry,
    ];
    // Pin captured primitives until all replacements are installed.
    let keeper = vm.new_host_function(set_environment, &captures)?;
    let keeper_root = vm.pin(keeper)?;
    let result = (|| {
        // Environment retargeting must not keep discarded loader closures alive.
        // Lua owns a saved wrapper through ordinary references and captures.
        let mt = vm.try_new_table()?;
        let mode = vm.try_intern(b"k")?;
        super::put(vm, mt, "__mode", mode)?;
        vm.call(setmetatable, &[registry, Value::Table(mt)])?;
        for (name, kind) in [
            ("loadfile", Kind::File),
            ("dofile", Kind::DoFile),
            ("loadstring", Kind::String),
        ] {
            let function = vm.new_host_function_with_data(
                load,
                &captures,
                Binding {
                    song_dir: song_dir.to_path_buf(),
                    kind,
                },
            )?;
            vm.set_global(name.as_bytes(), function)?;
        }
        vm.set_global(b"setfenv", keeper)?;
        let entry = vm.new_host_function_with_data(
            load,
            &captures,
            Binding {
                song_dir: song_dir.to_path_buf(),
                kind: Kind::Entry,
            },
        )?;
        vm.pin(entry)
    })();
    vm.unpin(keeper_root)?;
    vm.unpin(registry_root)?;
    result
}

fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live coerced string"))
        .map(str::to_owned)
        .map_err(|error| call.error(error.to_string().as_bytes()))
}

fn load(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().expect("loader binding");
    let kind = binding.kind;
    let song_dir = binding.song_dir.clone();
    // Argument conversion precedes loadfile's recoverable file/compile errors.
    let path = text(
        call,
        call.arguments().first().copied().unwrap_or(Value::Nil),
    )?;
    if matches!(kind, Kind::String) {
        let name = call.arguments().get(1).copied().unwrap_or(Value::Nil);
        let source = crate::cmd::preprocess_lua_cmd_syntax_cow(&path)
            .map_err(|error| call.error(error.as_bytes()))?;
        let source = call.try_intern(source.as_bytes())?;
        let function = compile(call, source, name)?;
        return call.push(function);
    }
    // A compiler entry already has an exact file path and an authored source
    // spelling. It must not be searched again or have whitespace trimmed.
    let exact = matches!(kind, Kind::Entry)
        && call
            .arguments()
            .get(2)
            .is_some_and(|value| *value != Value::Nil);
    let loaded = load_file(
        call,
        &song_dir,
        if exact { &path } else { path.trim() },
        kind,
    );
    match (kind, loaded) {
        (Kind::File, Ok(function)) => {
            call.push(function)?;
            call.push(Value::Nil)
        }
        (Kind::File, Err(error))
            if error.kind() != ErrorKind::Limit && error.kind() != ErrorKind::Memory =>
        {
            let message = call.try_intern(error.to_string().as_bytes())?;
            call.push(Value::Nil)?;
            call.push(message)
        }
        (Kind::DoFile, Ok(function)) => {
            // Existing song dofile returns only the first value.
            let first = call
                .call(function, &[])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            call.push(first)
        }
        (_, Ok(function)) => call.push(function),
        (_, Err(error)) => Err(error),
    }
}

fn compile(call: &mut HostCall<'_>, source: Value, name: Value) -> Result<Value, Error> {
    let loader = call.captures()[LOADSTRING];
    let values = call.call(loader, &[source, name])?;
    if let Some(&function @ (Value::Function(_) | Value::Native(_))) = values.first() {
        return Ok(function);
    }
    let error = values.get(1).copied().unwrap_or(Value::Nil);
    Err(call.raise(error))
}

fn resolve(call: &mut HostCall<'_>, song_dir: &Path, path: &str) -> Result<PathBuf, Error> {
    let raw = Path::new(path);
    if raw.is_absolute() && raw.exists() {
        return Ok(raw.to_path_buf());
    }
    let current = super::field(call, call.captures()[GLOBALS], CONTEXT[0])?;
    if current != Value::Nil {
        let current = text(call, current)?;
        if !current.trim().is_empty() {
            let candidate = Path::new(&current).join(path);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    let candidate = song_dir.join(path);
    if candidate.exists() {
        return Ok(candidate);
    }
    if raw.exists() {
        return Ok(raw.to_path_buf());
    }
    Err(call.error(
        format!(
            "script '{path}' not found relative to '{}'",
            song_dir.display()
        )
        .as_bytes(),
    ))
}

fn load_file(
    call: &mut HostCall<'_>,
    song_dir: &Path,
    path: &str,
    kind: Kind,
) -> Result<Value, Error> {
    let source = call.arguments().get(2).copied().unwrap_or(Value::Nil);
    let source_path = if matches!(kind, Kind::Entry) && source != Value::Nil {
        Some(PathBuf::from(text(call, source)?))
    } else {
        None
    };
    let path = if source_path.is_some() {
        PathBuf::from(path)
    } else {
        resolve(call, song_dir, path)?
    };
    let explicit_source = source_path.is_some();
    let source_path = source_path.unwrap_or_else(|| path.clone());
    let globals = call.captures()[GLOBALS];
    if !matches!(kind, Kind::Entry)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("easing.lua"))
        && !path.is_file()
    {
        let ease = super::field(call, globals, "ease")?;
        if !matches!(ease, Value::Table(_)) {
            return Err(call.error(b"ease: table expected"));
        }
        return call.new_host_function(return_capture, &[ease]);
    }
    let explicit_env = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    let env = if matches!(kind, Kind::Entry) && explicit_env != Value::Nil {
        if !matches!(explicit_env, Value::Table(_)) {
            return Err(call.error(b"environment: table expected"));
        }
        explicit_env
    } else if matches!(kind, Kind::File) {
        let getfenv = call.captures()[GETFENV];
        call.call(getfenv, &[Value::Number(2.0)])?
            .first()
            .copied()
            .unwrap_or(globals)
    } else if source_path.file_name().and_then(|name| name.to_str()) == Some("std.lua")
        && source_path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            == Some("template")
    {
        match super::field(call, globals, "xero")? {
            Value::Nil => globals,
            table @ Value::Table(_) => table,
            _ => return Err(call.error(b"xero: table expected")),
        }
    } else {
        globals
    };
    call.with_roots(&[env], |call| {
        let source =
            fs::read_to_string(&path).map_err(|error| call.error(error.to_string().as_bytes()))?;
        let source = crate::cmd::preprocess_lua_cmd_syntax_cow(&source)
            .map_err(|error| call.error(error.as_bytes()))?;
        let source = call.try_intern(source.as_bytes())?;
        // Compiler entries preserve the production wrapper's exact source
        // name. Other actor-file entry points retain their native @ marker.
        let spelling = source_path.to_string_lossy();
        let name = if explicit_source || spelling.starts_with(['@', '=']) {
            spelling.into_owned()
        } else {
            format!("@{spelling}")
        };
        let name = call.try_intern(name.as_bytes())?;
        let inner = compile(call, source, name)?;
        call.with_roots(&[inner], |call| {
            let script_path = call.try_intern(crate::file_path_string(&source_path).as_bytes())?;
            let script_dir = call.try_intern(
                source_path
                    .parent()
                    .unwrap_or(song_dir)
                    .to_string_lossy()
                    .as_bytes(),
            )?;
            let captures = call.captures().to_vec();
            let finish = call.new_host_function(finish_load, &captures)?;
            Ok(call.call(finish, &[inner, env, script_dir, script_path])?[0])
        })
    })
}

fn return_capture(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])
}

fn finish_load(call: &mut HostCall<'_>) -> Result<(), Error> {
    let [inner, env, dir, path] = call.arguments() else {
        unreachable!("loader arguments")
    };
    let (inner, env, dir, path) = (*inner, *env, *dir, *path);
    let globals = call.captures()[GLOBALS];
    let proxy = call.try_new_table()?;
    let target = call.try_intern(TARGET.as_bytes())?;
    call.raw_table_set(proxy, target, env)?;
    call.with_roots(&[Value::Table(proxy)], |call| {
        let mt = call.try_new_table()?;
        for (name, callback) in [
            ("__index", env_index as deadlib_lua::HostFunction),
            ("__newindex", env_newindex),
        ] {
            let captures = [Value::Table(proxy), globals];
            let function = if name == "__index" {
                // A literal's string lives as long as the VM, so the resolver
                // can hold it without a root.
                let keys = EnvKeys {
                    target: call.intern_static(TARGET.as_bytes())?,
                };
                let function = call.new_host_function_with_data(callback, &captures, keys)?;
                call.set_index_resolver(function, resolve_env_index)?;
                function
            } else {
                call.new_host_function(callback, &captures)?
            };
            let key = call.try_intern(name.as_bytes())?;
            call.raw_table_set(mt, key, function)?;
        }
        let setmetatable = call.captures()[SETMETATABLE];
        call.call(setmetatable, &[Value::Table(proxy), Value::Table(mt)])?;
        let setfenv = call.captures()[SETFENV];
        call.call(setfenv, &[inner, Value::Table(proxy)])?;
        let wrapper = call.new_host_function(
            invoke_file,
            &[globals, inner, dir, path, Value::Table(proxy)],
        )?;
        let Value::Table(registry) = call.captures()[REGISTRY] else {
            unreachable!("loader registry")
        };
        call.raw_table_set(registry, wrapper, Value::Table(proxy))?;
        call.push(wrapper)
    })
}

// What `resolve_env_index` needs and cannot make itself: the key of the
// proxy's target field.
struct EnvKeys {
    target: Value,
}

/// `env_index` for a read that consults nothing but tables: the proxy holds
/// a table target, and neither the target nor the globals reach a function
/// `__index` for this key. Every global read in song code comes through here.
/// Anything else is left to the handler, including its error for a target
/// that is not a table.
fn resolve_env_index(view: &IndexView<'_>, _: TableId, key: Value) -> Option<Value> {
    let keys = view.function_data::<EnvKeys>()?;
    let Value::Table(proxy) = view.captures()[0] else {
        return None;
    };
    let Value::Table(target) = view.get(proxy, keys.target)? else {
        return None;
    };
    match view.get(target, key)? {
        Value::Nil => match view.captures()[1] {
            Value::Table(globals) => view.get(globals, key),
            _ => None,
        },
        value => Some(value),
    }
}

fn env_index(call: &mut HostCall<'_>) -> Result<(), Error> {
    let target = super::field(call, call.captures()[0], TARGET)?;
    if !matches!(target, Value::Table(_)) {
        return Err(call.error(b"environment target: table expected"));
    }
    let key = call.arguments()[1];
    let value = call.table_get(target, key)?;
    let value = if value == Value::Nil {
        call.table_get(call.captures()[1], key)?
    } else {
        value
    };
    call.push(value)
}

fn env_newindex(call: &mut HostCall<'_>) -> Result<(), Error> {
    let key = call.arguments()[1];
    let value = call.arguments()[2];
    let target = super::field(call, call.captures()[0], TARGET)?;
    if !matches!(target, Value::Table(_)) {
        return Err(call.error(b"environment target: table expected"));
    }
    call.table_set(target, key, value)?;
    if let Value::String(name) = key
        && let Some(name) = call
            .string_bytes(name)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
        && crate::is_compile_global_name(name)
    {
        call.table_set(call.captures()[1], key, value)?;
    }
    Ok(())
}

fn set_environment(call: &mut HostCall<'_>) -> Result<(), Error> {
    let target = call.arguments().first().copied().unwrap_or(Value::Nil);
    let env = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    if !matches!(env, Value::Table(_)) {
        return Err(call.error(b"environment: table expected"));
    }
    match target {
        Value::Function(_) | Value::Native(_) => {
            let Value::Table(registry) = call.captures()[REGISTRY] else {
                unreachable!("loader registry")
            };
            let proxy = call.raw_table_get(registry, target)?;
            if proxy != Value::Nil {
                super::set(call, proxy, TARGET, env)?;
            }
            let setfenv = call.captures()[SETFENV];
            call.call(setfenv, &[target, env])?;
        }
        Value::Number(_) => {
            let proxy = super::field(call, call.captures()[GLOBALS], CONTEXT[2])?;
            if proxy != Value::Nil {
                super::set(call, proxy, TARGET, env)?;
            }
        }
        _ => {}
    }
    call.push(target)
}

fn invoke_file(call: &mut HostCall<'_>) -> Result<(), Error> {
    let arguments = call.arguments().to_vec();
    let results = context_scope(call, 0, &arguments)?;
    for value in results {
        call.push(value)?;
    }
    Ok(())
}

fn context_scope(
    call: &mut HostCall<'_>,
    index: usize,
    arguments: &[Value],
) -> Result<Vec<Value>, Error> {
    let globals = call.captures()[0];
    let key = CONTEXT[index];
    let previous = super::field(call, globals, key)?;
    call.with_roots(&[previous], |call| {
        super::set(call, globals, key, call.captures()[index + 2])?;
        let result = if index + 1 < CONTEXT.len() {
            context_scope(call, index + 1, arguments)
        } else {
            call.call(call.captures()[1], arguments)
                .map(<[Value]>::to_vec)
        };
        let roots = match &result {
            Ok(values) => values.clone(),
            Err(error) => error.value().into_iter().collect(),
        };
        call.with_roots(&roots, |call| {
            if result
                .as_ref()
                .is_err_and(|error| matches!(error.kind(), ErrorKind::Limit | ErrorKind::Memory))
            {
                // Fatal VM limits forbid further Lua reentry. Restore our own
                // bookkeeping without invoking authored metamethods.
                let Value::Table(globals) = globals else {
                    unreachable!("global table")
                };
                let key = call.try_intern(key.as_bytes())?;
                call.raw_table_set(globals, key, previous)?;
            } else {
                super::set(call, globals, key, previous)?;
            }
            result
        })
    })
}
