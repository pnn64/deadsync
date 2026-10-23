//! VM-free screen input interactions with explicit actor-state ownership.
use crate::{SongLuaInputEvent, SongLuaOverlayState, SongLuaTrackedActorTarget};
use mlua::program::{self, Action, Binding, Datum, Field, FieldValue, Group, Parameter, Types};
use mlua::{InspectionValue, Lua, Table};
use rustc_hash::FxHashMap;

/// An authored native actor write, in callback execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaActorInputAction {
    /// Actor indices address overlays first, followed by tracked actors.
    Visible { actor: usize, visible: bool },
}

/// Primitive state captured from a raw scalar table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongLuaInputScalar {
    Boolean(bool),
    Number(f64),
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
    scalar_fields: Box<[(usize, Box<[u8]>)]>,
    states: Box<[SongLuaOverlayState]>,
    overlay_count: usize,
    tracked: Box<[SongLuaTrackedActorTarget]>,
}

impl SongLuaCompiledActorInputs {
    /// Execute an input and apply its actor writes without Lua or allocation.
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
        for index in 0..self.callbacks.len() {
            let handled = self
                .callbacks
                .run(index, &values, |tag, args| {
                    let Some(Datum::Boolean(visible)) = args.get(1) else {
                        unreachable!("verified visible(boolean) action")
                    };
                    let actor = tag as usize;
                    self.states[actor].visible = visible;
                    observe(SongLuaActorInputAction::Visible { actor, visible });
                })
                .expect("input registry index")
                .expect("complete typed input frame")
                .is_truthy();
            if handled {
                return true;
            }
        }
        false
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
    pub fn scalar_states(&self) -> impl Iterator<Item = (usize, &[u8], SongLuaInputScalar)> + '_ {
        self.scalar_fields
            .iter()
            .enumerate()
            .map(|(index, (table, key))| {
                let value = match self.callbacks.cell(index).expect("bound scalar cell") {
                    Datum::Boolean(value) => SongLuaInputScalar::Boolean(value),
                    Datum::Number(value) => SongLuaInputScalar::Number(value),
                    _ => unreachable!("scalar table cell"),
                };
                (*table, key.as_ref(), value)
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
    if !crate::lua_util::can_compile_actor_input(lua) {
        return None;
    }
    let callbacks = crate::input::functions(lua)?;
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
    u32::try_from(tables.len()).ok()?.checked_add(2)?;
    let mut bindings = crate::input::compiled::bindings();
    let first_state_object = u32::try_from(tables.len()).ok()?.checked_add(2)?;
    let mut state_ids = FxHashMap::<usize, u32>::default();
    let mut scalar_fields = Vec::new();
    let mut pending = Vec::new();
    let mut captures = Vec::with_capacity(callbacks.len());
    for callback in &callbacks {
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
                    table.for_each::<InspectionValue, InspectionValue>(|key, value| {
                        let InspectionValue::Bytes(name) = key else {
                            return Err(mlua::Error::runtime("native input state needs byte keys"));
                        };
                        let cell = match value {
                            InspectionValue::Boolean(value) => program::Cell::Boolean(value),
                            InspectionValue::Integer(value) => program::Cell::Number(value as f64),
                            InspectionValue::Number(value) => program::Cell::Number(value),
                            InspectionValue::Bytes(value) => {
                                bindings.fields.push(Field {
                                    object,
                                    name,
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
                        scalar_fields.push((ordinal, name.clone()));
                        bindings.fields.push(Field {
                            object,
                            name,
                            value: FieldValue::Value(Binding::Cell(index)),
                        });
                        Ok(())
                    })?;
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
        let Some(target) = crate::actor_classes::visible_target(lua, tables[index]).ok()? else {
            continue;
        };
        let &target = indices.get(&target)?;
        // A copied setter writes its captured target, not the receiver.
        if target != index
            && let Some(mt) = tables[target].metatable()
        {
            match mt.raw_get::<InspectionValue>("__newindex").ok()? {
                InspectionValue::Nil => {}
                InspectionValue::Function(method)
                    if crate::actor_methods::identity(lua, &method).ok()?
                        == Some(crate::actor_methods::Method::Assign) => {}
                _ => return None,
            }
        }
        pending.push(target);
        let receiver = index as u32 + 2;
        bindings.fields.push(Field {
            object: receiver,
            name: b"visible".as_slice().into(),
            value: FieldValue::Action(Action {
                tag: target as u32,
                parameters: vec![Parameter::Object(receiver), Parameter::Type(Types::BOOLEAN)],
                result: Some(Binding::Object(target as u32 + 2)),
            }),
        });
    }
    // Reuse the complete schema. Only each callback's capture vector changes.
    let mut programs = Vec::with_capacity(callbacks.len());
    for (callback, upvalues) in callbacks.iter().zip(captures) {
        bindings.upvalues = upvalues;
        programs.push(program::compile(callback, &bindings).ok()?);
    }
    Some(SongLuaCompiledActorInputs {
        callbacks: Group::new(programs).ok()?,
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
