//! MessageManager delivery over owned Rust actors and traced Lua parameters.
use super::*;
mod capture;
mod compilation;
pub(super) mod judgment;
mod probe;
pub(super) use capture::Deferred;

const ACTIVE: &[u8] = b"__songlua_active_broadcast";
const MANAGER_HELPER: usize = 8;

#[derive(Clone, Copy)]
pub(super) enum MessageOp {
    Broadcast,
    Logging,
    Host,
}

pub(super) fn install(vm: &mut Vm, store: &Shared, captures: &[Value]) -> Result<Root, Error> {
    let manager = vm.try_new_table()?;
    let Value::Table(helpers) = captures[1] else {
        unreachable!()
    };
    vm.raw_table_set(
        helpers,
        Value::Number(MANAGER_HELPER as f64),
        Value::Table(manager),
    )?;
    for (name, op) in [
        ("Broadcast", MessageOp::Broadcast),
        ("SetLogging", MessageOp::Logging),
    ] {
        install_method(vm, manager, name, Op::Message(op), store, captures)?;
    }
    vm.set_global(b"MESSAGEMAN", Value::Table(manager))?;
    vm.set_global(
        crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes(),
        Value::Number(0.0),
    )?;
    let callback = vm.new_host_function_with_data(
        super::callback,
        captures,
        Binding {
            store: store.clone(),
            op: Op::Message(MessageOp::Host),
        },
    )?;
    vm.pin(callback)
}

fn global(call: &mut HostCall<'_>, key: &[u8], raw: bool) -> Result<Value, Error> {
    let globals = helper(call, 5)?;
    let key = call.try_intern(key)?;
    if raw {
        call.raw_table_get(table(call, globals)?, key)
    } else {
        call.table_get(globals, key)
    }
}

pub(super) fn side_effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = helper(call, 5)?;
    let tonumber = helper(call, 6)?;
    super::super::managers::side_effect(call, globals, tonumber)
}

pub(super) fn record(
    call: &mut HostCall<'_>,
    message: &[u8],
    has_params: bool,
) -> Result<(), Error> {
    let history = global(call, crate::SONG_LUA_BROADCASTS_KEY.as_bytes(), false)?;
    if history == Value::Nil {
        return Ok(());
    }
    let history = table(call, history)?;
    call.with_roots(&[Value::Table(history)], |call| {
        let entry = call.try_new_table()?;
        let message = call.try_intern(message)?;
        call.with_roots(&[Value::Table(entry), message], |call| {
            judgment::set(call, entry, b"message", message)?;
            judgment::set(call, entry, b"has_params", Value::Boolean(has_params))?;
            let index = call.raw_table_len(history)? + 1;
            call.raw_table_set(history, Value::Number(index as f64), Value::Table(entry))
        })
    })
}

pub(super) fn callback(
    call: &mut HostCall<'_>,
    store: &Shared,
    op: MessageOp,
) -> Result<(), Error> {
    if matches!(op, MessageOp::Host) {
        let name =
            command_name(call, arg(call, 0)).ok_or_else(|| call.error(b"message name expected"))?;
        // Internal manager bindings share the native guarded ScriptError path.
        if arg(call, 4) == Value::Boolean(true) {
            let text = String::from_utf8_lossy(&name);
            return framebuffers::report(call, store, &text);
        }
        let params = (arg(call, 1) == Value::Boolean(true)).then(|| arg(call, 2));
        return dispatch(
            call,
            store,
            &name,
            params,
            arg(call, 3) == Value::Boolean(true),
        );
    }
    if matches!(op, MessageOp::Logging) {
        side_effect(call)?;
    } else {
        let offset = usize::from(matches!(arg(call, 0), Value::Table(_)));
        if let Some(name) = command_name(call, arg(call, offset)) {
            let params = call.arguments().get(offset + 1).copied();
            if let Some(value) = params
                && !matches!(value, Value::Table(_) | Value::Nil)
            {
                return Err(call.error(b"bad argument #2 to 'Broadcast' (table or nil expected)"));
            }
            side_effect(call)?;
            let has_params = matches!(params, Some(Value::Table(_)));
            record(call, &name, has_params)?;
            live_sources::capture(call, store, &name, params)?;
            let suppressed = global(
                call,
                crate::SONG_LUA_SUPPRESS_BROADCAST_KEY.as_bytes(),
                true,
            )?;
            if has_params || matches!(suppressed, Value::Nil | Value::Boolean(false)) {
                dispatch(call, store, &name, params, true)?;
            }
            #[cfg(feature = "test-support")]
            {
                let observations = store.lock().unwrap().observations.clone();
                let globals = helper(call, 5)?;
                observations.state_call(
                    call,
                    globals,
                    "MessageManager.Broadcast",
                    vec![serde_json::json!(String::from_utf8_lossy(&name))],
                )?;
            }
        }
    }
    call.push(helper(call, MANAGER_HELPER)?)
}

