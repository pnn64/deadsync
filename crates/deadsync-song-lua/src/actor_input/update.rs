//! Native input/update callback units using the loading scene's update order.
use super::*;
use mlua::{Function, Value};

pub(crate) enum SourceJob {
    Enter {
        actor: Table,
        parent: Option<usize>,
        wrappers: bool,
    },
    Callback {
        actor: Table,
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
pub struct SongLuaCompiledActorInteractions {
    owner: SongLuaCompiledActorInputs,
    nodes: Box<[Node]>,
    jobs: Box<[Job]>,
    deltas: Box<[Delta]>,
}

impl SongLuaCompiledActorInteractions {
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
    pub fn scalar_states(&self) -> impl Iterator<Item = (usize, &[u8], SongLuaInputScalar)> + '_ {
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
        self.deltas.fill(Delta::default());
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
        std::mem::size_of::<Self>() - std::mem::size_of::<SongLuaCompiledActorInputs>()
            + self.owner.retained_bytes()
            + std::mem::size_of_val(self.nodes.as_ref())
            + std::mem::size_of_val(self.jobs.as_ref())
            + std::mem::size_of_val(self.deltas.as_ref())
    }
}

pub(crate) fn compile(
    lua: &Lua,
    root: &Value,
    actors: &[Table],
    states: &[SongLuaOverlayState],
    tracked: &[(SongLuaTrackedActorTarget, Table, SongLuaOverlayState)],
) -> Option<SongLuaCompiledActorInteractions> {
    if !crate::lua_util::can_compile_actor_input(lua) {
        return None;
    }
    let source = crate::lua_util::callback_update_jobs(lua, root)?;
    let indices = actors
        .iter()
        .chain(tracked.iter().map(|(_, actor, _)| actor))
        .enumerate()
        .map(|(index, actor)| (actor.to_pointer() as usize, index))
        .collect::<FxHashMap<_, _>>();
    let mut nodes = Vec::new();
    let mut jobs = Vec::new();
    let mut updates = Vec::<(Function, usize)>::new();
    for job in &source {
        match job {
            SourceJob::Enter {
                actor,
                parent,
                wrappers,
            } => {
                // The loader's synthetic root has no renderable actor state.
                let index = indices.get(&(actor.to_pointer() as usize)).copied();
                // Scheduling/foreground overrides require their own runtime
                // owners rather than a frozen interpretation of those fields.
                for key in ["__songlua_live_delta", "__songlua_recurring_update_command"] {
                    if !matches!(
                        actor.raw_get::<InspectionValue>(key).ok()?,
                        InspectionValue::Nil
                    ) {
                        return None;
                    }
                }
                if actor
                    .raw_get::<Option<bool>>("__songlua_update_pending")
                    .ok()?
                    .unwrap_or(false)
                    || actor
                        .raw_get::<Option<bool>>("__songlua_live_inactive")
                        .ok()?
                        .unwrap_or(false)
                {
                    return None;
                }
                jobs.push(Job::Enter(nodes.len()));
                nodes.push(Node {
                    actor: index,
                    parent: *parent,
                    wrappers: *wrappers,
                    rate: actor
                        .raw_get::<Option<f64>>("__songlua_update_rate")
                        .ok()?
                        .unwrap_or(1.0) as f32,
                    sleeping: actor
                        .raw_get::<Option<f32>>("__songlua_hibernate_seconds")
                        .ok()?
                        .unwrap_or(0.0),
                    paused: actor
                        .raw_get::<Option<bool>>("__songlua_theme_hibernating")
                        .ok()?
                        .unwrap_or(false),
                });
            }
            SourceJob::Callback { actor, slot } => {
                match actor
                    .raw_get::<InspectionValue>("__songlua_update_function")
                    .ok()?
                {
                    InspectionValue::Nil => {}
                    InspectionValue::Function(function) => {
                        jobs.push(Job::Callback {
                            slot: *slot,
                            program: updates.len(),
                        });
                        updates.push((function, *indices.get(&(actor.to_pointer() as usize))?));
                    }
                    _ => return None,
                }
            }
        }
    }
    let owner = super::compile_with_updates(lua, actors, states, tracked, &updates, &source)?;
    for job in &mut jobs {
        if let Job::Callback { program, .. } = job {
            *program += owner.input_count;
        }
    }
    let deltas = vec![Delta::default(); nodes.len()].into_boxed_slice();
    Some(SongLuaCompiledActorInteractions {
        owner,
        nodes: nodes.into_boxed_slice(),
        jobs: jobs.into_boxed_slice(),
        deltas,
    })
}
