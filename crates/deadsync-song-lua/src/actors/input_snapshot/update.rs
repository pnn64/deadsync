//! Existing combined input/update owner, retaining native hierarchy arithmetic.
use super::*;

#[derive(Clone, Copy)]
pub(in crate::actors) enum SourceJob {
    Enter {
        actor: TableId,
        parent: Option<usize>,
        wrappers: bool,
    },
    Callback {
        actor: TableId,
        slot: usize,
    },
}

struct Node {
    actor: Option<usize>,
    parent: Option<usize>,
    wrappers: bool,
    rate: f32,
    sleeping: f32,
    paused: bool,
}
#[derive(Clone, Copy, Default)]
struct Delta {
    wrappers: Option<f64>,
    children: Option<f64>,
}
enum Job {
    Enter(usize),
    Callback { slot: usize, program: usize },
}

/// An independent snapshot of input callbacks and the authored root's update
/// callbacks, sharing their actual captured scalar tables and actor states.
///
/// Real input and elapsed time remain runtime data. Hierarchical update order,
/// static update rates and hibernation gates follow the loading scene's plan.
/// Queued commands, tweens, effects, drawing and other scene lifecycle behavior
/// are not compiled by this owner. Unsupported units return None at loading.
pub struct CompiledActorInteractions {
    owner: CompiledActorInputs,
    nodes: Box<[Node]>,
    jobs: Box<[Job]>,
    deltas: Box<[Delta]>,
}

impl CompiledActorInteractions {
    pub fn input(&mut self, event: &SongLuaInputEvent<'_>) -> bool {
        self.owner.input(event)
    }
    pub fn input_with_actions(
        &mut self,
        event: &SongLuaInputEvent<'_>,
        observe: impl FnMut(SongLuaActorInputAction),
    ) -> bool {
        self.owner.input_with_actions(event, observe)
    }
    pub fn states(&self) -> &[SongLuaOverlayState] {
        self.owner.states()
    }
    pub fn tracked_states(
        &self,
    ) -> impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)> + '_ {
        self.owner.tracked_states()
    }
    pub fn scalar_states(
        &self,
    ) -> impl Iterator<Item = (usize, &[u8], SongLuaInputScalar<'_>)> + '_ {
        self.owner.scalar_states()
    }

    /// Advance this callback unit using the actual parent delta in seconds.
    pub fn update(&mut self, delta_seconds: f64) {
        self.update_with_actions(delta_seconds, |_| {})
    }

    /// Advance and observe visibility writes in the original traversal order.
    pub fn update_with_actions(
        &mut self,
        delta_seconds: f64,
        mut observe: impl FnMut(SongLuaActorInputAction),
    ) {
        // The frozen plan enters each parent and slot before any read. Every
        // Enter overwrites both fields, including paused/hibernating slots.
        for job in &self.jobs {
            match *job {
                Job::Enter(slot) => {
                    let node = &mut self.nodes[slot];
                    let incoming = node.parent.map_or(Some(delta_seconds), |parent| {
                        if node.wrappers {
                            self.deltas[parent].wrappers
                        } else {
                            self.deltas[parent].children
                        }
                    });
                    let delta = incoming.and_then(|delta| {
                        if node.paused {
                            return None;
                        }
                        if node.sleeping <= 0.0 || node.sleeping.is_nan() {
                            return Some(f64::from(delta as f32));
                        }
                        let left = node.sleeping - delta as f32;
                        node.sleeping = left.max(0.0);
                        if left <= 0.0
                            && let Some(actor) = node.actor
                        {
                            self.owner.states[actor].hibernating = false;
                        }
                        (left <= 0.0).then(|| f64::from(-left))
                    });
                    self.deltas[slot] = Delta {
                        wrappers: delta,
                        children: delta.map(|delta| f64::from(delta as f32 * node.rate)),
                    };
                }
                Job::Callback { slot, program } => {
                    if let Some(delta) = self.deltas[slot].children {
                        // Update results never stop the remaining update callbacks.
                        self.owner
                            .run(program, &[Datum::Number(delta)], &mut observe);
                    }
                }
            }
        }
    }

    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<CompiledActorInputs>()
            + self.owner.retained_bytes()
            + std::mem::size_of_val(self.nodes.as_ref())
            + std::mem::size_of_val(self.jobs.as_ref())
            + std::mem::size_of_val(self.deltas.as_ref())
    }
}

