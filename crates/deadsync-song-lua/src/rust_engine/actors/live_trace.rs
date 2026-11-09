//! Existing selected live observations over the shared Rust host API.
use super::*;
use deadlib_lua::{NativeDebugFrame, NativeStack};
use serde_json::{Value as Json, json};

const ROOTS: &[u8] = b"live_trace_roots";

#[derive(Default)]
pub(super) struct State {
    pub(super) scene: Option<u64>,
    pub(super) generation: u64,
    depth: usize,
    // Every identity here is retained in the helpers table, like the native
    // observer's owned Table handles. Rust maps alone never trace Lua values.
    objects: HashMap<TableId, Json>,
    hooks: HashSet<(TableId, String)>,
}

fn enabled(store: &Shared) -> bool {
    log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace)
        && store.lock().unwrap().trace.scene.is_some()
}
fn emit(event: Json) {
    #[cfg(feature = "test-support")]
    crate::trace_capture::emit(event);
    #[cfg(not(feature = "test-support"))]
    log::trace!(target: "deadsync_song_lua::trace", "{event}");
}

impl Actors {
    pub(in crate::rust_engine) fn install_live_trace(&self, vm: &mut Vm) -> Result<(), Error> {
        if !self
            .store
            .lock()
            .unwrap()
            .clock
            .live
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(());
        }
        if !log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) {
            let mut store = self.store.lock().unwrap();
            store.trace.scene = Some(store.trace.generation);
            return Ok(());
        }
        let captures = [Value::Table(self.registry), Value::Table(self.helpers)];
        let remember = vm.new_host_function_with_data(remember, &captures, self.store.clone())?;
        let pin = vm.pin(remember)?;
        let result = (|| {
            let hook = vm.new_host_function_with_data(hook, &captures, self.store.clone())?;
            let pin = vm.pin(hook)?;
            let result = (|| {
                let code =
                    compile_named(include_bytes!("live_trace.lua"), b"=DeadSync live observer")?;
                let install = vm.execute(&code)?[0];
                {
                    let mut store = self.store.lock().unwrap();
                    store.trace.objects.clear();
                    store.trace.depth += 1;
                }
                let result = vm.call(install, &[remember, hook]).map(|_| ());
                self.store.lock().unwrap().trace.depth -= 1;
                result
            })();
            vm.unpin(pin)?;
            result
        })();
        vm.unpin(pin)?;
        result?;
        let mut store = self.store.lock().unwrap();
        store.trace.scene = Some(store.trace.generation);
        Ok(())
    }
}

fn retain(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    let helpers = table(call, call.captures()[1])?;
    let key = call.try_intern(ROOTS)?;
    let roots = call.with_roots(&[key], |call| match call.raw_table_get(helpers, key)? {
        Value::Nil => {
            let roots = call.try_new_table()?;
            call.raw_table_set(helpers, key, Value::Table(roots))?;
            Ok(roots)
        }
        value => table(call, value),
    })?;
    call.raw_table_set(roots, Value::Table(owner), Value::Boolean(true))
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let value = bytes(call, value)?;
    String::from_utf8(value).map_err(|e| call.error(e.to_string().as_bytes()))
}
fn remember(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let owner = table(call, arg(call, 0))?;
    retain(call, owner)?;
    let kind = text(call, arg(call, 1))?;
    let label = match kind.as_str() {
        "Song" => json!({"type":kind,"song_dir":text(call,arg(call,2))?}),
        "Style" => json!({"type":kind,"name":text(call,arg(call,2))?}),
        "StageStats" | "PlayerStageStats" | "PlayerState" | "PlayerOptions" => {
            json!({"type":kind,"path":text(call,arg(call,2))?})
        }
        "Steps" => json!({"type":kind,"song_dir":text(call,arg(call,2))?,
            "index":call.number(3)? as usize,"steps_type":text(call,arg(call,4))?,
            "difficulty":text(call,arg(call,5))?,"description":text(call,arg(call,6))?,
            "meter":call.number(7)? as i64}),
        _ => json!({"type":kind}),
    };
    store.lock().unwrap().trace.objects.insert(owner, label);
    Ok(())
}

