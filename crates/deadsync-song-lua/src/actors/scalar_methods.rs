//! Original setter identities and captured targets for existing input snapshots.
use super::*;
use smallvec::SmallVec;
use std::sync::Weak;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Setter {
    Visible,
    X,
    Y,
}
impl Setter {
    pub(super) const ALL: [Self; 3] = [Self::Visible, Self::X, Self::Y];
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|setter| setter.name() == name)
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::X => "x",
            Self::Y => "y",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Method {
    Lookup,
    Assign,
    Forward(Setter),
    // This is an opaque identity only. The function's traced captures own it.
    Bound(Setter, TableId),
}
#[derive(Default)]
pub(super) struct Methods {
    native: HashMap<u64, Weak<Method>>,
    next_sweep: usize,
}
impl Methods {
    fn insert(&mut self, function: Value, method: &Arc<Method>) {
        let Value::Native(id) = function else {
            unreachable!("Rust setter closure")
        };
        if self.native.len() >= self.next_sweep {
            self.native.retain(|_, method| method.strong_count() != 0);
            self.next_sweep = (self.native.len() * 2).max(256);
        }
        self.native.insert(id, Arc::downgrade(method));
    }
    pub(super) fn identity(&self, function: Value) -> Option<Method> {
        match function {
            Value::Native(id) => self.native.get(&id)?.upgrade().map(|m| *m),
            _ => None,
        }
    }
}
#[derive(Clone)]
struct Binding {
    store: Shared,
    identity: Arc<Method>,
}
struct LookupKeys {
    actor_type: Value,
    classes: [Value; 10],
}
struct LookupBinding {
    // Keep the same native method and Store lifetimes as other bindings.
    _store: Shared,
    _identity: Arc<Method>,
    keys: LookupKeys,
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
fn class_name(kind: &str) -> &'static str {
    match kind {
        "Quad" | "Sprite" => "Sprite",
        "Banner" => "Banner",
        "ActorFrameTexture" => "ActorFrameTexture",
        "ActorMultiVertex" => "ActorMultiVertex",
        "ActorProxy" => "ActorProxy",
        "BitmapText" => "BitmapText",
        "Player" => "Player",
        "NoteField" => "NoteField",
        kind if is_frame(kind) => "ActorFrame",
        _ => "Actor",
    }
}

// Prepare the method lookup keys once per VM. A private, traced symbol table
// owns the strings; the native binding only holds copyable IDs. Keeping two
// native captures also preserves debug.getinfo(..., 'u').nups.
const CLASS_KEYS: [&str; 10] = [
    "Actor",
    "ActorFrame",
    "Sprite",
    "Banner",
    "ActorFrameTexture",
    "ActorMultiVertex",
    "ActorProxy",
    "BitmapText",
    "Player",
    "NoteField",
];

fn class_key(keys: &LookupKeys, class: &str) -> Value {
    let offset = match class {
        "Actor" => 0,
        "ActorFrame" => 1,
        "Sprite" => 2,
        "Banner" => 3,
        "ActorFrameTexture" => 4,
        "ActorMultiVertex" => 5,
        "ActorProxy" => 6,
        "BitmapText" => 7,
        "Player" => 8,
        "NoteField" => 9,
        _ => unreachable!("all actor classes map to a captured method key"),
    };
    keys.classes[offset]
}
fn method_flags(call: &HostCall<'_>, key: Value) -> Result<(bool, bool), Error> {
    let Value::String(id) = key else {
        return Ok((false, false));
    };
    let name = std::str::from_utf8(call.string_bytes(id).unwrap())
        .map_err(|e| call.error(e.to_string().as_bytes()))?;
    Ok((
        !name.starts_with("__songlua_") && !name.ends_with("Command"),
        matches!(
            name,
            "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
        ),
    ))
}

