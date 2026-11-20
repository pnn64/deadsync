//! Wrapper actors are owned with their scene, but are not ordinary children.
use super::*;

fn text_value(call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
    let bytes = bytes(call, value)?;
    if std::str::from_utf8(&bytes).is_err() {
        return Err(call.error(b"actor directory or tag must be valid UTF-8"));
    }
    call.try_intern(&bytes)
}

fn copy_tags(call: &mut HostCall<'_>, owner: TableId, wrapper: TableId) -> Result<(), Error> {
    for key in ["__songlua_player1_enabled", "__songlua_player2_enabled"] {
        let key = call.try_intern(key.as_bytes())?;
        let value = call.raw_table_get(owner, key)?;
        if value != Value::Nil {
            call.raw_table_set(wrapper, key, Value::Boolean(value != Value::Boolean(false)))?;
        }
    }
    for key in [
        "__songlua_player_index",
        "__songlua_compile_layer",
        "__songlua_player_child_name",
        "__songlua_top_screen_child_name",
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_style_name",
    ] {
        let field = call.try_intern(key.as_bytes())?;
        let value = call.table_get(Value::Table(owner), field)?;
        if value == Value::Nil {
            continue;
        }
        let value = if matches!(key, "__songlua_player_index" | "__songlua_compile_layer") {
            let value = call.call(helper(call, 6)?, &[value])?[0];
            let Value::Number(value) = value else {
                return Err(call.error(b"integer tag expected"));
            };
            if !(-9223372036854775808.0..9223372036854775808.0).contains(&value) {
                return Err(call.error(b"integer tag out of range"));
            }
            Value::Number((value as i64) as f64)
        } else {
            text_value(call, value)?
        };
        // Lookup/coercion may have collected the old temporary field string.
        let field = call.try_intern(key.as_bytes())?;
        call.raw_table_set(wrapper, field, value)?;
    }
    Ok(())
}

fn remove(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    // LunaActor uses Lua 5.1's luaL_checkint: numeric strings are accepted and
    // finite fractions truncate before the native one-based index check.
    let number = call.number(1)?;
    let index = (number as i64) as i32;
    let offset = index.wrapping_sub(1);
    let slot = usize::try_from(offset).ok().filter(|&slot| {
        number.is_finite() && slot < call.store_ref().actors[&owner].wrappers.len()
    });
    let Some(slot) = slot else {
        return Err(call.error(format!("{index} is not a valid wrapper state index.").as_bytes()));
    };
    action_capture::prepare(call, owner)?;
    let wrapper = call.store().actors.get_mut(&owner).unwrap().wrappers.remove(slot);
    let mut removed = HashSet::new();
    let mut pending = vec![wrapper];
    while let Some(actor) = pending.pop() {
        if removed.insert(actor)
            && let Some(instance) = call.store_ref().actors.get(&actor)
        {
            // Actor::~Actor deletes its wrappers. A plain ActorFrame wrapper
            // does not own its ordinary children (m_bDeleteChildren is false).
            pending.extend_from_slice(&instance.wrappers);
        }
    }
    lifecycle::invalidate_plans(call)?;
    let registry = table(call, call.captures()[0])?;
    for &actor in &removed {
        call.raw_table_set(registry, Value::Table(actor), Value::Nil)?;
    }
    let store = call.store();
    store.actor_order.retain(|actor| !removed.contains(actor));
    for scene in store.scenes.values_mut() {
        scene.actors.retain(|actor| !removed.contains(actor));
    }
    if let Some(queued) = &mut store.startup_queues {
        queued.retain(|actor| !removed.contains(actor));
    }
    store.deferred_messages.retain(|entry| !removed.contains(&entry.actor));
    for actor in removed {
        // Removing metadata stops the queue and invalidates bound methods on a
        // retained Lua table; ordinary raw fields on that table remain readable.
        store.actors.remove(&actor);
        store.retired_subscriptions.remove(&actor);
        #[cfg(feature = "test-support")]
        store.message_ranks.remove(&actor);
    }
    store.wrapper_epoch = store.wrapper_epoch.wrapping_add(1);
    call.push(Value::Table(owner))
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    match name {
        "RemoveWrapperState" => remove(call, actor),
        "GetNumWrapperStates" => call.push(Value::Number(
            call.store_ref().actors[&actor].wrappers.len() as f64,
        )),
        "GetWrapperState" => {
            // Production accepts numeric values, truncating fractions; numeric
            // strings are not indices. Wrapper indices start at one.
            let value = if let Value::Number(index) = arg(call, 1) {
                let index = index as i64;
                usize::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| call.store().actors[&actor].wrappers.get(index).copied())
                    .map_or(Value::Nil, Value::Table)
            } else {
                Value::Nil
            };
            call.push(value)
        }
        "AddWrapperState" => {
            action_capture::prepare(call, actor)?;
            let scene = call.store().actors[&actor].scene;
            let attrs = call.try_new_table()?;
            let key = call.intern_static(b"Class")?;
            let kind = call.intern_static(b"WrapperState")?;
            call.raw_table_set(attrs, key, kind)?;
            let wrapper = call.with_roots(&[Value::Table(attrs)], |call| {
                // Native dummy actors inherit the current scope, which may
                // differ from their owner's original declaration directory.
                for key in ["__songlua_script_dir", "__songlua_song_dir"] {
                    let field = call.try_intern(key.as_bytes())?;
                    let value = call.table_get(helper(call, 5)?, field)?;
                    if value != Value::Nil {
                        let value = text_value(call, value)?;
                        let field = call.try_intern(key.as_bytes())?;
                        call.raw_table_set(attrs, field, value)?;
                    }
                }
                let node = call.try_new_table()?;
                call.raw_table_set(node, Value::Number(1.0), Value::Table(attrs))?;
                let empty = call.try_new_table()?;
                call.raw_table_set(node, Value::Number(2.0), Value::Table(empty))?;
                call.with_roots(&[Value::Table(node)], |call| {
                    Ok(build(call, scene, node, None, true, false)?.expect("wrapper class exists"))
                })
            })?;
            courses::share_child(call, actor, wrapper)?;
            // The registry already roots the new actor before tag lookups can
            // reenter Lua, detach fields or collect. No store borrow crosses Lua.
            copy_tags(call, actor, wrapper)?;
            {
                let store = call.store();
                // A tag failure leaves the registered actor unattached, matching
                // the production constructor's partial state and cleanup lifetime.
                store.actors.get_mut(&wrapper).unwrap().parent = Some(actor);
                store.actors.get_mut(&actor).unwrap().wrappers.push(wrapper);
                store.wrapper_epoch = store.wrapper_epoch.wrapping_add(1);
            }
            chronological::created(call, actor, wrapper)?;
            lifecycle::invalidate_plans(call)?;
            call.push(Value::Table(wrapper))
        }
        _ => unreachable!(),
    }
}
