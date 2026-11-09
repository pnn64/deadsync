//! Original class-lookup differential fixture on the Rust VM.
use deadlib_lua::{Error, HostCall, Value, Vm};
use deadsync_song_lua::{SongLuaCompileContext, SongRuntime};
#[path = "support/baked_program.rs"]
mod api;
fn raw(call: &mut HostCall<'_>, owner: deadlib_lua::TableId, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_get(owner, key)
}
fn original(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(actor) = call.arguments()[0] else {
        return Err(call.error(b"actor table expected"));
    };
    let key = call.arguments()[1];
    let Value::String(name) = key else {
        return call.push(Value::Nil);
    };
    let name = std::str::from_utf8(call.string_bytes(name).unwrap())
        .map_err(|e| call.error(e.to_string().as_bytes()))?
        .to_owned();
    let kind = raw(call, actor, "__songlua_actor_type")?;
    let kind = if kind == Value::Nil {
        "Actor".into()
    } else {
        let id = call
            .coerce_string(kind)?
            .ok_or_else(|| call.error(b"actor type must be a string"))?;
        std::str::from_utf8(call.string_bytes(id).unwrap())
            .map_err(|e| call.error(e.to_string().as_bytes()))?
            .to_owned()
    };
    let class_key = call.try_intern(class_name(&kind).as_bytes())?;
    let class = call.table_get(call.captures()[0], class_key)?;
    if class != Value::Nil {
        if !matches!(class, Value::Table(_)) {
            return Err(call.error(b"actor table expected"));
        }
        let value = call.table_get(class, key)?;
        if value != Value::Nil {
            return call.push(value);
        }
        if is_core(&kind) {
            return call.push(Value::Nil);
        }
    }
    if !is_frame(&kind)
        && matches!(
            name.as_str(),
            "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
        )
    {
        return call.push(Value::Nil);
    }
    let methods = raw(call, actor, "__songlua_bound_methods")?;
    let result = match methods {
        Value::Nil => Value::Nil,
        Value::Table(methods) => call.raw_table_get(methods, key)?,
        _ => return Err(call.error(b"actor method table expected")),
    };
    call.push(result)
}
fn result(vm: &mut Vm, function: Value, args: &[Value]) -> Result<Value, String> {
    match vm.call(function, args) {
        Ok(values) => {
            let value = values[0];
            vm.pin(value).unwrap();
            Ok(value)
        }
        Err(error) => Err(error
            .value()
            .and_then(|v| {
                if let Value::String(id) = v {
                    vm.string_bytes(id)
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| error.to_string())),
    }
}
fn compare(vm: &mut Vm, original: Value, lookup: Value, actor: Value, key: Value) {
    vm.set_global(b"LOOKUPS", Value::Number(0.)).unwrap();
    let expected = result(vm, original, &[actor, key]);
    let calls = vm.get_global(b"LOOKUPS").unwrap();
    vm.set_global(b"LOOKUPS", Value::Number(0.)).unwrap();
    let actual = result(vm, lookup, &[actor, key]);
    assert_eq!(actual, expected);
    assert_eq!(vm.get_global(b"LOOKUPS").unwrap(), calls);
}
#[test]
fn class_name_reads_preserve_lookup_hooks_identity_and_errors() {
    let mut host = SongRuntime::new(&SongLuaCompileContext::new(".", "class lookup unit")).unwrap();
    let declaration = api::eval(host.vm_mut(), "return Def.Actor{}")[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let instance = host.actor_scene_root(scene).unwrap();
    host.vm_mut()
        .set_global(b"_CLASS_LOOKUP_INSTANCE", instance)
        .unwrap();
    let lookup = api::eval(
        host.vm_mut(),
        "return getmetatable(_CLASS_LOOKUP_INSTANCE).__index",
    )[0];
    assert_eq!(
        deadsync_song_lua::test_support::actor_method_identity(&host, lookup),
        Some(deadsync_song_lua::test_support::ActorMethodIdentity::Lookup)
    );
    let vm = host.vm_mut();
    let globals = vm.get_global(b"_G").unwrap();
    let original = vm.new_host_function(original, &[globals]).unwrap();
    vm.pin(original).unwrap();
    api::eval(vm, ORIGINAL_CLASS_SETUP);
    let actor = api::table(vm);
    let methods = api::table(vm);
    api::set(vm, methods, b"fallback", Value::Number(91.));
    api::set(vm, methods, b"SetUpdateFunction", Value::Number(92.));
    api::set(vm, actor, b"__songlua_bound_methods", methods);
    let kinds = [
        Value::Nil,
        api::string(vm, "Actor"),
        api::string(vm, "ActorFrame"),
        api::string(vm, "Quad"),
        api::string(vm, "GraphDisplay"),
        api::string(vm, "custom".repeat(40)),
        api::string(vm, [0xff]),
        Value::Number(42.),
        Value::Boolean(false),
    ];
    for kind in kinds {
        api::set(vm, actor, b"__songlua_actor_type", kind);
        for key in [
            "base",
            "own",
            "disabled",
            "fallback",
            "missing",
            "SetUpdateFunction",
        ] {
            let key = api::string(vm, key);
            compare(vm, original, lookup, actor, key)
        }
        compare(vm, original, lookup, actor, Value::Number(42.));
        let key = api::string(vm, [0xff]);
        compare(vm, original, lookup, actor, key);
    }
    api::set_string(vm, actor, b"__songlua_actor_type", "ActorFrame");
    api::eval(vm, ORIGINAL_CLASS_HOOKS);
    for key in ["base", "own", "missing", "hook", "fault"] {
        let key = api::string(vm, key);
        compare(vm, original, lookup, actor, key)
    }
    api::eval(vm, ORIGINAL_CLASS_REBOUND);
    for key in ["base", "own", "missing", "hook", "fault"] {
        let key = api::string(vm, key);
        compare(vm, original, lookup, actor, key)
    }
    api::set_metatable(vm, globals, Value::Nil);
    for value in [Value::Nil, Value::Boolean(false)] {
        vm.set_global(b"ActorFrame", value).unwrap();
        let key = api::string(vm, "fallback");
        compare(vm, original, lookup, actor, key);
    }
}

const ORIGINAL_CLASS_SETUP: &str = r#"
            LOOKUPS = 0
            Actor = {base = 17, disabled = false}
            ActorFrame = setmetatable({own = 42}, {__index = Actor})
            Sprite = setmetatable({}, {__index = ActorFrame})
        "#;
const ORIGINAL_CLASS_HOOKS: &str = r#"
            setmetatable(Actor, {__index = function(_, name)
                LOOKUPS = LOOKUPS + 1
                if name == 'fault' then error('inherited method fault') end
                if name == 'hook' then return false end
            end})
        "#;
const ORIGINAL_CLASS_REBOUND: &str = r#"
            CLASS = ActorFrame
            ActorFrame = nil
            setmetatable(_G, { __index = function(_, name)
                LOOKUPS = LOOKUPS + 1
                if name == 'ActorFrame' then return CLASS end
            end })
        "#;

fn class_name(kind: &str) -> &'static str {
    match kind {
        "Sprite" | "Quad" => "Sprite",
        "Banner" => "Banner",
        "ActorFrameTexture" => "ActorFrameTexture",
        "ActorMultiVertex" => "ActorMultiVertex",
        "ActorProxy" => "ActorProxy",
        "BitmapText" => "BitmapText",
        "Player" => "Player",
        // Player inherits ActorFrame in native Player.h; gameplay host actors
        // use this internal kind before the complete Player inventory is linked.
        "PlayerActor" => "ActorFrame",
        "NoteField" => "NoteField",
        kind if is_frame(kind) => "ActorFrame",
        _ => "Actor",
    }
}

fn is_frame(kind: &str) -> bool {
    matches!(
        kind,
        "ActorFrame"
            | "ActorFrameTexture"
            | "TopScreen"
            | "Player"
            | "PlayerActor"
            | "HoldJudgment"
            | "NoteField"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "MeterDisplay"
            | "CourseContentsList"
            | "StepsDisplay"
            | "PercentageDisplay"
            | "WrapperState"
    )
}

fn is_core(kind: &str) -> bool {
    matches!(
        kind,
        "Actor"
            | "ActorFrame"
            | "Sprite"
            | "Quad"
            | "Banner"
            | "ActorFrameTexture"
            | "ActorMultiVertex"
            | "ActorProxy"
            | "BitmapText"
            | "WrapperState"
    )
}