#[cfg(feature = "test-support")]
pub(super) fn method_flags_probe(vm: &mut Vm) -> Result<Value, Error> {
    vm.new_host_function(
        |call| {
            let (public, frame_callback) = method_flags(call, arg(call, 0))?;
            call.push(Value::Boolean(public))?;
            call.push(Value::Boolean(frame_callback))
        },
        &[],
    )
}
fn lookup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    let key = arg(call, 1);
    if !matches!(key, Value::String(_)) {
        return call.push(Value::Nil);
    }
    let type_key = call
        .function_data::<LookupBinding>()
        .unwrap()
        .keys
        .actor_type;
    let (_, frame_callback) = method_flags(call, key)?;
    let kind = call.raw_table_get(actor, type_key)?;
    let (public, core, frame) = if kind == Value::Nil {
        ("Actor", true, false)
    } else {
        let id = call
            .coerce_string(kind)?
            .ok_or_else(|| call.error(b"actor type must be a string"))?;
        let kind = std::str::from_utf8(call.string_bytes(id).unwrap())
            .map_err(|e| call.error(e.to_string().as_bytes()))?;
        (class_name(kind), is_core(kind), is_frame(kind))
    };
    let class_key = class_key(&call.function_data::<LookupBinding>().unwrap().keys, public);
    let globals = call.captures()[0];
    let class = match call.peek_table_get(globals, class_key)? {
        Some(value) => value,
        None => call.table_get(globals, class_key)?,
    };
    if class != Value::Nil {
        table(call, class)?;
        let method = match call.peek_table_get(class, key)? {
            Some(value) => value,
            None => call.table_get(class, key)?,
        };
        if method != Value::Nil || core {
            return call.push(method);
        }
    }
    if !frame && frame_callback {
        return call.push(Value::Nil);
    }
    let mut result = match get(call, actor, b"__songlua_bound_methods")? {
        Value::Nil => Value::Nil,
        Value::Table(bound) => call.raw_table_get(bound, key)?,
        _ => return Err(call.error(b"actor method table expected")),
    };
    // Preserve the existing shared-class fallback for the other methods. The
    // three authenticated setters always resolve their actual bound closures.
    // Their eligibility must never depend on a fallback class or Lua helper.
    let Value::String(name) = key else {
        unreachable!()
    };
    let scalar = matches!(call.string_bytes(name).unwrap(), b"visible" | b"x" | b"y");
    if class == Value::Nil && result == Value::Nil && !scalar {
        // The private symbol table also keeps the original classes reachable.
        let symbols = table(call, call.captures()[1])?;
        let originals = table(call, call.raw_table_get(symbols, Value::Number(1.))?)?;
        let mut original = call.raw_table_get(originals, class_key)?;
        if original == Value::Nil {
            let name = if frame {
                b"ActorFrame".as_slice()
            } else {
                b"Actor".as_slice()
            };
            original = get(call, originals, name)?;
        }
        result = call.table_get(original, key)?;
    }
    call.push(result)
}
fn assign(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    let key = arg(call, 1);
    let value = arg(call, 2);
    if matches!(key, Value::String(_))
        && matches!(value, Value::Function(_) | Value::Native(_))
        && method_flags(call, key)?.0
        && matches!(value, Value::Native(_))
    {
        match get(call, actor, b"__songlua_bound_methods")? {
            Value::Nil => {}
            Value::Table(bound) => return call.raw_table_set(bound, key, value),
            _ => return Err(call.error(b"actor method table expected")),
        }
    }
    call.raw_table_set(actor, key, value)
}

