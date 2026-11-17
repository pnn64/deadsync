//! Existing VM-free input snapshots, using the Rust bytecode program compiler.
use super::super::baked_program::{
    self as program, Action, Binding, Datum, Field, FieldValue, Group, Parameter, Types,
};
use super::*;
use crate::{
    SongLuaActorInputAction, SongLuaInputEvent, SongLuaInputScalar, SongLuaOverlayState,
    SongLuaTrackedActorTarget,
};
use deadlib_lua::NativeStack;
use rustc_hash::FxHashMap;
mod tables;
pub(super) mod update;
pub use update::CompiledActorInteractions;

/// A closed snapshot of screen input callbacks and their actor states.
///
/// Owns no Lua VM, function or table. Each call receives real input, executes
/// authored branching and writes native actor state. Callback ordering and
/// first-return-value truthiness match the screen dispatcher. Unsupported
/// interactions are rejected as a group, before any callback body runs.
/// Other scene lifecycle work is not part of this input-only snapshot.
pub struct CompiledActorInputs {
    callbacks: Group,
    input_count: usize,
    scalar_fields: Box<[(usize, Box<[u8]>)]>,
    states: Box<[SongLuaOverlayState]>,
    overlay_count: usize,
    tracked: Box<[SongLuaTrackedActorTarget]>,
}

impl CompiledActorInputs {
    /// Execute an input and apply its actor writes without Lua. First writes of
    /// new Boolean-table keys may allocate native storage.
    pub fn input(&mut self, input: &SongLuaInputEvent<'_>) -> bool {
        self.input_with_actions(input, |_| {})
    }

    /// Execute and observe each applied write in authored order.
    /// The observer does not expose or alter this owner's state.
    pub fn input_with_actions(
        &mut self,
        input: &SongLuaInputEvent<'_>,
        mut observe: impl FnMut(SongLuaActorInputAction),
    ) -> bool {
        let frame = super::super::input::Frame::new(input);
        let values = frame.values(input);
        for index in 0..self.input_count {
            if self.run(index, &values, &mut observe) {
                return true;
            }
        }
        false
    }

    fn run(
        &mut self,
        index: usize,
        values: &[Datum<'_>],
        mut observe: impl FnMut(SongLuaActorInputAction),
    ) -> bool {
        self.callbacks
            .run_truthy(index, values, |tag, args| {
                let actor = (tag >> 2) as usize;
                match tag & 3 {
                    0 => {
                        let Some(Datum::Boolean(visible)) = args.get(1) else {
                            unreachable!("verified visible(boolean) action")
                        };
                        self.states[actor].visible = visible;
                        observe(SongLuaActorInputAction::Visible { actor, visible });
                    }
                    axis @ 1..=2 => {
                        let Some(Datum::Number(value)) = args.get(1) else {
                            unreachable!("verified position(number) action")
                        };
                        let value = value as f32;
                        if axis == 1 {
                            self.states[actor].x = value;
                            observe(SongLuaActorInputAction::X {
                                actor,
                                bits: value.to_bits(),
                            });
                        } else {
                            self.states[actor].y = value;
                            observe(SongLuaActorInputAction::Y {
                                actor,
                                bits: value.to_bits(),
                            });
                        }
                    }
                    _ => unreachable!("verified setter tag"),
                }
            })
            .expect("input registry index")
            .expect("complete typed input frame")
    }

    /// Overlay states, using the live scene's original overlay indices.
    pub fn states(&self) -> &[SongLuaOverlayState] {
        &self.states[..self.overlay_count]
    }

    /// Host actor states with their original gameplay target identities.
    pub fn tracked_states(
        &self,
    ) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.tracked
            .iter()
            .copied()
            .zip(self.states[self.overlay_count..].iter().copied())
    }