// Source jobs come from the existing cached lifecycle
// plan, restricted to the authored root's Enter..Callback span. Plan roots and
// callback values remain on the caller's temporary rooted stack until lowering
// finishes. Building or executing a replacement traversal here is forbidden.
fn optional_number(stack: &mut NativeStack<'_, '_>, value: Value) -> Result<Option<f64>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let top = stack.top();
    stack.push(value)?;
    let number = stack.to_number(-1);
    stack.set_top(top)?;
    number
        .map(Some)
        .ok_or_else(|| stack.error(b"number expected"))
}

pub(super) fn compile_source(
    stack: &mut NativeStack<'_, '_>,
    store: &Shared,
    inputs: &[Value],
    source: &[SourceJob],
    actors: &[TableId],
    states: &[SongLuaOverlayState],
    tracked: &[(SongLuaTrackedActorTarget, TableId, SongLuaOverlayState)],
) -> Option<CompiledActorInteractions> {
    let indices = actors
        .iter()
        .copied()
        .chain(tracked.iter().map(|(_, actor, _)| *actor))
        .enumerate()
        .map(|(index, actor)| (actor, index))
        .collect::<FxHashMap<_, _>>();
    let mut reserved = smallvec::SmallVec::<[TableId; 8]>::new();
    let mut nodes = Vec::new();
    let mut jobs = Vec::new();
    let mut updates = Vec::new();
    for job in source {
        match *job {
            SourceJob::Enter {
                actor,
                parent,
                wrappers,
            } => {
                let index = indices.get(&actor).copied();
                if index.is_none() {
                    reserved.push(actor);
                }
                for key in [
                    b"__songlua_live_delta".as_slice(),
                    b"__songlua_recurring_update_command",
                ] {
                    if raw(stack, actor, key).ok()? != Value::Nil {
                        return None;
                    }
                }
                if raw(stack, actor, b"__songlua_update_pending")
                    .ok()?
                    .is_truthy()
                    || raw(stack, actor, b"__songlua_live_inactive")
                        .ok()?
                        .is_truthy()
                {
                    return None;
                }
                let rate = raw(stack, actor, b"__songlua_update_rate").ok()?;
                // Rust currently stores SetUpdateRate's canonical value in its
                // Actor, unlike the native table representation. Preserve that
                // value when no raw override exists; validate explicit values
                // with the same Lua number conversion as the native factory.
                let canonical_rate = store
                    .lock()
                    .unwrap()
                    .actors
                    .get(&actor)
                    .map_or(1., |actor| actor.update_rate);
                let rate = optional_number(stack, rate)
                    .ok()?
                    .map_or(canonical_rate, |rate| rate as f32);
                let sleeping = raw(stack, actor, b"__songlua_hibernate_seconds").ok()?;
                let sleeping = optional_number(stack, sleeping).ok()?.unwrap_or(0.) as f32;
                let paused = raw(stack, actor, b"__songlua_theme_hibernating")
                    .ok()?
                    .is_truthy();
                jobs.push(Job::Enter(nodes.len()));
                nodes.push(Node {
                    actor: index,
                    parent,
                    wrappers,
                    rate,
                    sleeping,
                    paused,
                });
            }
            SourceJob::Callback { actor, slot } => {
                match raw(stack, actor, b"__songlua_update_function").ok()? {
                    Value::Nil => {}
                    function @ Value::Function(_) => {
                        // Match native Function handles: retain even callbacks
                        // read from a weak actor table throughout compilation.
                        stack.push(function).ok()?;
                        jobs.push(Job::Callback {
                            slot,
                            program: updates.len(),
                        });
                        updates.push((function, *indices.get(&actor)?));
                    }
                    _ => return None,
                }
            }
        }
    }
    let owner = super::compile_with_updates(
        stack, store, inputs, actors, states, tracked, &updates, &reserved,
    )?;
    for job in &mut jobs {
        if let Job::Callback { program, .. } = job {
            *program += owner.input_count;
        }
    }
    let deltas = vec![Delta::default(); nodes.len()].into_boxed_slice();
    Some(CompiledActorInteractions {
        owner,
        nodes: nodes.into_boxed_slice(),
        jobs: jobs.into_boxed_slice(),
        deltas,
    })
}