pub(super) fn install_metatable(
    vm: &mut Vm,
    methods: TableId,
    store: &Shared,
) -> Result<(), Error> {
    let stringify = vm.new_host_function(stringify, &[])?;
    super::super::put(vm, methods, "Stringify", stringify)?;
    let globals = vm.get_global(b"_G")?;
    let symbols = vm.try_new_table()?;
    let pin = vm.pin(Value::Table(symbols))?;
    let result = (|| {
        vm.raw_table_set(symbols, Value::Number(1.), Value::Table(methods))?;
        let actor_type = vm.try_intern(b"__songlua_actor_type")?;
        vm.raw_table_set(symbols, Value::Number(2.), actor_type)?;
        let mut classes = [Value::Nil; CLASS_KEYS.len()];
        for (index, class) in CLASS_KEYS.iter().enumerate() {
            let value = vm.try_intern(class.as_bytes())?;
            vm.raw_table_set(symbols, Value::Number((index + 3) as f64), value)?;
            classes[index] = value;
        }

        for method in [Method::Lookup, Method::Assign] {
            let identity = Arc::new(method);
            let function = match method {
                Method::Lookup => vm.new_host_function_with_data(
                    lookup,
                    &[globals, Value::Table(symbols)],
                    LookupBinding {
                        _store: store.clone(),
                        _identity: identity.clone(),
                        keys: LookupKeys {
                            actor_type,
                            classes,
                        },
                    },
                )?,
                Method::Assign => vm.new_host_function_with_data(
                    assign,
                    &[globals, Value::Table(methods)],
                    Binding {
                        store: store.clone(),
                        identity: identity.clone(),
                    },
                )?,
                _ => unreachable!("only metatable callbacks are registered here"),
            };
            store
                .lock()
                .unwrap()
                .scalar_methods
                .insert(function, &identity);
            super::super::put(
                vm,
                methods,
                match method {
                    Method::Lookup => "Lookup",
                    Method::Assign => "Assign",
                    _ => unreachable!(),
                },
                function,
            )?;
        }
        Ok(())
    })();
    vm.unpin(pin)?;
    result
}

fn stringify(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    let key = call.try_intern(b"__songlua_actor_type")?;
    let value = call.raw_table_get(actor, key)?;
    let kind = if value == Value::Nil {
        "Actor".to_owned()
    } else {
        let id = call
            .coerce_string(value)?
            .ok_or_else(|| call.error(b"actor type must be a string"))?;
        std::str::from_utf8(call.string_bytes(id).unwrap())
            .map_err(|error| call.error(error.to_string().as_bytes()))?
            .to_owned()
    };
    let kind = match kind.as_str() {
        "Quad" => "Sprite",
        "WrapperState" => "ActorFrame",
        "TopScreen" => "ScreenGameplay",
        kind => kind,
    };
    // Use the VM's opaque identity; actor strings do not expose native pointers.
    let identity = call.object_pointer(Value::Table(actor)).unwrap();
    #[cfg(target_os = "windows")]
    let text = format!("{kind} ({identity:016X})");
    #[cfg(not(target_os = "windows"))]
    let text = format!("{kind} (0x{identity:x})");
    let value = call.try_intern(text.as_bytes())?;
    call.push(value)
}
pub(super) fn install(
    vm: &mut Vm,
    class: TableId,
    setter: Setter,
    store: &Shared,
    captures: &[Value],
) -> Result<(), Error> {
    let identity = Arc::new(Method::Forward(setter));
    let function = vm.new_host_function_with_data(
        forward,
        captures,
        Binding {
            store: store.clone(),
            identity: identity.clone(),
        },
    )?;
    store
        .lock()
        .unwrap()
        .scalar_methods
        .insert(function, &identity);
    super::super::put(vm, class, setter.name(), function)
}

