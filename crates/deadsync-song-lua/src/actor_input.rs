//! VM-free screen input interactions with explicit actor-state ownership.
use crate::{SongLuaInputEvent, SongLuaOverlayState, SongLuaTrackedActorTarget};
use mlua::program::{self, Action, Binding, Datum, Field, FieldValue, Group, Parameter, Types};
use mlua::{InspectionValue, Lua, Table};
use rustc_hash::FxHashMap;

pub(crate) mod update;
mod tables;
pub use update::SongLuaCompiledActorInteractions;

/// An authored native actor write, in callback execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaActorInputAction {
    /// Actor indices address overlays first, followed by tracked actors.
    Visible {
        actor: usize,
        visible: bool,
    },
    /// The exact f32 bits written by the authored numeric setter.
    X {
        actor: usize,
        bits: u32,
    },
    Y {
        actor: usize,
        bits: u32,
    },
}

/// Primitive state captured from a raw scalar table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongLuaInputScalar<'a> {
    Boolean(bool),
    Number(f64),
    Bytes(&'a [u8]),
}

/// A closed snapshot of screen input callbacks and their actor states.
///
/// Owns no Lua VM, function or table. Each call receives real input, executes
/// authored branching and writes native actor state. Callback ordering and
/// first-return-value truthiness match the screen dispatcher. Unsupported
/// interactions are rejected as a group, before any callback body runs.
/// Other scene lifecycle work is not part of this input-only snapshot.

pub struct SongLuaCompiledActorInputs {
    callbacks: Group,
    input_count: usize,
    scalar_fields: Box<[(usize, Box<[u8]>)]>,
    states: Box<[SongLuaOverlayState]>,
    overlay_count: usize,
    tracked: Box<[SongLuaTrackedActorTarget]>,
}