#[derive(Clone)]
struct Query {
    store: Shared,
    class: String,
    name: String,
}
fn hook(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let mut owner = table(call, arg(call, 0))?;
    let class = text(call, arg(call, 1))?;
    let name = text(call, arg(call, 2))?;
    if class == "GameState" {
        let methods = get(call, owner, b"__songlua_class_methods")?;
        if methods != Value::Nil {
            owner = table(call, methods)?;
        }
    }
    retain(call, owner)?;
    if store
        .lock()
        .unwrap()
        .trace
        .hooks
        .contains(&(owner, name.clone()))
    {
        return Ok(());
    }
    let key = call.try_intern(name.as_bytes())?;
    let original = call.table_get(Value::Table(owner), key)?;
    if !matches!(original, Value::Function(_) | Value::Native(_)) {
        return Err(
            call.error(format!("missing native observer binding {class}.{name}").as_bytes())
        );
    }
    call.with_roots(&[original, key], |call| {
        let function = wrap(call, &store, class, name.clone(), original)?;
        call.raw_table_set(owner, key, function)
    })?;
    store.lock().unwrap().trace.hooks.insert((owner, name));
    Ok(())
}

fn wrap(
    call: &mut HostCall<'_>,
    store: &Shared,
    class: String,
    name: String,
    original: Value,
) -> Result<Value, Error> {
    let captures = [call.captures()[0], call.captures()[1]];
    let left = call.new_host_function_with_data(
        |call| {
            let store = call.function_data::<Shared>().unwrap().clone();
            let actor = table(call, arg(call, 0))?;
            let left = lifecycle::time_left(call, &store, actor)?;
            call.push(Value::Number(f64::from(left)))
        },
        &captures,
        store.clone(),
    )?;
    call.with_roots(&[left], |call| {
        call.new_host_function_with_data(
            query,
            &[captures[0], captures[1], original, left],
            Query {
                store: store.clone(),
                class,
                name,
            },
        )
    })
}

// The authenticated forward remains the publicly selected closure. Only its
// existing call to the captured bound setter goes through this observer.
pub(super) fn scalar_forward(
    call: &mut HostCall<'_>,
    store: &Shared,
    name: &str,
    original: Value,
    args: &[Value],
) -> Result<Vec<Value>, Error> {
    if !enabled(store) {
        return call.call(original, args).map(|values| values.to_vec());
    }
    let wrapped = call.with_roots(&[original], |call| {
        wrap(call, store, "Actor".into(), name.into(), original)
    })?;
    call.call(wrapped, args).map(|values| values.to_vec())
}

#[derive(Clone)]
struct Command {
    store: Shared,
    name: String,
    entering: bool,
}
pub(super) fn command(
    call: &mut HostCall<'_>,
    store: &Shared,
    function: Value,
    name: &[u8],
    entering: bool,
) {
    if !enabled(store) {
        return;
    }
    let _ = (|| -> Result<(), Error> {
        let f = call.new_host_function_with_data(
            command_event,
            &[call.captures()[0], call.captures()[1], function],
            Command {
                store: store.clone(),
                name: String::from_utf8_lossy(name).into_owned(),
                entering,
            },
        )?;
        call.call(f, &[]).map(|_| ())
    })();
}
fn command_event(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Command>().unwrap().clone();
    let globals = helper(call, 5)?;
    let function = call.captures()[2];
    let mut s = call.native_stack()?;
    s.push(function)?;
    let info = s.get_info(NativeDebugFrame::from_index(s.thread_id(), 0), b">S")?;
    let Some(source) = info.source.and_then(|source| s.debug_bytes(source)) else {
        return Ok(());
    };
    let source = source.strip_suffix(&[0]).unwrap_or(source);
    if source.starts_with(b"@theme:") {
        return Ok(());
    }
    let source = String::from_utf8_lossy(source).into_owned();
    let [beat, seconds] = clock(&mut s, globals)?;
    let Some(scene) = data.store.lock().unwrap().trace.scene else {
        return Ok(());
    };
    let line = if info.what == Some(b"C\0".as_slice()) {
        None
    } else {
        info.defined.map(|v| v[0])
    };
    emit(
        json!({"scene":scene,"kind":"command","operation":if data.entering{"command.begin"}else{"command.end"},
        "command":data.name,"source":source,"line":line,"beat":beat,"seconds":seconds}),
    );
    Ok(())
}