    /// Captured scalar cells by table identity ordinal and byte key. Distinct
    /// tables keep distinct ordinals even when their initial values are equal.
    pub fn scalar_states(
        &self,
    ) -> impl Iterator<Item = (usize, &[u8], SongLuaInputScalar<'_>)> + '_ {
        self.scalar_fields
            .iter()
            .enumerate()
            .map(|(index, (table, key))| {
                let value = match self.callbacks.cell(index).expect("bound scalar cell") {
                    Datum::Boolean(value) => SongLuaInputScalar::Boolean(value),
                    Datum::Number(value) => SongLuaInputScalar::Number(value),
                    Datum::Bytes(value) => SongLuaInputScalar::Bytes(value),
                    _ => unreachable!("scalar table cell"),
                };
                (*table, key.as_ref(), value)
            })
    }

    /// Declared native table identities and their present Boolean entries.
    /// Nil entries are absent; byte keys and false values remain exact.
    pub fn boolean_states(&self) -> impl Iterator<Item = (u32, &[u8], bool)> + '_ {
        self.callbacks.boolean_objects().flat_map(|object| {
            self.callbacks
                .boolean_entries(object)
                .into_iter()
                .flatten()
                .map(move |(key, value)| (object, key, value))
        })
    }

    /// Native owned payload, excluding allocator and Arc bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.callbacks.retained_bytes() - std::mem::size_of::<Group>()
            + std::mem::size_of_val(self.states.as_ref())
            + std::mem::size_of_val(self.tracked.as_ref())
            + std::mem::size_of_val(self.scalar_fields.as_ref())
            + self
                .scalar_fields
                .iter()
                .map(|(_, key)| key.len())
                .sum::<usize>()
    }
}

// Local, non-executing inspection for the existing snapshot schema.
fn raw(stack: &mut NativeStack<'_, '_>, table: TableId, key: &[u8]) -> Result<Value, Error> {
    let top = stack.top();
    let result = (|| {
        stack.push(Value::Table(table))?;
        stack.push_bytes(key)?;
        stack.get_table((top + 1) as isize, true)?;
        Ok(stack.value(-1).unwrap())
    })();
    stack.set_top(top)?;
    result
}
fn metatable(stack: &mut NativeStack<'_, '_>, table: TableId) -> Result<Option<TableId>, Error> {
    let top = stack.top();
    let result = (|| {
        stack.push(Value::Table(table))?;
        if !stack.get_metatable(-1)? {
            return Ok(None);
        }
        let Some(Value::Table(table)) = stack.value(-1) else {
            unreachable!()
        };
        Ok(Some(table))
    })();
    stack.set_top(top)?;
    result
}
fn raw_len(stack: &mut NativeStack<'_, '_>, table: TableId) -> Result<usize, Error> {
    let top = stack.top();
    stack.push(Value::Table(table))?;
    let result = stack.object_len(-1);
    stack.set_top(top)?;
    result
}
fn byte_string(stack: &mut NativeStack<'_, '_>, value: Value) -> Result<Option<Box<[u8]>>, Error> {
    if !matches!(value, Value::String(_)) {
        return Ok(None);
    }
    let top = stack.top();
    stack.push(value)?;
    let result = stack.to_bytes(-1)?.map(Box::<[u8]>::from);
    stack.set_top(top)?;
    Ok(result)
}
fn raw_entries(
    stack: &mut NativeStack<'_, '_>,
    table: TableId,
) -> Result<Vec<(Value, Value)>, Error> {
    let top = stack.top();
    let result = (|| {
        stack.push(Value::Table(table))?;
        let index = (top + 1) as isize;
        stack.push(Value::Nil)?;
        let mut rows = Vec::new();
        while stack.next_table(index)? {
            rows.push((stack.value(-2).unwrap(), stack.value(-1).unwrap()));
            stack.set_top(stack.top() - 1)?;
        }
        Ok(rows)
    })();
    stack.set_top(top)?;
    result
}
fn dump(stack: &mut NativeStack<'_, '_>, function: Value) -> Result<Option<Vec<u8>>, Error> {
    let top = stack.top();
    stack.push(function)?;
    let mut bytes = Vec::new();
    let result = stack.dump(|_, block| {
        bytes.extend_from_slice(block);
        Ok(0)
    });
    stack.set_top(top)?;
    Ok((result? == 0).then_some(bytes))
}
fn raw_global(stack: &mut NativeStack<'_, '_>, key: &[u8]) -> Result<Value, Error> {
    let top = stack.top();
    stack.push_bytes(key)?;
    let result = stack
        .get_table(-10002, true)
        .map(|()| stack.value(-1).unwrap());
    stack.set_top(top)?;
    result
}