impl SongLuaCompiledActorInputs {
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
        let frame = crate::input::compiled::Frame::new(input);
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
            self.callbacks.boolean_entries(object).into_iter().flatten()
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

pub(crate) fn compile(
    lua: &Lua,
    actors: &[Table],
    states: &[SongLuaOverlayState],
    tracked: &[(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)],
) -> Option<SongLuaCompiledActorInputs> {
    compile_with_updates(lua, actors, states, tracked, &[], &[])
}

fn compile_with_updates(
    lua: &Lua,
    actors: &[Table],
    states: &[SongLuaOverlayState],
    tracked: &[(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)],
    updates: &[(mlua::Function, usize)],
    reserved: &[usize],
) -> Option<SongLuaCompiledActorInputs> {
    if !crate::lua_util::can_compile_actor_input(lua) {
        return None;
    }
    let inputs = crate::input::functions(lua)?;
    let input_count = inputs.len();
    let callback_count = input_count + updates.len();
    // The snapshot roots inputs; the caller already roots every update function.
    let callbacks = || inputs.iter().chain(updates.iter().map(|(function, _)| function));
    let tables = actors
        .iter()
        .chain(tracked.iter().map(|(_, table, _)| table))
        .collect::<Vec<_>>();
    let indices = tables
        .iter()
        .enumerate()
        .map(|(index, table)| (table.to_pointer() as usize, index))
        .collect::<FxHashMap<_, _>>();
    // Repeated aliases in the host state inventory need one shared state owner.
    // Reject them until that ownership is represented rather than duplicating it.
    if indices.len() != tables.len() || actors.len() != states.len() {
        return None;
    }
    u32::try_from(tables.len())
        .ok()?
        .checked_mul(4)?
        .checked_add(2)?;
    let mut bindings = crate::input::compiled::bindings();
    let first_state_object = u32::try_from(tables.len()).ok()?.checked_add(2)?;
    let mut state_ids = FxHashMap::<usize, u32>::default();
    let mut byte_fields = Vec::new();
    let mut pending = updates.iter().map(|(_, actor)| *actor).collect::<Vec<_>>();
    let mut captures = Vec::with_capacity(callback_count);
    for callback in callbacks() {
        let mut upvalues = Vec::new();
        let mut supported = true;
        callback
            .for_each_upvalue_as::<InspectionValue>(|_, _, value| {
                let InspectionValue::Table(table) = value else {
                    supported = false;
                    return Ok(());
                };
                let pointer = table.to_pointer() as usize;
                let object = if let Some(&index) = indices.get(&pointer) {
                    pending.push(index);
                    index as u32 + 2
                } else if reserved.contains(&pointer) {
                    // Timing fields belong to the frozen traversal. A table
                    // outside the render inventory must not enter a second,
                    // mutable scalar store through a captured alias.
                    supported = false;
                    return Ok(());
                } else if let Some(&object) = state_ids.get(&pointer) {
                    object
                } else {
                    if table.metatable().is_some() {
                        supported = false;
                        return Ok(());
                    }
                    let ordinal = state_ids.len();
                    let Some(object) = u32::try_from(ordinal)
                        .ok()
                        .and_then(|index| first_state_object.checked_add(index))
                    else {
                        supported = false;
                        return Ok(());
                    };
                    state_ids.insert(pointer, object);
                    if table.raw_len() > 0 {
                        tables::capture_array(&table, object, first_state_object, &indices, reserved, &mut state_ids, &mut bindings)?;
                    } else {
                    table.for_each::<InspectionValue, InspectionValue>(|key, value| {
                        let InspectionValue::Bytes(name) = key else {
                            return Err(mlua::Error::runtime("native input state needs byte keys"));
                        };
                        let cell = match value {
                            InspectionValue::Boolean(value) => program::Cell::Boolean(value),
                            InspectionValue::Integer(value) => program::Cell::Number(value as f64),
                            InspectionValue::Number(value) => program::Cell::Number(value),
                            InspectionValue::Bytes(value) => {
                                byte_fields.push(bindings.fields.len());
                                bindings.fields.push(Field {
                                    object,
                                    name: name.into_vec().into(),
                                    value: FieldValue::Value(Binding::Literal(
                                        program::Literal::Bytes(value),
                                    )),
                                });
                                return Ok(());
                            }
                            _ => {
                                return Err(mlua::Error::runtime(
                                    "unsupported native input state value",
                                ));
                            }
                        };
                        let index = bindings.cells.len();
                        bindings.cells.push(cell);
                        bindings.fields.push(Field {
                            object,
                            name: name.into_vec().into(),
                            value: FieldValue::Value(Binding::Cell(index)),
                        });
                        Ok(())
                    })?;
                    }
                    object
                };
                upvalues.push(Binding::Object(object));
                Ok(())
            })
            .ok()?;
        if !supported {
            return None;
        }
        captures.push(upvalues);
    }
    let mut visited = vec![false; tables.len()];
    while let Some(index) = pending.pop() {
        if visited[index] {
            continue;
        }
        visited[index] = true;
        let targets = crate::actor_classes::setter_targets(lua, tables[index]).ok()?;
        let mut motion = None;
        'method: for (requested, selected) in
            crate::actor_methods::Setter::ALL.into_iter().zip(targets)
        {
            let Some((setter, pointer)) = selected else {
                continue;
            };
            let Some(&target) = indices.get(&pointer) else {
                continue;
            };
            if setter != crate::actor_methods::Setter::Visible {
                let ready = match motion {
                    Some((previous, ready)) if previous == target => ready,
                    _ => {
                        let ready =
                            crate::lua_util::can_compile_actor_position(lua, tables[target]);
                        motion = Some((target, ready));
                        ready
                    }
                };
                if !ready {
                    continue;
                }
            }
            // A copied setter writes and returns its captured target.
            if target != index
                && let Some(mt) = tables[target].metatable()
            {
                match mt.raw_get::<InspectionValue>("__newindex").ok()? {
                    InspectionValue::Nil => {}
                    InspectionValue::Function(method)
                        if crate::actor_methods::identity(lua, &method).ok()?
                            == Some(crate::actor_methods::Method::Assign) => {}
                    _ => continue 'method,
                }
            }
            pending.push(target);
            let receiver = index as u32 + 2;
            let (opcode, parameters): (_, &'static [Parameter]) = match setter {
                crate::actor_methods::Setter::Visible => {
                    (0, &[Parameter::Receiver, Parameter::Type(Types::BOOLEAN)])
                }
                crate::actor_methods::Setter::X => {
                    (1, &[Parameter::Receiver, Parameter::Type(Types::NUMBER)])
                }
                crate::actor_methods::Setter::Y => {
                    (2, &[Parameter::Receiver, Parameter::Type(Types::NUMBER)])
                }
            };
            bindings.fields.push(Field {
                object: receiver,
                name: requested.name().as_bytes().into(),
                value: FieldValue::Action(Action {
                    tag: ((target as u32) << 2) | opcode,
                    parameters: parameters.into(),
                    result: Some(Binding::Object(target as u32 + 2)),
                }),
            });
        }
    }
    // Reuse the complete schema. Only each callback's capture vector changes.
    // Retain dumps only when byte-state specialization can require a retry.
    // Populate on demand so an earlier rejection still avoids later Lua reads.
    let mut bytecode = if byte_fields.is_empty() {
        Vec::new()
    } else {
        (0..callback_count).map(|_| None).collect::<Vec<_>>()
    };
    let mut lower = |bindings: &program::Bindings| {
        let mut programs = Vec::with_capacity(callback_count);
        let compiler = program::Compiler::new(bindings).ok()?;
        let mut batch = (callback_count >= 8).then(|| compiler.batch());
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
            let program = if bytecode.is_empty() {
                if let Some(batch) = &mut batch {
                    batch.compile(callback, arguments, upvalues)
                } else {
                    compiler.compile(callback, arguments, upvalues)
                }
            } else {
                let bytes = &mut bytecode[index];
                if bytes.is_none() {
                    *bytes = Some(program::dump(callback).ok()?);
                }
                if let Some(batch) = &mut batch {
                    batch.compile_bytecode(bytes.as_ref()?, arguments, upvalues)
                } else {
                    compiler.compile_bytecode(bytes.as_ref()?, arguments, upvalues)
                }
            };
            programs.push(program.ok()?);
        }
        Some(programs)
    };
    // Keep existing immutable-byte specializations, including constant field
    // names. Only a rejected body needs the mutable byte-cell schema retry.
    let programs = if let Some(programs) = lower(&bindings) {
        programs
    } else {
        if byte_fields.is_empty() {
            return None;
        }
        for field_index in byte_fields {
            let cell = bindings.cells.len();
            let FieldValue::Value(Binding::Literal(program::Literal::Bytes(value))) =
                std::mem::replace(
                    &mut bindings.fields[field_index].value,
                    FieldValue::Value(Binding::Cell(cell)),
                )
            else {
                unreachable!("captured byte literal")
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
    // Every cell belongs to a raw captured table. Transfer its schema key
    // after lowering, preserving cell order without a second key allocation.
    let mut scalar_fields = vec![(0usize, Box::<[u8]>::default()); bindings.cells.len()];
    for field in &mut bindings.fields {
        if let FieldValue::Value(Binding::Cell(index)) = &field.value {
            scalar_fields[*index] = (
                field.object.checked_sub(first_state_object)? as usize,
                std::mem::take(&mut field.name)
                    .into_owned()
                    .into_boxed_slice(),
            );
        }
    }
    Some(SongLuaCompiledActorInputs {
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