pub(super) fn bind(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let bound = match get(call, actor, b"__songlua_bound_methods")? {
        Value::Nil => {
            let bound = call.try_new_table()?;
            let key = call.try_intern(b"__songlua_bound_methods")?;
            call.raw_table_set(actor, key, Value::Table(bound))?;
            bound
        }
        Value::Table(bound) => bound,
        _ => return Err(call.error(b"actor method table expected")),
    };
    let captures = [call.captures()[0], call.captures()[1], Value::Table(actor)];
    for setter in Setter::ALL {
        // Native installation replaces same-named declaration attributes.
        // Authored Init/On callbacks can replace these methods afterwards.
        let key = call.try_intern(setter.name().as_bytes())?;
        call.raw_table_set(actor, key, Value::Nil)?;
        let previous = get(call, bound, setter.name().as_bytes())?;
        if store.lock().unwrap().scalar_methods.identity(previous)
            == Some(Method::Bound(setter, actor))
        {
            // An interrupted construction can leave some original bindings
            // installed before the actor joins Store. Reuse only bindings
            // authenticated for this exact actor and operation on retry.
            continue;
        }
        let identity = Arc::new(Method::Bound(setter, actor));
        let function = call.new_host_function_with_data(
            bound_setter,
            &captures,
            Binding {
                store: store.clone(),
                identity: identity.clone(),
            },
        )?;
        store
            .lock()
            .unwrap()
            .scalar_methods
            .insert(function, &identity);
        call.with_roots(&[function], |call| {
            let key = call.try_intern(setter.name().as_bytes())?;
            call.raw_table_set(bound, key, function)
        })?;
    }
    Ok(())
}

fn forward(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (setter, store) = {
        let data = call.function_data::<Binding>().unwrap();
        let Method::Forward(setter) = *data.identity else {
            unreachable!()
        };
        (setter, data.store.clone())
    };
    let Value::Table(actor) = arg(call, 0) else {
        return Err(
            call.error(format!("actor method {} requires an actor", setter.name()).as_bytes())
        );
    };
    let source = match get(call, actor, b"__songlua_bound_methods")? {
        Value::Nil => actor,
        Value::Table(bound) => bound,
        _ => return Err(call.error(b"actor method table expected")),
    };
    let function = get(call, source, setter.name().as_bytes())?;
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(
            format!(
                "native actor method {} has no semantic implementation",
                setter.name()
            )
            .as_bytes(),
        ));
    }
    let args = SmallVec::<[Value; 4]>::from_slice(call.arguments());
    live_trace::scalar_forward(call, &store, setter.name(), function, &args)
}
fn bound_setter(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (setter, identity, store) = {
        let data = call.function_data::<Binding>().unwrap();
        let Method::Bound(setter, identity) = *data.identity else {
            unreachable!()
        };
        (setter, identity, data.store.clone())
    };
    let actor = table(call, call.captures()[2])?;
    debug_assert_eq!(actor, identity);
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return Err(call.error(b"actor instance is not owned by this runtime or has been released"));
    }
    ease_probe::method(call, &store, actor, setter.name())?;
    set_scalar(call, &store, actor, setter.name())?;
    if setter == Setter::Visible
        && let Some(value) = boolish(call, arg(call, 1))
    {
        publish_plain_visibility(call, &store, actor, value)?;
    }
    call.push(Value::Table(actor))
}