// Mirror actor_classes::setter_targets. Read only raw fields and original Rust
// identities; never execute a user lookup, assignment hook or callback body.
fn setter_targets(
    stack: &mut NativeStack<'_, '_>,
    actor: TableId,
) -> Result<[Option<(scalar_methods::Setter, TableId)>; 3], Error> {
    use scalar_methods::{Method, Setter};
    fn identity(stack: &NativeStack<'_, '_>, function: Value) -> Option<scalar_methods::Method> {
        stack.store_ref().scalar_methods.identity(function)
    }
    let mut selected = [None; 3];
    let mut inherit = [false; 3];
    let lookup = if let Some(mt) = metatable(stack, actor)? {
        let assign = raw(stack, mt, b"__newindex")?;
        if assign != Value::Nil && identity(stack, assign) != Some(Method::Assign) {
            return Ok([None; 3]);
        }
        let lookup = raw(stack, mt, b"__index")?;
        if lookup == Value::Nil {
            false
        } else if identity(stack, lookup) == Some(Method::Lookup) {
            true
        } else {
            return Ok([None; 3]);
        }
    } else {
        false
    };
    for (index, setter) in Setter::ALL.into_iter().enumerate() {
        let value = raw(stack, actor, setter.name().as_bytes())?;
        match value {
            Value::Function(_) | Value::Native(_) => selected[index] = identity(stack, value),
            Value::Nil => inherit[index] = lookup,
            _ => {}
        }
    }
    if inherit.iter().any(|flag| *flag) {
        let inherited = (|| -> Result<Option<[Option<Method>; 3]>, Error> {
            let kind = raw(stack, actor, b"__songlua_actor_type")?;
            let kind = if kind == Value::Nil {
                b"Actor".to_vec().into_boxed_slice()
            } else if let Some(kind) = byte_string(stack, kind)? {
                kind
            } else {
                return Ok(None);
            };
            let Ok(kind) = std::str::from_utf8(&kind) else {
                return Ok(None);
            };
            if !matches!(
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
            ) {
                return Ok(None);
            }
            let public = match kind {
                "Sprite" | "Quad" => "Sprite",
                "WrapperState" => "ActorFrame",
                kind => kind,
            };
            let Value::Table(mut class) = raw_global(stack, public.as_bytes())? else {
                return Ok(None);
            };
            let mut seen = smallvec::SmallVec::<[TableId; 8]>::new();
            let mut found = [None; 3];
            let mut pending = inherit;
            loop {
                if seen.len() == 64 || seen.contains(&class) {
                    return Ok(None);
                }
                seen.push(class);
                for (index, setter) in Setter::ALL.into_iter().enumerate() {
                    if !pending[index] {
                        continue;
                    }
                    match raw(stack, class, setter.name().as_bytes())? {
                        value @ (Value::Function(_) | Value::Native(_)) => {
                            found[index] = identity(stack, value);
                            pending[index] = false;
                        }
                        Value::Nil => {}
                        _ => pending[index] = false,
                    }
                }
                let Some(mt) = metatable(stack, class)? else {
                    break;
                };
                match raw(stack, mt, b"__index")? {
                    Value::Nil => break,
                    Value::Table(next) => class = next,
                    _ => return Ok(None),
                }
            }
            Ok(Some(found))
        })()?;
        if let Some(inherited) = inherited {
            for index in 0..3 {
                if inherit[index] {
                    selected[index] = inherited[index];
                }
            }
        }
    }
    let mut forwards = [None; 3];
    if let Value::Table(methods) = raw(stack, actor, b"__songlua_bound_methods")? {
        for setter in Setter::ALL {
            if !selected.contains(&Some(Method::Forward(setter))) {
                continue;
            }
            let function = raw(stack, methods, setter.name().as_bytes())?;
            if let Some(Method::Bound(operation, target)) = identity(stack, function) {
                forwards[setter as usize] = Some((operation, target));
            }
        }
    }
    Ok(selected.map(|method| match method {
        Some(Method::Bound(setter, target)) => Some((setter, target)),
        Some(Method::Forward(setter)) => forwards[setter as usize],
        _ => None,
    }))
}

