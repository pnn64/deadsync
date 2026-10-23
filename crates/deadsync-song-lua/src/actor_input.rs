//! VM-free screen input interactions with explicit actor-state ownership.
use crate::{SongLuaInputEvent, SongLuaOverlayState, SongLuaTrackedActorTarget};
use mlua::program::{self, Action, Binding, Datum, Field, FieldValue, Machine, Parameter, Types};
use mlua::{Function, InspectionValue, Lua, Table};
use rustc_hash::FxHashMap;

/// An authored native actor write, in callback execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaActorInputAction {
    /// Actor indices address overlays first, followed by tracked actors.
    Visible { actor: usize, visible: bool },
}

/// A closed snapshot of screen input callbacks and their actor states.
///
/// Owns no Lua VM, function or table. Each call receives real input, executes
/// authored branching and writes native actor state. Callback ordering and
/// first-return-value truthiness match the screen dispatcher. Unsupported
/// interactions are rejected as a group, before any callback body runs.
/// Other scene lifecycle work is not part of this input-only snapshot.
pub struct SongLuaCompiledActorInputs {
    callbacks: Box<[Machine]>,
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
        for callback in &mut self.callbacks {
            let handled = callback
                .run(&values, |tag, args| {
                    let Some(Datum::Boolean(visible)) = args.get(1) else {
                        unreachable!("verified visible(boolean) action")
                    };
                    let actor = tag as usize;
                    self.states[actor].visible = visible;
                    observe(SongLuaActorInputAction::Visible { actor, visible });
                })
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

    /// Native owned payload, excluding allocator and Arc bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + std::mem::size_of_val(self.states.as_ref())
            + std::mem::size_of_val(self.tracked.as_ref())
            + self
                .callbacks
                .iter()
                .map(Machine::retained_bytes)
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
    let callbacks = callbacks
        .iter()
        .map(|callback| bind(lua, callback, &tables, &indices))
        .collect::<Option<Box<[_]>>>()?;
    Some(SongLuaCompiledActorInputs {
        callbacks,
        states: states
            .iter()
            .copied()
            .chain(tracked.iter().map(|(_, _, state)| *state))
            .collect(),
        overlay_count: states.len(),
        tracked: tracked.iter().map(|(target, _, _)| *target).collect(),
    })
}

fn bind(
    lua: &Lua,
    callback: &Function,
    actors: &[&Table],
    indices: &FxHashMap<usize, usize>,
) -> Option<Machine> {
    let mut bindings = crate::input::compiled::bindings();
    let mut pending = Vec::new();
    let mut supported = true;
    callback
        .for_each_upvalue_as::<InspectionValue>(|_, _, value| {
            if let InspectionValue::Table(actor) = value
                && let Some(&index) = indices.get(&(actor.to_pointer() as usize))
            {
                bindings.upvalues.push(Binding::Object(index as u32 + 2));
                pending.push(index);
            } else {
                supported = false;
            }
            Ok(())
        })
        .ok()?;
    if !supported {
        return None;
    }
    let mut visited = vec![false; actors.len()];
    while let Some(index) = pending.pop() {
        if visited[index] {
            continue;
        }
        visited[index] = true;
        let Some(target) = crate::actor_classes::visible_target(lua, actors[index]).ok()? else {
            continue;
        };
        let &target = indices.get(&target)?;
        // A copied bound method writes its captured target, independently of
        // the receiver's metatable and the target's current visible method.
        if target != index
            && let Some(mt) = actors[target].metatable()
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
    Some(program::compile(callback, &bindings).ok()?.machine())
}