// Ordinary actor calls also expose the native preparation block to Lua. Song
// compiler sessions and playback have their own existing capture/publication
// paths; this mirrors the plain constructor path, without adding a second
// capture owner to those sessions.
fn publish_plain_visibility(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    value: bool,
) -> Result<(), Error> {
    let plain = {
        let guard = store.lock().unwrap();
        !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
            && !guard.playback
            && !guard.preparing
            && guard.capture_scope.is_none()
            && guard.startup_queues.is_none()
            && guard.replay_frame.is_none()
            && guard.actors[&actor].capture.is_none()
            && guard.actors[&actor].replay.is_none()
    };
    if !plain {
        return Ok(());
    }
    let block = match sprites::field(call, actor, "__songlua_capture_block")? {
        Value::Nil => {
            let block = call.try_new_table()?;
            call.with_roots(&[Value::Table(block)], |call| {
                for (name, source) in [
                    ("start", "__songlua_capture_cursor"),
                    ("duration", "__songlua_capture_duration"),
                ] {
                    let value = sprites::field(call, actor, source)?;
                    let value = if value == Value::Nil {
                        0.
                    } else {
                        sprites::number(call, value)? as f32
                    };
                    let value = if name == "duration" {
                        value.max(0.)
                    } else {
                        value
                    };
                    sprites::set(call, block, name, Value::Number(f64::from(value)))?;
                }
                let steps = sprites::field(call, actor, "__songlua_command_queue_steps")?;
                if let Value::Table(steps) = steps {
                    let count = call.raw_table_len(steps)?;
                    sprites::set(
                        call,
                        block,
                        "__songlua_tween_step",
                        Value::Number(count as f64),
                    )?;
                } else if steps != Value::Nil {
                    return Err(call.error(b"actor command queue steps must be a table"));
                }
                sprites::set(call, block, "__songlua_queued_block", Value::Boolean(false))?;
                for (name, source) in [
                    ("easing", "__songlua_capture_easing"),
                    ("opt1", "__songlua_capture_opt1"),
                    ("opt2", "__songlua_capture_opt2"),
                ] {
                    let value = sprites::field(call, actor, source)?;
                    sprites::set(call, block, name, value)?;
                }
                sprites::set(call, block, "__songlua_has_changes", Value::Boolean(false))?;
                sprites::set(call, actor, "__songlua_capture_block", Value::Table(block))
            })?;
            block
        }
        value => table(call, value)?,
    };
    call.with_roots(&[Value::Table(block)], |call| {
        sprites::set(call, block, "visible", Value::Boolean(value))?;
        sprites::set(call, block, "__songlua_has_changes", Value::Boolean(true))?;
        sprites::set(
            call,
            actor,
            "__songlua_state_visible",
            Value::Boolean(value),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_method_identities_are_private_and_do_not_root_replaced_functions() {
        let context =
            crate::SongLuaCompileContext::new(std::path::Path::new("."), "Setter identities");
        let mut runtime = crate::SongRuntime::new(&context).unwrap();
        let script = |runtime: &mut crate::SongRuntime, source: &str| {
            runtime
                .vm_mut()
                .execute(&compile_named(source.as_bytes(), b"=setter identity").unwrap())
                .unwrap()
                .to_vec()
        };
        let value = script(&mut runtime, "return Def.ActorFrame{Def.Quad{}}")[0];
        let scene = runtime.instantiate_actor(value).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        let values = script(
            &mut runtime,
            "return root[1].__songlua_bound_methods.x, Actor.x, getmetatable(root[1]).__index, getmetatable(root[1]).__newindex, function()end",
        );
        let actor = runtime
            .vm_mut()
            .raw_table_get(table_id(root), Value::Number(1.))
            .unwrap();
        {
            let guard = runtime.actors.store.lock().unwrap();
            let methods = &guard.scalar_methods;
            assert_eq!(
                methods.identity(values[0]),
                Some(Method::Bound(Setter::X, table_id(actor)))
            );
            assert_eq!(
                methods.identity(values[1]),
                Some(Method::Forward(Setter::X))
            );
            assert_eq!(methods.identity(values[2]), Some(Method::Lookup));
            assert_eq!(methods.identity(values[3]), Some(Method::Assign));
            assert_eq!(methods.identity(values[4]), None);
        }
        script(
            &mut runtime,
            "root[1].__songlua_bound_methods.x=function()end; collectgarbage('collect');collectgarbage('collect')",
        );
        assert_eq!(
            runtime
                .actors
                .store
                .lock()
                .unwrap()
                .scalar_methods
                .identity(values[0]),
            None
        );
        assert_eq!(
            runtime
                .actors
                .store
                .lock()
                .unwrap()
                .scalar_methods
                .identity(values[1]),
            Some(Method::Forward(Setter::X))
        );
    }
    fn table_id(value: Value) -> TableId {
        let Value::Table(id) = value else {
            panic!("actor table")
        };
        id
    }
}