// The caller gates playback/capture/hooks and restores the complete
// idle native-stack window on every result. Original callbacks stay rooted in
// Input's private map; scene actors and updates have traced scene/plan owners.
#[allow(
    clippy::too_many_arguments,
    reason = "Keep the existing snapshot schema and shared actor/update inventories explicit."
)]
fn compile_with_updates(
    stack: &mut NativeStack<'_, '_>,
    inputs: &[Value],
    actors: &[TableId],
    states: &[SongLuaOverlayState],
    tracked: &[(SongLuaTrackedActorTarget, TableId, SongLuaOverlayState)],
    updates: &[(Value, usize)],
    reserved: &[TableId],
) -> Option<CompiledActorInputs> {
    let input_count = inputs.len();
    let callback_count = input_count + updates.len();
    let callbacks = || {
        inputs
            .iter()
            .chain(updates.iter().map(|(function, _)| function))
    };
    let tables = actors
        .iter()
        .copied()
        .chain(tracked.iter().map(|(_, table, _)| *table))
        .collect::<Vec<_>>();
    let indices = tables
        .iter()
        .copied()
        .enumerate()
        .map(|(index, table)| (table, index))
        .collect::<FxHashMap<_, _>>();
    if indices.len() != tables.len() || actors.len() != states.len() {
        return None;
    }
    u32::try_from(tables.len())
        .ok()?
        .checked_mul(4)?
        .checked_add(2)?;
    let mut bindings = super::super::input::bindings();
    let first = u32::try_from(tables.len()).ok()?.checked_add(2)?;
    let mut ids = FxHashMap::<TableId, u32>::default();
    let mut byte_fields = Vec::new();
    let mut pending = updates.iter().map(|(_, actor)| *actor).collect::<Vec<_>>();
    let mut captures = Vec::with_capacity(callback_count);
    for callback in callbacks() {
        if !matches!(callback, Value::Function(_)) {
            return None;
        }
        stack.push(*callback).ok()?;
        let function_index = stack.top();
        let mut upvalues = Vec::new();
        for number in 1.. {
            if stack
                .get_upvalue(function_index as isize, number)
                .ok()?
                .is_none()
            {
                break;
            }
            let Value::Table(table) = stack.value(-1)? else {
                return None;
            };
            let object = if let Some(&index) = indices.get(&table) {
                pending.push(index);
                index as u32 + 2
            } else if reserved.contains(&table) {
                return None;
            } else if let Some(&object) = ids.get(&table) {
                object
            } else {
                if metatable(stack, table).ok()?.is_some() {
                    return None;
                }
                let object = u32::try_from(ids.len())
                    .ok()
                    .and_then(|offset| first.checked_add(offset))?;
                ids.insert(table, object);
                if raw_len(stack, table).ok()? > 0 {
                    tables::capture_array(
                        stack,
                        table,
                        object,
                        first,
                        &indices,
                        reserved,
                        &mut ids,
                        &mut bindings,
                    )
                    .ok()?;
                } else {
                    for (key, value) in raw_entries(stack, table).ok()? {
                        let name = byte_string(stack, key).ok()??;
                        let cell = match value {
                            Value::Boolean(value) => program::Cell::Boolean(value),
                            Value::Number(value) => program::Cell::Number(value),
                            Value::String(_) => {
                                let bytes = byte_string(stack, value).ok()??;
                                byte_fields.push(bindings.fields.len());
                                bindings.fields.push(Field {
                                    object,
                                    name: name.into_vec().into(),
                                    value: FieldValue::Value(Binding::Literal(
                                        program::Literal::Bytes(bytes),
                                    )),
                                });
                                continue;
                            }
                            _ => return None,
                        };
                        let index = bindings.cells.len();
                        bindings.cells.push(cell);
                        bindings.fields.push(Field {
                            object,
                            name: name.into_vec().into(),
                            value: FieldValue::Value(Binding::Cell(index)),
                        });
                    }
                }
                object
            };
            upvalues.push(Binding::Object(object));
            stack.set_top(function_index).ok()?;
        }
        stack.set_top(function_index - 1).ok()?;
        captures.push(upvalues);
    }
    let mut visited = vec![false; tables.len()];
    while let Some(index) = pending.pop() {
        if visited[index] {
            continue;
        }
        visited[index] = true;
        let targets = setter_targets(stack, tables[index]).ok()?;
        let mut motion = None;
        'method: for (requested, selected) in scalar_methods::Setter::ALL.into_iter().zip(targets) {
            let Some((setter, identity)) = selected else {
                continue;
            };
            let Some(&target) = indices.get(&identity) else {
                continue;
            };
            if setter != scalar_methods::Setter::Visible {
                let ready = match motion {
                    Some((previous, ready)) if previous == target => ready,
                    _ => {
                        // Implement against Rust's canonical queue/effect owner,
                        // plus the same raw override checks as the native gate.
                        let ready = position_ready(stack, tables[target]).ok()?;
                        motion = Some((target, ready));
                        ready
                    }
                };
                if !ready {
                    continue;
                }
            }
            if target != index
                && let Some(mt) = metatable(stack, tables[target]).ok()?
            {
                let method = raw(stack, mt, b"__newindex").ok()?;
                if method != Value::Nil
                    && stack.store().scalar_methods.identity(method)
                        != Some(scalar_methods::Method::Assign)
                {
                    continue 'method;
                }
            }
            pending.push(target);
            let (opcode, parameters): (_, &'static [Parameter]) = match setter {
                scalar_methods::Setter::Visible => {
                    (0, &[Parameter::Receiver, Parameter::Type(Types::BOOLEAN)])
                }
                scalar_methods::Setter::X => {
                    (1, &[Parameter::Receiver, Parameter::Type(Types::NUMBER)])
                }
                scalar_methods::Setter::Y => {
                    (2, &[Parameter::Receiver, Parameter::Type(Types::NUMBER)])
                }
            };
            bindings.fields.push(Field {
                object: index as u32 + 2,
                name: requested.name().as_bytes().into(),
                value: FieldValue::Action(Action {
                    tag: ((target as u32) << 2) | opcode,
                    parameters: parameters.into(),
                    result: Some(Binding::Object(target as u32 + 2)),
                }),
            });
        }
    }
    let mut bytecode = if byte_fields.is_empty() {
        Vec::new()
    } else {
        (0..callback_count).map(|_| None).collect::<Vec<_>>()
    };
    let mut lower = |bindings: &program::Bindings| {
        let compiler = program::Compiler::new(bindings).ok()?;
        let mut batch = (callback_count >= 8).then(|| compiler.batch());
        let mut programs = Vec::with_capacity(callback_count);
        for (index, (callback, upvalues)) in callbacks().zip(&captures).enumerate() {
            let update_arguments;
            let arguments = if index < input_count {
                &bindings.arguments[..]
            } else {
                update_arguments = [
                    Binding::Object(updates[index - input_count].1 as u32 + 2),
                    Binding::Input {
                        slot: 0,
                        types: Types::NUMBER,
                    },
                ];
                &update_arguments[..]
            };
            let temporary;
            let bytes = if bytecode.is_empty() {
                temporary = dump(stack, *callback).ok()??;
                &temporary[..]
            } else {
                let bytes = &mut bytecode[index];
                if bytes.is_none() {
                    *bytes = Some(dump(stack, *callback).ok()??);
                }
                &bytes.as_ref()?[..]
            };
            let program = if let Some(batch) = &mut batch {
                batch.compile_bytecode(bytes, arguments, upvalues)
            } else {
                compiler.compile_bytecode(bytes, arguments, upvalues)
            };
            programs.push(program.ok()?);
        }
        Some(programs)
    };
    let programs = if let Some(programs) = lower(&bindings) {
        programs
    } else {
        if byte_fields.is_empty() {
            return None;
        }
        for field in byte_fields {
            let cell = bindings.cells.len();
            let FieldValue::Value(Binding::Literal(program::Literal::Bytes(value))) =
                std::mem::replace(
                    &mut bindings.fields[field].value,
                    FieldValue::Value(Binding::Cell(cell)),
                )
            else {
                unreachable!("captured byte literal");
            };
            let index = if bindings.strings.last().is_some_and(|bytes| *bytes == value) {
                bindings.strings.len() - 1
            } else {
                let index = bindings.strings.len();
                bindings.strings.push(value);
                index
            };
            bindings.cells.push(program::Cell::Bytes(index));
        }
        lower(&bindings)?
    };
    let mut scalar_fields = vec![(0usize, Box::<[u8]>::default()); bindings.cells.len()];
    for field in &mut bindings.fields {
        if let FieldValue::Value(Binding::Cell(index)) = &field.value {
            scalar_fields[*index] = (
                field.object.checked_sub(first)? as usize,
                std::mem::take(&mut field.name)
                    .into_owned()
                    .into_boxed_slice(),
            );
        }
    }
    Some(CompiledActorInputs {
        callbacks: Group::new(programs).ok()?,
        input_count,
        scalar_fields: scalar_fields.into_boxed_slice(),
        states: states
            .iter()
            .copied()
            .chain(tracked.iter().map(|(_, _, state)| *state))
            .collect(),
        overlay_count: states.len(),
        tracked: tracked.iter().map(|(target, _, _)| *target).collect(),
    })
}

