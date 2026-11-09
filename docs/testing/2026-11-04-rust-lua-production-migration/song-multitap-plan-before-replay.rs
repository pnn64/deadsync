//! Compiler observations for the existing multitap preparation branch.
use super::*;

#[derive(Clone, Copy)]
enum Job {
    Advance(TableId),
    Recurring,
    Callback(TableId),
}
pub(super) struct Plan {
    pub(super) roots: TableId,
    jobs: Vec<Job>,
}

impl Actors {
    pub(in crate::rust_engine) fn install_compiler_materials(
        &self,
        vm: &mut Vm,
        materials: Vec<(TableId, Vec<Option<crate::model_texture::Clock>>)>,
    ) -> Result<bool, String> {
        let active = !materials.is_empty();
        {
            let mut store = self.store.lock().unwrap();
            if materials
                .iter()
                .any(|(id, _)| !store.actors.contains_key(id))
            {
                return Err(
                    "model material owner has been released or belongs to another runtime".into(),
                );
            }
            for actor in store.actors.values_mut() {
                actor.model_materials.clear();
            }
            for (id, clocks) in materials {
                store.actors.get_mut(&id).unwrap().model_materials = clocks;
            }
        }
        let function = vm
            .new_host_function_with_data(
                |call| {
                    let store = call.function_data::<Shared>().unwrap().clone();
                    lifecycle::invalidate_plans(call, &store)
                },
                &[Value::Table(self.registry), Value::Table(self.helpers)],
                self.store.clone(),
            )
            .map_err(|e| e.to_string())?;
        vm.call(function, &[]).map_err(|e| e.to_string())?;
        Ok(active)
    }

    pub(in crate::rust_engine) fn multitap_update_only(
        &self,
        vm: &mut Vm,
        root: Value,
    ) -> Result<bool, Error> {
        let function = vm.new_host_function_with_data(
            update_only,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                root,
            ],
            self.store.clone(),
        )?;
        Ok(vm.call(function, &[])?[0] == Value::Boolean(true))
    }

    pub(in crate::rust_engine) fn multitap_dummy(&self, vm: &mut Vm) -> Result<Root, Error> {
        let function = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = offline::dummy(call, &store, "Model")?;
                call.push(Value::Table(actor))
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        let value = vm.call(function, &[])?[0];
        vm.pin(value)
    }
}

