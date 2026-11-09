//! Screen-owned input registry. Function values are rooted by the private Lua map.
use super::*;
use crate::{SongLuaInputEvent, SongLuaInputType};
use deadlib_lua::{ErrorKind, NativeHook, NativeStack};
mod predicate;
pub use predicate::CompiledInput;
pub(super) use predicate::{Frame, bindings};

pub(super) struct Input {
    root: Root,
    pub state: Shared,
}
impl Input {
    pub(super) fn snapshot_functions(&self, vm: &mut Vm) -> Option<Vec<Value>> {
        let Value::Table(owner) = vm.rooted(self.root).ok()? else {
            return None;
        };
        let key = vm.try_intern(b"functions").ok()?;
        if !matches!(vm.raw_table_get(owner, key).ok()?, Value::Table(_)) {
            return None;
        }
        let state = self.state.lock().unwrap();
        (!state.dispatching).then(|| state.entries.iter().map(|entry| entry.function).collect())
    }
}
pub(super) type Shared = Arc<Mutex<State>>;
#[derive(Default)]
pub(super) struct State {
    entries: Vec<Entry>,
    dispatching: bool,
    removals: Vec<usize>,
    required: bool,
}
struct Entry {
    key: usize,
    function: Value,
    compiled: Option<CompiledInput>,
}
#[derive(Default, Clone, Copy)]
struct Cursor {
    previous: Option<usize>,
    next: usize,
}
enum Prefix {
    Done(bool),
    Lua(Cursor),
}
#[derive(Clone, Copy)]
enum Op {
    Add,
    Remove,
}
struct Binding {
    state: Shared,
    op: Op,
}
#[cfg(feature = "test-support")]
#[derive(Clone)]
struct OwnedEvent {
    player: Option<usize>,
    controller: Option<usize>,
    button: String,
    game_button: String,
    kind: SongLuaInputType,
    device: String,
    device_button: String,
    level: f32,
    z: i32,
    down: bool,
    ago: f64,
    is_joystick: bool,
    is_mouse: bool,
}
#[cfg(feature = "test-support")]
impl OwnedEvent {
    pub fn new(input: &SongLuaInputEvent<'_>) -> Self {
        Self {
            player: input.player,
            controller: input.controller,
            button: input.button.into(),
            game_button: input.game_button.into(),
            kind: input.kind,
            device: input.device.into(),
            device_button: input.device_button.into(),
            level: input.level,
            z: input.z,
            down: input.down,
            ago: input.ago,
            is_joystick: input.is_joystick,
            is_mouse: input.is_mouse,
        }
    }
    fn event(&self) -> SongLuaInputEvent<'_> {
        SongLuaInputEvent {
            player: self.player,
            controller: self.controller,
            button: &self.button,
            game_button: &self.game_button,
            kind: self.kind,
            device: &self.device,
            device_button: &self.device_button,
            level: self.level,
            z: self.z,
            down: self.down,
            ago: self.ago,
            is_joystick: self.is_joystick,
            is_mouse: self.is_mouse,
        }
    }
}
pub(super) fn event_kind(kind: SongLuaInputType) -> &'static str {
    match kind {
        SongLuaInputType::FirstPress => "InputEventType_FirstPress",
        SongLuaInputType::Repeat => "InputEventType_Repeat",
        SongLuaInputType::Release => "InputEventType_Release",
    }
}
impl RustSongRuntime {
    /// Bind a newly constructed Screen and replace its process-owned callback map.
    /// Old saved methods address the new map; offline incompatibility stays sticky.
    pub fn bind_screen_input(&mut self, screen: Value) -> Result<(), Error> {
        let owner = self.vm.rooted(self.input.root)?;
        let state = self.input.state.clone();
        self.vm
            .with_native_stack(|stack| bind(stack, &state, owner, screen))
    }
    /// Input handling only controls subsequent Lua callbacks, not gameplay input.
    pub fn dispatch_input(
        &mut self,
        event: &SongLuaInputEvent<'_>,
    ) -> Result<(bool, Vec<String>), Error> {
        let owner = self.vm.rooted(self.input.root)?;
        let state = self.input.state.clone();
        let result = self
            .vm
            .with_native_stack(|stack| dispatch_stack(stack, &state, owner, event));
        // Resource aborts unwind to this idle boundary before deferred roots can
        // be released. Ordinary callback errors are collected inside dispatch.
        let cleanup = self
            .vm
            .with_native_stack(|stack| finish_stack(stack, &state, owner));
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                cleanup?;
                Ok(value)
            }
        }
    }
    pub fn require_input_offline_compatible(&self) -> Result<(), String> {
        if self.input.state.lock().unwrap().required {
            Err(crate::input_types::RUNTIME_REQUIRED.into())
        } else {
            Ok(())
        }
    }
}
pub(super) fn install(vm: &mut Vm) -> Result<Input, Error> {
    let owner = vm.try_new_table()?;
    let root = vm.pin(Value::Table(owner))?;
    let state = Arc::new(Mutex::new(State::default()));
    Ok(Input { root, state })
}
fn table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    if let Value::Table(t) = value {
        Ok(t)
    } else {
        Err(call.error(b"input table expected"))
    }
}
fn function(call: &HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"input callback function expected"))
    }
}
fn map(call: &mut HostCall<'_>, owner: Value) -> Result<TableId, Error> {
    let key = call.try_intern(b"functions")?;
    let value = call.raw_table_get(table(call, owner)?, key)?;
    table(call, value)
}
fn bind(
    stack: &mut NativeStack<'_, '_>,
    state: &Shared,
    owner: Value,
    screen: Value,
) -> Result<(), Error> {
    if !matches!(screen, Value::Table(_)) {
        return Err(stack.error(b"input table expected"));
    }
    let base = stack.top();
    stack.push(screen)?;
    let screen_index = stack.top();
    stack.push(owner)?;
    let owner_index = stack.top();
    stack.create_table(0, 0)?;
    let functions = stack.value(-1).unwrap();
    stack.push_bytes(b"functions")?;
    stack.push(functions)?;
    stack.set_table(owner_index as isize, true)?;
    {
        let mut s = state.lock().unwrap();
        s.entries.clear();
        s.removals.clear();
        s.dispatching = false;
    }
    for (name, op) in [
        ("AddInputCallback", Op::Add),
        ("RemoveInputCallback", Op::Remove),
    ] {
        stack.push_bytes(name.as_bytes())?;
        stack.push(owner)?;
        stack.push_closure(
            callback,
            1,
            Binding {
                state: state.clone(),
                op,
            },
        )?;
        stack.set_table(screen_index as isize, false)?;
    }
    stack.set_top(base)
}
fn dump(call: &mut HostCall<'_>, value: Value) -> Result<Option<Vec<u8>>, Error> {
    if !matches!(value, Value::Function(_)) {
        return Ok(None);
    }
    let mut stack = call.native_stack()?;
    stack.push(value)?;
    let mut bytes = Vec::new();
    let status = stack.dump(|_, part| {
        bytes.extend_from_slice(part);
        Ok(0)
    })?;
    Ok((status == 0).then_some(bytes))
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap();
    let op = binding.op;
    let state = binding.state.clone();
    let owner = call.captures()[0];
    match op {
        Op::Add | Op::Remove => {
            let screen = table(
                call,
                call.arguments().first().copied().unwrap_or(Value::Nil),
            )?;
            let f = function(call, call.arguments().get(1).copied().unwrap_or(Value::Nil))?;
            let key = call.object_pointer(f).expect("function identity");
            if matches!(op, Op::Add) {
                state.lock().unwrap().required = true;
                let compiled = dump(call, f)?.and_then(|bytes| CompiledInput::compile(&bytes));
                let map = map(call, owner)?;
                call.raw_table_set(map, f, Value::Boolean(true))?;
                let mut state = state.lock().unwrap();
                let entry = Entry {
                    key,
                    function: f,
                    compiled,
                };
                match state.entries.binary_search_by_key(&key, |e| e.key) {
                    Ok(i) => state.entries[i] = entry,
                    Err(i) => state.entries.insert(i, entry),
                }
            } else {
                let deferred = {
                    let mut state = state.lock().unwrap();
                    if state.dispatching {
                        state.removals.push(key);
                        true
                    } else {
                        false
                    }
                };
                if !deferred {
                    remove(call, &state, owner, key)?;
                }
            }
            call.push(Value::Table(screen))
        }
    }
}
fn remove(call: &mut HostCall<'_>, shared: &Shared, owner: Value, key: usize) -> Result<(), Error> {
    let value = {
        let state = shared.lock().unwrap();
        state
            .entries
            .binary_search_by_key(&key, |e| e.key)
            .ok()
            .map(|i| state.entries[i].function)
    };
    if let Some(value) = value {
        let map = map(call, owner)?;
        call.raw_table_set(map, value, Value::Nil)?;
        let mut state = shared.lock().unwrap();
        if let Ok(i) = state.entries.binary_search_by_key(&key, |e| e.key) {
            state.entries.remove(i);
        }
    }
    Ok(())
}
fn prefix(shared: &Shared, input: &SongLuaInputEvent<'_>, hook: bool) -> Prefix {
    let mut cursor = Cursor::default();
    let mut state = shared.lock().unwrap();
    if !hook && !state.dispatching {
        if state.entries.is_empty() {
            return Prefix::Done(false);
        }
        if state.entries.first().is_some_and(|e| e.compiled.is_some()) {
            let frame = predicate::Frame::new(input);
            let values = frame.values(input);
            for entry in &mut state.entries {
                let Some(compiled) = entry.compiled.as_mut() else {
                    return Prefix::Lua(cursor);
                };
                cursor.previous = Some(entry.key);
                cursor.next += 1;
                if compiled.evaluate_frame(&values) {
                    return Prefix::Done(true);
                }
            }
            return Prefix::Done(false);
        }
    }
    Prefix::Lua(cursor)
}
// The idle VM stack introduces no authored call frame. In particular, debug
// call hooks observe the input callbacks, not an internal dispatcher closure.
fn dispatch_stack(
    stack: &mut NativeStack<'_, '_>,
    state: &Shared,
    owner: Value,
    input: &SongLuaInputEvent<'_>,
) -> Result<(bool, Vec<String>), Error> {
    let hook = stack.hook(stack.thread_id())?;
    let mut cursor = match prefix(state, input, !matches!(hook.function, NativeHook::None)) {
        Prefix::Done(handled) => return Ok((handled, Vec::new())),
        Prefix::Lua(cursor) => cursor,
    };
    let base = stack.top();
    let event = create_event(stack, input)?;
    let event_top = stack.top();
    state.lock().unwrap().dispatching = true;
    let result = (|| {
        let mut handled = false;
        let mut errors = Vec::new();
        loop {
            let next = {
                let state = state.lock().unwrap();
                let i = match cursor.previous {
                    None => 0,
                    Some(key)
                        if cursor.next > 0
                            && state
                                .entries
                                .get(cursor.next - 1)
                                .is_some_and(|e| e.key == key) =>
                    {
                        cursor.next
                    }
                    Some(key) => state.entries.partition_point(|e| e.key <= key),
                };
                cursor.next = i + 1;
                state.entries.get(i).map(|e| (e.key, e.function))
            };
            let Some((key, f)) = next else {
                break;
            };
            cursor.previous = Some(key);
            stack.push(f)?;
            stack.push(event)?;
            match stack.call(1, Some(1)) {
                Ok(_) => handled = stack.to_boolean(-1),
                Err(error) if matches!(error.kind(), ErrorKind::Limit | ErrorKind::Memory) => {
                    return Err(error);
                }
                Err(error) => errors.push(format!("Error running input callback: {error}")),
            }
            stack.set_top(event_top)?;
            if handled {
                break;
            }
        }
        Ok((handled, errors))
    })();
    state.lock().unwrap().dispatching = false;
    if result.is_ok() {
        finish_stack(stack, state, owner)?;
        stack.set_top(base)?;
    }
    result
}
fn finish_stack(
    stack: &mut NativeStack<'_, '_>,
    shared: &Shared,
    owner: Value,
) -> Result<(), Error> {
    loop {
        let entry = {
            let state = shared.lock().unwrap();
            state.removals.last().copied().map(|key| {
                (
                    key,
                    state
                        .entries
                        .binary_search_by_key(&key, |e| e.key)
                        .ok()
                        .map(|i| state.entries[i].function),
                )
            })
        };
        let Some((key, f)) = entry else {
            return Ok(());
        };
        if let Some(f) = f {
            let base = stack.top();
            stack.push(owner)?;
            stack.get_field(-1, b"functions")?;
            stack.push(f)?;
            stack.push(Value::Nil)?;
            stack.set_table(-3, true)?;
            stack.set_top(base)?;
        }
        let mut state = shared.lock().unwrap();
        if let Ok(i) = state.entries.binary_search_by_key(&key, |e| e.key) {
            state.entries.remove(i);
        }
        state.removals.pop();
    }
}
fn create_event(
    stack: &mut NativeStack<'_, '_>,
    input: &SongLuaInputEvent<'_>,
) -> Result<Value, Error> {
    fn put(
        stack: &mut NativeStack<'_, '_>,
        table: usize,
        name: &str,
        value: Value,
    ) -> Result<(), Error> {
        stack.push_bytes(name.as_bytes())?;
        stack.push(value)?;
        stack.set_table(table as isize, true)
    }
    fn text(
        stack: &mut NativeStack<'_, '_>,
        table: usize,
        name: &str,
        value: &str,
    ) -> Result<(), Error> {
        stack.push_bytes(name.as_bytes())?;
        stack.push_bytes(value.as_bytes())?;
        stack.set_table(table as isize, true)
    }
    stack.create_table(0, 0)?;
    let event = stack.top();
    stack.create_table(0, 0)?;
    let device = stack.top();
    text(stack, device, "device", input.device)?;
    text(stack, device, "button", input.device_button)?;
    put(stack, device, "level", Value::Number(input.level.into()))?;
    put(stack, device, "z", Value::Number(input.z.into()))?;
    put(stack, device, "down", Value::Boolean(input.down))?;
    put(stack, device, "ago", Value::Number(input.ago))?;
    put(
        stack,
        device,
        "is_joystick",
        Value::Boolean(input.is_joystick),
    )?;
    put(stack, device, "is_mouse", Value::Boolean(input.is_mouse))?;
    put(
        stack,
        event,
        "DeviceInput",
        stack.value(device as isize).unwrap(),
    )?;
    stack.set_top(event)?;
    if let Some(p) = input.controller {
        text(
            stack,
            event,
            "controller",
            &format!("GameController_{}", p + 1),
        )?;
    } else {
        put(stack, event, "controller", Value::Nil)?;
    }
    text(stack, event, "button", input.button)?;
    text(stack, event, "GameButton", input.game_button)?;
    text(stack, event, "type", event_kind(input.kind))?;
    if let Some(p) = input.player {
        text(
            stack,
            event,
            "PlayerNumber",
            &format!("PlayerNumber_P{}", p + 1),
        )?;
    } else {
        put(stack, event, "PlayerNumber", Value::Nil)?;
    }
    put(stack, event, "MultiPlayer", Value::Nil)?;
    Ok(stack.value(event as isize).unwrap())
}

