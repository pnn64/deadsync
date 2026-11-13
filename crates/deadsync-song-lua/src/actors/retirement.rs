//! Transfer an outgoing scene from process roots to Lua-owned reachability.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Collected scene notifications are rare compared with actor method calls.
/// Publish an atomic pending bit while holding the queue lock so the common
/// path can skip both mutexes without missing a concurrent finalizer.
pub(super) type Collected = Arc<CollectedQueue>;

#[derive(Default)]
pub(super) struct CollectedQueue {
    pending: AtomicBool,
    records: Mutex<Vec<(u64, TableId)>>,
}
impl CollectedQueue {
    #[inline]
    pub(super) fn pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    fn enqueue(&self, scene: u64, owner: TableId) {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        records.push((scene, owner));
        self.pending.store(true, Ordering::Release);
    }

    fn take(&self) -> Vec<(u64, TableId)> {
        if !self.pending() {
            return Vec::new();
        }
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let collected = std::mem::take(&mut *records);
        // Keep the reset under the lock: a concurrent Drop can only set true
        // after this point, once it has enqueued its own notification.
        self.pending.store(false, Ordering::Release);
        collected
    }
}
struct CollectedScene {
    queue: Collected,
    id: u64,
    owner: TableId,
}
impl Drop for CollectedScene {
    fn drop(&mut self) {
        // GC/shutdown only notifies. Never touch the store or reenter the VM from
        // a closure destructor, including collection inside an actor callback.
        self.queue.enqueue(self.id, self.owner);
    }
}
fn lifetime(_: &mut HostCall<'_>) -> Result<(), Error> {
    Ok(())
}

pub(super) fn sweep(store: &mut Store) {
    let queue = store.collected_scenes.clone();
    let collected = queue.take();
    if collected.is_empty() {
        return;
    }
    let guard = &mut *store;
    for (id, owner) in collected {
        // An attachment may have failed before publication, then been retried.
        if guard.retired_scenes.get(&id) != Some(&owner) {
            continue;
        }
        guard.retired_scenes.remove(&id);
        let Some(scene) = guard.scenes.remove(&id) else {
            continue;
        };
        for actor in scene.actors {
            let other = guard
                .scenes
                .iter()
                .find_map(|(id, s)| s.actors.contains(&actor).then_some(*id));
            if let Some(other) = other {
                if let Some(a) = guard.actors.get_mut(&actor)
                    && a.scene == id
                {
                    a.scene = other;
                }
            } else {
                guard.actors.remove(&actor);
                guard.retired_subscriptions.remove(&actor);
                guard.actor_order.retain(|candidate| *candidate != actor);
                if let Some(queued) = &mut guard.startup_queues {
                    queued.retain(|candidate| *candidate != actor);
                }
                #[cfg(feature = "test-support")]
                guard.message_ranks.remove(&actor);
            }
        }
    }
}

/// Late wrappers/native children retain the old graph and join the current
/// native registry. Its next replacement relinquishes those additional roots.
pub(super) fn retain(
    call: &mut HostCall<'_>,
    scene: u64,
    actor: TableId,
    registry: TableId,
) -> Result<(), Error> {
    let owner = call.store().retired_scenes.get(&scene).copied();
    if let Some(owner) = owner {
        call.raw_table_set(owner, Value::Table(actor), Value::Boolean(true))?;
        if let Some(previous) = call.table_host_state(actor)?
            && previous != owner
        {
            call.raw_table_set(owner, Value::Table(previous), Value::Boolean(true))?;
        }
        call.set_table_host_state(actor, Some(owner))?;
        call.raw_table_set(registry, Value::Table(actor), Value::Boolean(true))?;
        call.store().retired_subscriptions.insert(actor);
    } else {
        call.raw_table_set(registry, Value::Table(actor), Value::Boolean(true))?;
    }
    if !call.store().actors.contains_key(&actor) {
        scalar_methods::bind(call, actor)?;
    }
    Ok(())
}

pub(super) fn retire(call: &mut HostCall<'_>, id: u64) -> Result<bool, Error> {
    let (members, sounds_retired, queue) = {
        let guard = call.store();
        if guard.retired_scenes.contains_key(&id) {
            return Ok(false);
        }
        let Some(scene) = guard.scenes.get(&id) else {
            return Ok(false);
        };
        (
            scene.actors.clone(),
            scene.sounds_retired,
            guard.collected_scenes.clone(),
        )
    };
    lifecycle::invalidate_plans(call)?;
    let owner = call.try_new_table()?;
    call.with_roots(&[Value::Table(owner)], |call| {
        let token =
            call.new_host_function_with_data(lifetime, &[], CollectedScene { queue, id, owner })?;
        call.raw_table_set(owner, Value::Number(1.), token)?;
        let (exclusive, references) = {
            let guard = call.store();
            let exclusive: Vec<_> = members
                .iter()
                .copied()
                .filter(|actor| {
                    !guard.scenes.iter().any(|(other, s)| {
                        *other != id
                            && !guard.retired_scenes.contains_key(other)
                            && s.actors.contains(actor)
                    })
                })
                .collect();
            let mut references = members.clone();
            for actor in &exclusive {
                let a = &guard.actors[actor];
                references.extend(a.parent);
                references.extend(a.children.iter().copied());
                references.extend(a.wrappers.iter().copied());
                references.extend(a.sound);
            }
            (exclusive, references)
        };
        for value in references {
            call.raw_table_set(owner, Value::Table(value), Value::Boolean(true))?;
        }
        // Native methods are part of the retained actor, even when an authored
        // weak metatable removes the instance's ordinary strong value edges.
        for &actor in &exclusive {
            if let Value::Table(bound) = get(call, actor, b"__songlua_bound_methods")? {
                call.raw_table_set(owner, Value::Table(bound), Value::Boolean(true))?;
            }
        }
        for &actor in &exclusive {
            if let Some(previous) = call.table_host_state(actor)? {
                call.raw_table_set(owner, Value::Table(previous), Value::Boolean(true))?;
            }
            call.set_table_host_state(actor, Some(owner))?;
        }
        // Publish only after every graph edge is traced. Registry removal below
        // cannot collect a partially transferred graph at a callback boundary.
        {
            let guard = call.store();
            guard.retired_scenes.insert(id, owner);
            guard.constructor_scenes.remove(&id);
            guard.actor_order.retain(|actor| !exclusive.contains(actor));
            if let Some(queued) = &mut guard.startup_queues {
                queued.retain(|actor| !exclusive.contains(actor));
            }
            for &actor in &members {
                let other = guard.scenes.iter().find_map(|(other, s)| {
                    (*other != id
                        && !guard.retired_scenes.contains_key(other)
                        && s.actors.contains(&actor))
                    .then_some(*other)
                });
                if let Some(other) = other
                    && guard.actors[&actor].scene == id
                {
                    guard.actors.get_mut(&actor).unwrap().scene = other;
                }
            }
        }
        let registry = table(call, call.captures()[0])?;
        for actor in exclusive {
            if !sounds_retired {
                sound::retire(call, actor);
            }
            call.raw_table_set(registry, Value::Table(actor), Value::Nil)?;
            call.store().retired_subscriptions.remove(&actor);
        }
        Ok(true)
    })
}