fn raw(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(actor, key)
}
fn function(call: &HostCall<'_>, value: Value) -> Result<bool, Error> {
    match value {
        Value::Nil => Ok(false),
        Value::Function(_) | Value::Native(_) => Ok(true),
        _ => Err(call.error(b"function expected")),
    }
}
fn retain(call: &mut HostCall<'_>, plan: &Plan, actor: TableId) -> Result<(), Error> {
    call.raw_table_set(plan.roots, Value::Table(actor), Value::Boolean(true))
}
fn sequence(
    call: &mut HostCall<'_>,
    value: Value,
    mut visit: impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let array = table(call, value)?;
        for i in 1usize.. {
            let value = call.raw_table_get(array, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            let actor = table(call, value)?;
            call.with_roots(&[value], |call| visit(call, actor))?;
        }
        Ok(())
    })
}
fn collect_self(
    call: &mut HostCall<'_>,
    store: &Shared,
    plan: &mut Plan,
    actor: TableId,
) -> Result<(), Error> {
    retain(call, plan, actor)?;
    let wrappers = sprites::field(call, actor, "__songlua_wrappers")?;
    if wrappers != Value::Nil {
        sequence(call, wrappers, |call, wrapper| {
            collect(call, store, plan, wrapper)
        })?;
    } else {
        // Owned wrappers need no exported Lua array unless the author replaces it.
        let wrappers = store
            .lock()
            .unwrap()
            .actors
            .get(&actor)
            .map(|a| a.wrappers.clone())
            .unwrap_or_default();
        for wrapper in wrappers {
            collect(call, store, plan, wrapper)?;
        }
    }
    let recurring = sprites::field(call, actor, "__songlua_recurring_update_command")?;
    let recurring = initial_state::value_text(call, recurring)?.is_some();
    plan.jobs.push(Job::Advance(actor));
    if recurring {
        plan.jobs.push(Job::Recurring);
    }
    let transition = raw(call, actor, "__songlua_transition_state")?;
    if transition != Value::Nil {
        let read = call.new_host_function(
            |call| {
                let value = call.captures()[0];
                let mut stack = call.native_stack()?;
                stack.push(value)?;
                let integer = stack
                    .to_integer(-1)
                    .ok_or_else(|| stack.error(b"integer expected"))?;
                if u8::try_from(integer).is_err() {
                    return Err(stack.error(b"transition state out of range"));
                }
                stack.finish(0)
            },
            &[transition],
        )?;
        call.call(read, &[])?;
    }
    Ok(())
}
fn collect(
    call: &mut HostCall<'_>,
    store: &Shared,
    plan: &mut Plan,
    actor: TableId,
) -> Result<(), Error> {
    collect_self(call, store, plan, actor)?;
    for i in 1usize.. {
        let child = call.raw_table_get(actor, Value::Number(i as f64))?;
        if child == Value::Nil {
            break;
        }
        if let Value::Table(child) = child {
            call.with_roots(&[Value::Table(child)], |call| {
                native_children::adopt_declared(call, store, child, actor)?;
                collect(call, store, plan, child)
            })?;
        }
    }
    if store.lock().unwrap().actors.contains_key(&actor)
        && let Some(stream) = song_meters::stream(call, store, actor)?
    {
        collect(call, store, plan, stream)?;
    }
    plan.jobs.push(Job::Callback(actor));
    let columns = raw(call, actor, "__songlua_note_columns")?;
    if columns != Value::Nil {
        sequence(call, columns, |call, child| {
            collect(call, store, plan, child)
        })?;
    }
    Ok(())
}
fn build(call: &mut HostCall<'_>, store: &Shared, root: TableId) -> Result<Arc<Plan>, Error> {
    if let Some(plan) = store.lock().unwrap().multitap_plan.clone() {
        return Ok(plan);
    }
    let roots = call.try_new_table()?;
    call.with_roots(&[Value::Table(roots)], |call| {
        let mut plan = Plan {
            roots,
            jobs: Vec::new(),
        };
        let globals = table(call, helper(call, 5)?)?;
        let top = raw(call, globals, "__songlua_top_screen")?;
        let mut after = Vec::new();
        if top != Value::Nil {
            let screen = table(call, top)?;
            collect_self(call, store, &mut plan, screen)?;
            let children = native_children::named_children(call, screen)?;
            call.with_roots(&[children], |call| {
                let children = table(call, children)?;
                for name in ["Underlay", "Overlay", "Debug"] {
                    let actor = raw(call, children, name)?;
                    if actor != Value::Nil {
                        let actor = table(call, actor)?;
                        retain(call, &plan, actor)?;
                        collect(call, store, &mut plan, actor)?;
                    }
                }
                for name in ["In", "Out", "Cancel"] {
                    let actor = raw(call, children, name)?;
                    if actor != Value::Nil {
                        let actor = table(call, actor)?;
                        retain(call, &plan, actor)?;
                        after.push(actor);
                    }
                }
                Ok(())
            })?;
            sequence(call, top, |call, actor| {
                let role = raw(call, actor, "__songlua_screen_actor_role")?;
                if initial_state::value_text(call, role)?.is_some() {
                    collect(call, store, &mut plan, actor)?;
                }
                Ok(())
            })?;
        }
        for key in [
            "__songlua_top_screen_player_1",
            "__songlua_top_screen_player_2",
        ] {
            let player = raw(call, globals, key)?;
            if player != Value::Nil {
                let player = table(call, player)?;
                retain(call, &plan, player)?;
                collect(call, store, &mut plan, player)?;
            }
        }
        collect(call, store, &mut plan, root)?;
        for actor in after {
            collect(call, store, &mut plan, actor)?;
        }
        if top != Value::Nil {
            plan.jobs.push(Job::Callback(table(call, top)?));
        }
        let registry = table(call, call.captures()[0])?;
        call.raw_table_set(registry, Value::Table(roots), Value::Boolean(true))?;
        let plan = Arc::new(plan);
        store.lock().unwrap().multitap_plan = Some(plan.clone());
        Ok(plan)
    })
}
fn update_only(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let Value::Table(root) = call.captures()[2] else {
        return call.push(Value::Boolean(false));
    };
    let plan = build(call, &store, root)?;
    let eligible = call.with_roots(&[Value::Table(plan.roots)], |call| {
        // Preserve the native two passes, including errors after extra callbacks.
        for job in &plan.jobs {
            if let Job::Advance(actor) = *job {
                let sleep = raw(call, actor, "__songlua_hibernate_seconds")?;
                if sleep != Value::Nil && sprites::number(call, sleep)? as f32 > 0. {
                    return Ok(false);
                }
                let deferred = raw(call, actor, "__songlua_command_queue_deferred")?;
                if !matches!(deferred, Value::Nil | Value::Boolean(false)) {
                    return Ok(false);
                }
            }
        }
        let mut active = Vec::new();
        for job in &plan.jobs {
            match *job {
                Job::Advance(_) => (),
                Job::Recurring => active.push(None),
                Job::Callback(actor) => {
                    let current = sprites::field(call, actor, "__songlua_update_function")?;
                    if function(call, current)? {
                        active.push(Some(actor));
                    } else {
                        let previous = sprites::field(call, actor, "__songlua_update_previous")?;
                        if function(call, previous)? {
                            active.push(Some(actor));
                        }
                    }
                }
            }
        }
        match active.as_slice() {
            [] => Ok(true),
            [Some(actor)] => {
                let name = sprites::field(call, *actor, "Name")?;
                Ok(initial_state::value_text(call, name)?.as_deref() == Some("Update"))
            }
            _ => Ok(false),
        }
    })?;
    call.push(Value::Boolean(eligible))
}