pub(super) fn queued(call: &mut HostCall<'_>, store: &Shared, message: &[u8]) -> Result<(), Error> {
    record(call, message, true)?;
    let params = Value::Table(call.try_new_table()?);
    call.with_roots(&[params], |call| {
        dispatch(call, store, message, Some(params), true)
    })
}

pub(super) fn dispatch(
    call: &mut HostCall<'_>,
    store: &Shared,
    message: &[u8],
    params: Option<Value>,
    normalize: bool,
) -> Result<(), Error> {
    if std::str::from_utf8(message).is_ok_and(|message| message.trim().is_empty()) {
        return Ok(());
    }
    song_capture::broadcast(call, store, message, params.is_some())?;
    let active_key = call.try_intern(ACTIVE)?;
    let globals = table(call, helper(call, 5)?)?;
    let previous = call.raw_table_get(globals, active_key)?;
    let name = call.try_intern(message)?;
    call.with_roots(
        &[active_key, previous, name, params.unwrap_or(Value::Nil)],
        |call| {
            call.raw_table_set(globals, active_key, name)?;
            let previous_message = std::mem::replace(
                &mut store.lock().unwrap().active_message,
                std::str::from_utf8(message).ok().map(str::to_owned),
            );
            let result = deliver(call, store, message, params, normalize);
            store.lock().unwrap().active_message = previous_message;
            // Raw restoration also works after a fatal instruction limit.
            call.raw_table_set(globals, active_key, previous)?;
            result
        },
    )
}

fn deliver(
    call: &mut HostCall<'_>,
    store: &Shared,
    message: &[u8],
    params: Option<Value>,
    normalize: bool,
) -> Result<(), Error> {
    let field = [message, b"MessageCommand"].concat();
    let key = call.try_intern(&field)?;
    call.with_roots(&[key], |call| {
        let order = store.lock().unwrap().actor_order.clone();
        let mut actors = Vec::new();
        for actor in order {
            let function = call.table_get(Value::Table(actor), key)?;
            if matches!(function, Value::Function(_) | Value::Native(_)) {
                let identity = call
                    .object_pointer(Value::Table(actor))
                    .expect("registry owns actor");
                actors.push((identity, actor));
            }
        }
        actors.sort_unstable_by_key(|&(identity, _)| identity);
        // Differential controls import allocator identities, just as the
        // existing production oracle does. Normal delivery uses VM identities.
        #[cfg(feature = "test-support")]
        {
            let (ranks, paths) = {
                let store = store.lock().unwrap();
                (store.message_ranks.clone(), store.message_paths.clone())
            };
            let mut known = Vec::new();
            let mut slots = Vec::new();
            for (slot, &(_, actor)) in actors.iter().enumerate() {
                let rank = if paths.is_empty() || actors.len() <= 1 {
                    ranks.get(&actor).copied()
                } else {
                    let path = live_visuals::actor_path(call, actor)?;
                    let rank = paths.get(&path).copied();
                    if rank.is_none() {
                        let gap = format!("missing captured subscriber identity for {path}");
                        let mut store = store.lock().unwrap();
                        if store.message_coverage_seen.insert(gap.clone()) {
                            store.message_coverage.push(gap);
                        }
                    }
                    rank
                };
                if let Some(rank) = rank {
                    slots.push(slot);
                    known.push((rank, actor));
                }
            }
            known.sort_unstable_by_key(|&(rank, _)| rank);
            for (slot, actor) in slots.into_iter().zip(known) {
                actors[slot] = actor;
            }
        }
        if normalize
            && message.eq_ignore_ascii_case(b"Judgment")
            && let Some(Value::Table(params)) = params
        {
            judgment::normalize(call, params)?;
        }
        let name = [message, b"Message"].concat();
        for (_, actor) in actors {
            // A broadcast is synchronous even when the same handler is already
            // active. Direct playcommand keeps its separate recursion guard.
            let was_active = store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .active_commands
                .remove(&field);
            let result = run_named(call, store, actor, &name, params);
            if was_active {
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .active_commands
                    .insert(field.clone());
            }
            result?;
            let mut guard = store.lock().unwrap();
            if guard.replay_clock.is_some() {
                for entry in &mut guard.deferred_messages {
                    if entry.actor == actor && entry.command.as_bytes() == field {
                        entry.observed = true;
                    }
                }
            }
        }
        Ok(())
    })
}