fn query(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Query>().unwrap().clone();
    let original = call.captures()[2];
    let left = call.captures()[3];
    if !enabled(&data.store) {
        let args = call.arguments().to_vec();
        let result = call.call(original, &args)?.to_vec();
        for value in result {
            call.push(value)?;
        }
        return Ok(());
    }
    let globals = helper(call, 5)?;
    let mut s = call.native_stack()?;
    let count = s.top();
    s.push(original)?;
    for index in 1..=count {
        s.push(s.value(index as isize).unwrap())?;
    }
    let special = data.class == "Actor" && matches!(data.name.as_str(), "sleep" | "queuecommand");
    let observed = data.store.lock().unwrap().trace.depth == 0;
    if !special {
        data.store.lock().unwrap().trace.depth += 1;
    }
    let result = s.call(count, None);
    let result_top = s.top();
    if !special {
        data.store.lock().unwrap().trace.depth -= 1;
    }
    if data.class == "PlayerState" && data.name == "GetPlayerOptions" {
        let _ = option_path(&mut s, &data.store, count, result.as_ref().ok().copied());
    }
    if observed && (!special || result.is_ok()) {
        let modifier = matches!(data.class.as_str(), "PlayerOptions" | "SongOptions");
        let kind = if data.name.starts_with("Get")
            || data.name.starts_with("Is")
            || data.name.starts_with("Supports")
            || modifier && count == 1
        {
            "query"
        } else if modifier || data.class == "PlayerState" && data.name == "SetPlayerOptions" {
            "modifier"
        } else {
            "call"
        };
        let operation = format!("{}.{}", data.class, data.name);
        let details = |s: &mut NativeStack<'_, '_>| -> Result<Json, Error> {
            if special {
                let argument = s.value(2).unwrap_or(Value::Nil);
                let argument = match argument {
                    Value::String(_) | Value::Number(_) => encode(s, &data.store, argument, 0)?,
                    _ => Json::Null,
                };
                let actor = s.value(1);
                Ok(json!({"args":[argument],"tween_time_left":tween_left(s,left,actor)?}))
            } else if let Ok(results) = &result {
                let mut args = Vec::new();
                for index in 2..=count {
                    let value = s.value(index as isize).unwrap();
                    args.push(encode(s, &data.store, value, 0)?);
                }
                let mut values = Vec::new();
                for index in count + 1..=count + results {
                    let value = s.value(index as isize).unwrap();
                    values.push(encode(s, &data.store, value, 0)?);
                }
                let mut detail = json!({"args":args,"results":values});
                if kind == "call"
                    && matches!(
                        data.class.as_str(),
                        "Actor" | "ActorFrame" | "Sprite" | "ActorProxy" | "ActorFrameTexture"
                    )
                {
                    let actor = s.value(1);
                    detail["tween_time_left"] = tween_left(s, left, actor)?;
                }
                Ok(detail)
            } else {
                Ok(json!({"kind":"trace-error","error":result.as_ref().unwrap_err().to_string()}))
            }
        };
        let _ = method(&mut s, &data.store, globals, kind, &operation, details);
    }
    let restored = s.set_top(result_top);
    match result {
        Ok(count) => {
            restored?;
            s.finish(count)
        }
        Err(error) => Err(error),
    }
}

fn option_path(
    s: &mut NativeStack<'_, '_>,
    store: &Shared,
    count: usize,
    results: Option<usize>,
) -> Result<(), Error> {
    if results.unwrap_or(0) == 0 || count < 2 {
        return Ok(());
    }
    let (Some(Value::Table(state)), Some(Value::Table(options))) =
        (s.value(1), s.value((count + 1) as isize))
    else {
        return Ok(());
    };
    let level = s.value(2).unwrap();
    s.push(level)?;
    let level = s
        .to_bytes(-1)?
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_else(|| "nil".into());
    s.set_top(s.top() - 1)?;
    let mut store = store.lock().unwrap();
    let path = store
        .trace
        .objects
        .get(&state)
        .and_then(|v| v["path"].as_str())
        .unwrap_or("player-state")
        .to_owned();
    if let Some(label) = store.trace.objects.get_mut(&options) {
        label["path"] = json!(format!("{path}/options:{level}"));
    }
    Ok(())
}

fn tween_left(
    s: &mut NativeStack<'_, '_>,
    function: Value,
    actor: Option<Value>,
) -> Result<Json, Error> {
    let Some(actor @ Value::Table(_)) = actor else {
        return Ok(Json::Null);
    };
    let before = s.top();
    s.push(function)?;
    s.push(actor)?;
    s.call(1, Some(1))?;
    let left = s.number(-1)?;
    s.set_top(before)?;
    Ok(number(left))
}