#[cfg(feature = "test-support")]
pub(super) mod testing {
    use super::*;
    pub fn input_functions(runtime: &RustSongRuntime) -> Option<Vec<Value>> {
        let state = runtime.input.state.lock().unwrap();
        (!state.dispatching).then(|| state.entries.iter().map(|e| e.function).collect())
    }
    pub fn input_compiled_count(runtime: &RustSongRuntime) -> usize {
        runtime
            .input
            .state
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|e| e.compiled.is_some())
            .count()
    }
    pub fn seed_pending_input_removal(runtime: &mut RustSongRuntime, function: Value) {
        let key = runtime
            .vm
            .object_pointer(function)
            .expect("input function identity") as usize;
        let mut state = runtime.input.state.lock().unwrap();
        state.dispatching = true;
        state.removals.push(key);
    }
    pub fn input_dispatch_state(runtime: &RustSongRuntime) -> (bool, usize, usize) {
        let state = runtime.input.state.lock().unwrap();
        (state.dispatching, state.removals.len(), state.entries.len())
    }
    struct Control {
        state: Shared,
        event: OwnedEvent,
        reset: bool,
    }
    // Drive real registry replacement/reentry through the same VM activation.
    pub fn install_input_controls(
        runtime: &mut RustSongRuntime,
        event: &SongLuaInputEvent<'_>,
    ) -> Result<(), Error> {
        let owner = runtime.vm.rooted(runtime.input.root)?;
        for (name, reset) in [("NESTED", false), ("RESET", true)] {
            let f = runtime.vm.new_host_function_with_data(
                control,
                &[owner],
                Control {
                    state: runtime.input.state.clone(),
                    event: OwnedEvent::new(event),
                    reset,
                },
            )?;
            runtime.vm.set_global(name.as_bytes(), f)?;
        }
        Ok(())
    }
    fn control(call: &mut HostCall<'_>) -> Result<(), Error> {
        let data = call.function_data::<Control>().unwrap();
        let state = data.state.clone();
        let event = data.event.clone();
        let reset = data.reset;
        let owner = call.captures()[0];
        if reset {
            let screen = call.arguments().first().copied().unwrap_or(Value::Nil);
            return bind(&mut call.native_stack()?, &state, owner, screen);
        }
        let result = {
            let mut stack = call.native_stack()?;
            dispatch_stack(&mut stack, &state, owner, &event.event())?
        };
        call.push(Value::Boolean(result.0))?;
        call.push(Value::Number(result.1.len() as f64))
    }
}