fn position_ready(stack: &mut NativeStack<'_, '_>, actor: TableId) -> Result<bool, Error> {
    {
        let guard = stack.store();
        let Some(actor) = guard.actors.get(&actor) else {
            return Ok(false);
        };
        if guard.ease_probe_active
            || actor.raw_live_epoch
            || !actor.queue.is_empty()
            || !effects::snapshot_position_ready(actor)
        {
            return Ok(false);
        }
    }
    for key in [
        b"__songlua_command_queue".as_slice(),
        b"__songlua_state_spin",
    ] {
        match raw(stack, actor, key)? {
            Value::Nil => {}
            Value::Table(queue)
                if key == b"__songlua_command_queue" && raw_len(stack, queue)? == 0 => {}
            _ => return Ok(false),
        }
    }
    let mode = raw(stack, actor, b"__songlua_state_effect_mode")?;
    if mode != Value::Nil && byte_string(stack, mode)?.as_deref() != Some(b"none") {
        return Ok(false);
    }
    Ok(!raw(stack, actor, b"__songlua_state_rainbow")?.is_truthy())
}

fn advance_ready(stack: &mut NativeStack<'_, '_>, actor: TableId) -> Result<bool, Error> {
    {
        let guard = stack.store();
        if guard.actors.get(&actor).is_some_and(|actor| {
            !actor.queue.is_empty() || !effects::snapshot_position_ready(actor)
        }) {
            return Ok(false);
        }
    }
    for key in [
        b"__songlua_command_queue".as_slice(),
        b"__songlua_state_spin",
    ] {
        match raw(stack, actor, key)? {
            Value::Nil => {}
            Value::Table(queue)
                if key == b"__songlua_command_queue" && raw_len(stack, queue)? == 0 => {}
            _ => return Ok(false),
        }
    }
    let mode = raw(stack, actor, b"__songlua_state_effect_mode")?;
    if mode != Value::Nil && byte_string(stack, mode)?.as_deref() != Some(b"none") {
        return Ok(false);
    }
    Ok(!raw(stack, actor, b"__songlua_state_rainbow")?.is_truthy())
}