fn method(
    s: &mut NativeStack<'_, '_>,
    store: &Shared,
    globals: Value,
    kind: &str,
    operation: &str,
    details: impl FnOnce(&mut NativeStack<'_, '_>) -> Result<Json, Error>,
) -> Result<(), Error> {
    let (scene, depth, callback) = {
        let g = store.lock().unwrap();
        (g.trace.scene, g.trace.depth, g.song_callback_depth != 0)
    };
    let Some(scene) = scene else {
        return Ok(());
    };
    if depth != 0 {
        return Ok(());
    }
    let mut caller = None;
    for level in 1..32 {
        let Some(frame) = s.get_stack(s.thread_id(), level)? else {
            break;
        };
        let info = s.get_info(frame, b"S")?;
        if info.what != Some(b"C\0".as_slice()) {
            caller = info
                .source
                .and_then(|source| s.debug_bytes(source))
                .map(|source| {
                    let source = source.strip_suffix(&[0]).unwrap_or(source);
                    (
                        String::from_utf8_lossy(source).into_owned(),
                        info.defined.map(|v| v[0]),
                    )
                });
            break;
        }
    }
    let Some((source, line)) = caller else {
        return Ok(());
    };
    let [beat, seconds] = clock(s, globals)?;
    if source.starts_with("@theme:") && !callback {
        return Ok(());
    }
    // Match the native observer: ineligible callers never evaluate details,
    // and diagnostic failures cannot replace the authored return values.
    let details =
        details(s).unwrap_or_else(|error| json!({"kind":"trace-error","error":error.to_string()}));
    let mut event = json!({"scene":scene,"kind":kind,"operation":operation,"source":source,
        "line":line,"phase":if callback{"callback"}else{"construction"},"beat":beat,"seconds":seconds});
    if let Json::Object(details) = details {
        event.as_object_mut().unwrap().extend(details);
    }
    emit(event);
    Ok(())
}

fn field(s: &mut NativeStack<'_, '_>, owner: isize, key: &[u8]) -> Result<Value, Error> {
    s.push_bytes(key)?;
    s.get_table(owner, true)?;
    let value = s.value(-1).unwrap();
    s.set_top(s.top() - 1)?;
    Ok(value)
}
fn clock(s: &mut NativeStack<'_, '_>, globals: Value) -> Result<[f64; 2], Error> {
    let before = s.top();
    s.push(globals)?;
    let globals = s.top_offset();
    let runtime = field(s, globals, crate::SONG_LUA_RUNTIME_KEY.as_bytes())?;
    s.push(runtime)?;
    let runtime = s.top_offset();
    let mut output = [0.; 2];
    for (i, key) in [
        crate::SONG_LUA_RUNTIME_BEAT_KEY,
        crate::SONG_LUA_RUNTIME_SECONDS_KEY,
    ]
    .iter()
    .enumerate()
    {
        let value = field(s, runtime, key.as_bytes())?;
        s.push(value)?;
        output[i] = s.number(-1)?;
        s.set_top(s.top() - 1)?;
    }
    s.set_top(before)?;
    Ok(output)
}
fn number(value: f64) -> Json {
    if value == (value as u32) as f64 {
        json!(value as u32)
    } else if value == (value as i32) as f64 {
        json!(value as i32)
    } else if value.is_finite() {
        json!(value)
    } else {
        json!({"type":"number","value":if value.is_nan(){"nan"}else if value.is_sign_negative(){"-infinity"}else{"infinity"}})
    }
}
fn string(s: &mut NativeStack<'_, '_>, value: Value) -> Result<String, Error> {
    s.push(value)?;
    let bytes = match s.to_bytes(-1)? {
        Some(bytes) => bytes.to_vec(),
        None => return Err(s.error(b"trace string expected")),
    };
    let result = String::from_utf8(bytes).map_err(|e| s.error(e.to_string().as_bytes()))?;
    s.set_top(s.top() - 1)?;
    Ok(result)
}
fn actor_name(s: &mut NativeStack<'_, '_>, store: &Shared, index: isize) -> Result<String, Error> {
    let native = match s.value(index) {
        Some(Value::Table(id)) => store.lock().unwrap().actors.get(&id).and_then(|actor| {
            (!actor.raw_instance && !actor.pending_init && !actor.offline_instance)
                .then(|| actor.state.name.clone())
        }),
        _ => None,
    };
    if let Some(name) = native {
        return String::from_utf8(name).map_err(|error| s.error(error.to_string().as_bytes()));
    }
    for key in [
        b"__songlua_load_name".as_slice(),
        b"__songlua_native_name",
        b"Name",
    ] {
        let value = field(s, index, key)?;
        if value != Value::Nil {
            return string(s, value);
        }
    }
    Ok(String::new())
}
fn encode(
    s: &mut NativeStack<'_, '_>,
    store: &Shared,
    value: Value,
    depth: usize,
) -> Result<Json, Error> {
    Ok(match value {
        Value::Nil => json!({"type":"nil"}),
        Value::Boolean(value) => json!(value),
        Value::Number(value) => number(value),
        Value::String(_) => json!(string(s, value)?),
        Value::Function(_) | Value::Native(_) => {
            s.push(value)?;
            let info = s.get_info(NativeDebugFrame::from_index(s.thread_id(), 0), b">S")?;
            let source = info
                .source
                .and_then(|source| s.debug_bytes(source))
                .unwrap_or_default();
            let source = source.strip_suffix(&[0]).unwrap_or(source);
            let [line, last] = info.defined.unwrap_or([-1, -1]);
            json!({"type":"function","source":String::from_utf8_lossy(source),"line":line,"last_line":last})
        }
        Value::Table(id) => {
            let before = s.top();
            s.push(value)?;
            let index = s.top_offset();
            let kind = field(s, index, b"__songlua_actor_type")?;
            let label = { store.lock().unwrap().trace.objects.get(&id).cloned() };
            let result = if kind != Value::Nil {
                let name = actor_name(s, store, index)?;
                let identity = s.to_pointer(index).unwrap();
                let mut path = Vec::new();
                let mut seen = HashSet::new();
                let mut current = Some(value);
                for _ in 0..32 {
                    let Some(actor) = current.take() else {
                        break;
                    };
                    let Value::Table(actor_id) = actor else {
                        return Err(s.error(b"actor parent must be a table"));
                    };
                    if !seen.insert(actor_id) {
                        return Err(s.error(b"cycle in actor query ancestry"));
                    }
                    s.push(actor)?;
                    let actor = s.top_offset();
                    let name = actor_name(s, store, actor)?;
                    if !name.is_empty() {
                        path.push(name);
                    }
                    let native_parent = store
                        .lock()
                        .unwrap()
                        .actors
                        .get(&actor_id)
                        .map(|actor| actor.parent.map_or(Value::Nil, Value::Table));
                    let parent = match native_parent {
                        Some(parent) => parent,
                        None => field(s, actor, b"__songlua_parent")?,
                    };
                    if parent != Value::Nil {
                        current = Some(parent);
                    }
                    s.set_top(s.top() - 1)?;
                }
                if current.is_some() {
                    return Err(s.error(b"actor query ancestry exceeds capture depth"));
                }
                path.reverse();
                json!({"actor":format!("0x{identity:x}"),"name":name,"path":path.join("/")})
            } else {
                let kind = field(s, index, b"__songlua_native_type")?;
                if kind != Value::Nil {
                    json!({"type":string(s,kind)?})
                } else if let Some(label) = label {
                    label
                } else if depth >= 5 {
                    json!({"type":"table","truncated":"depth"})
                } else {
                    let mut values = Vec::new();
                    for i in 1..=s.object_len(index)? {
                        s.raw_get_integer(index, i as i32)?;
                        let value = s.value(-1).unwrap();
                        values.push(encode(s, store, value, depth + 1)?);
                        s.set_top(s.top() - 1)?;
                    }
                    Json::Array(values)
                }
            };
            s.set_top(before)?;
            result
        }
        value => json!({"type":value.type_name()}),
    })
}