impl Actors {
    pub(crate) fn prepare_input_plan(&self, vm: &mut Vm, root: Value) -> Result<(), Error> {
        if !matches!(root, Value::Table(_)) {
            return Ok(());
        }
        let function = vm.new_host_function(
            |call| {
                let actor = table(call, call.captures()[2])?;
                lifecycle::retained_plan(call, actor)?;
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                root,
            ],
        )?;
        vm.call(function, &[]).map(|_| ())
    }

    pub(crate) fn compile_interaction_snapshot(
        &self,
        vm: &mut Vm,
        functions: &[Value],
        root: Value,
        actors: &[Value],
        states: &[SongLuaOverlayState],
        tracked: &[(SongLuaTrackedActorTarget, Value, SongLuaOverlayState)],
    ) -> Option<CompiledActorInteractions> {
        {
            let guard = vm.store();
            if !guard.playback || guard.action_capture.is_some() {
                return None;
            }
        }
        if log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) {
            return None;
        }
        let Value::Table(root) = root else {
            return None;
        };
        let plan = lifecycle::cached_for_root(vm.store(), root)?;
        let actor_table = |value| {
            if let Value::Table(table) = value {
                Some(table)
            } else {
                None
            }
        };
        let actors = actors
            .iter()
            .map(|value| actor_table(*value))
            .collect::<Option<Vec<_>>>()?;
        let tracked = tracked
            .iter()
            .map(|(target, value, state)| Some((*target, actor_table(*value)?, *state)))
            .collect::<Option<Vec<_>>>()?;
        vm.with_native_stack(|stack| {
            let top = stack.top();
            let result = (|| {
                if stack.hook(stack.thread_id()).ok()?.function != deadlib_lua::NativeHook::None {
                    return None;
                }
                stack
                    .push(Value::Table(lifecycle::plan_roots(&plan)))
                    .ok()?;
                let source = lifecycle::callback_jobs(&plan, root, |actor| {
                    advance_ready(stack, actor).unwrap_or(false)
                })?;
                update::compile_source(stack, functions, &source, &actors, states, &tracked)
            })();
            stack.set_top(top)?;
            Ok(result)
        })
        .ok()
        .flatten()
    }

    pub(crate) fn compile_input_snapshot(
        &self,
        vm: &mut Vm,
        functions: &[Value],
        actors: &[Value],
        states: &[SongLuaOverlayState],
        tracked: &[(SongLuaTrackedActorTarget, Value, SongLuaOverlayState)],
    ) -> Option<CompiledActorInputs> {
        {
            let guard = vm.store();
            if !guard.playback || guard.action_capture.is_some() {
                return None;
            }
        }
        if log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace) {
            return None;
        }
        let actor_table = |value| {
            if let Value::Table(table) = value {
                Some(table)
            } else {
                None
            }
        };
        let actors = actors
            .iter()
            .map(|value| actor_table(*value))
            .collect::<Option<Vec<_>>>()?;
        let tracked = tracked
            .iter()
            .map(|(target, value, state)| Some((*target, actor_table(*value)?, *state)))
            .collect::<Option<Vec<_>>>()?;
        vm.with_native_stack(|stack| {
            let top = stack.top();
            let result = if stack.hook(stack.thread_id())?.function != deadlib_lua::NativeHook::None
            {
                None
            } else {
                compile_with_updates(stack, functions, &actors, states, &tracked, &[], &[])
            };
            stack.set_top(top)?;
            Ok(result)
        })
        .ok()
        .flatten()
    }
}