#[cfg(feature = "test-support")]
impl Actors {
    pub(in crate::rust_engine) fn fixture_query_value(
        &self,
        vm: &mut Vm,
        value: Value,
        depth: usize,
    ) -> Result<Json, Error> {
        vm.with_native_stack(|stack| encode(stack, &self.store, value, depth))
    }
    pub(in crate::rust_engine) fn fixture_query_objects(&self, objects: &[(Value, Json)]) {
        let mut store = self.store.lock().unwrap();
        store.trace.objects.clear();
        store.trace.hooks.clear();
        store.trace.depth = 0;
        for (value, label) in objects {
            let Value::Table(id) = *value else {
                panic!("query fixture table")
            };
            store.trace.objects.insert(id, label.clone());
        }
    }
    pub(in crate::rust_engine) fn fixture_query_hook(
        &self,
        vm: &mut Vm,
        owner: Value,
        class: &str,
        name: &str,
    ) -> Result<(), Error> {
        let class = vm.try_intern(class.as_bytes())?;
        let class_pin = vm.pin(class)?;
        let result = (|| {
            let name = vm.try_intern(name.as_bytes())?;
            let callback = vm.new_host_function_with_data(
                hook,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    owner,
                    class,
                    name,
                ],
                self.store.clone(),
            )?;
            vm.call(callback, &[owner, class, name]).map(|_| ())
        })();
        vm.unpin(class_pin)?;
        result
    }
    pub(in crate::rust_engine) fn fixture_query_hooks(&self) -> Vec<(Value, String)> {
        self.store
            .lock()
            .unwrap()
            .trace
            .hooks
            .iter()
            .map(|(owner, name)| (Value::Table(*owner), name.clone()))
            .collect()
    }
}
