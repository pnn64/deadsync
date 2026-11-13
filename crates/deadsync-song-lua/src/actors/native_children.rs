//! Named native children shared by composite actor bindings.
use super::*;
use sprites::set;

pub(super) fn install_helpers(vm: &mut Vm, actor: TableId, helpers: TableId) -> Result<(), Error> {
    let setup = compile_named(
        include_bytes!("native_children.lua"),
        b"=DeadSync native children",
    )?;
    let setup = vm.execute(&setup)?[0];
    let root = vm.pin(setup)?;
    let key = vm.try_intern(b"GetName")?;
    let get_name = vm.raw_table_get(actor, key)?;
    let group = vm.raw_table_get(helpers, Value::Number(3.0))?;
    let functions = vm.call(setup, &[group, get_name])?.to_vec();
    for (key, function) in [
        "native_registry",
        "native_named_children",
        "native_children",
        "native_is_group",
        "native_remove_children",
    ]
    .into_iter()
    .zip(functions)
    {
        super::super::put(vm, helpers, key, function)?;
    }
    vm.unpin(root)?;
    Ok(())
}

pub(super) fn helper_function(call: &mut HostCall<'_>, name: &[u8]) -> Result<Value, Error> {
    let helpers = table(call, call.captures()[1])?;
    get(call, helpers, name)
}

pub(super) fn named_children(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    let function = helper_function(call, b"native_named_children")?;
    Ok(call.call(function, &[Value::Table(actor)])?[0])
}

/// One entry of the snapshot [`named_children`] builds: `named_children(actor)[name]`.
///
/// The Lua helper defines the result. The usual shape is read here without
/// building that snapshot; every other shape runs the helper.
pub(super) fn named_child(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
) -> Result<Value, Error> {
    if let Some(child) = find_named(call, actor, name)? {
        return Ok(child);
    }
    let children = named_children(call, actor)?;
    call.with_roots(&[children], |call| get(call, table(call, children)?, name))
}

// Whether the helper's `isgroup` holds for a table, or `None` when a
// `__metatable` field or a metatable of the metatable takes part in its answer.
fn is_group(call: &mut HostCall<'_>, value: TableId) -> Result<Option<bool>, Error> {
    let Some(metatable) = call.raw_metatable(value)? else {
        return Ok(Some(false));
    };
    if get(call, metatable, b"__metatable")? != Value::Nil {
        return Ok(None);
    }
    Ok(match get(call, metatable, b"__songlua_child_group")? {
        Value::Nil if call.raw_metatable(metatable)?.is_some() => None,
        Value::Nil | Value::Boolean(false) => Some(false),
        _ => Some(true),
    })
}

// The helper's result for one name, where it is a value the registry or the
// child array already holds. `None` leaves the helper to run: for a registry
// it would create, a name it would export as a new group, every error it would
// raise, and children whose `GetName` is more than the stored name.
pub(super) fn find_named(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
) -> Result<Option<Value>, Error> {
    let wanted = call.try_intern(name)?;
    let mut found = Value::Nil;
    if matches!(
        get(call, actor, b"__songlua_owned_child_lookup")?,
        Value::Nil | Value::Boolean(false)
    ) {
        let Value::Table(registry) = get(call, actor, b"__songlua_children")? else {
            return Ok(None);
        };
        if let Some(metatable) = call.raw_metatable(registry)? {
            if get(call, metatable, b"__metatable")? != Value::Nil {
                return Ok(None);
            }
            // The creation order only orders the snapshot; it must be a table.
            match get(call, metatable, b"__songlua_child_names")? {
                Value::Table(_) => {}
                Value::Nil if call.raw_metatable(metatable)?.is_none() => {}
                _ => return Ok(None),
            }
        }
        let mut key = Value::Nil;
        while let Some((next, value)) = call.raw_table_next(registry, key)? {
            key = next;
            let Value::Table(entry) = value else {
                if key == wanted {
                    found = value;
                }
                continue;
            };
            let Some(group) = is_group(call, entry)? else {
                return Ok(None);
            };
            if !group {
                if key == wanted {
                    found = value;
                }
                continue;
            }
            // Every group is checked, as the snapshot copies all of them. A
            // group exports its member when all of its members are that one.
            let mut only = Value::Nil;
            for index in 1.. {
                match call.raw_table_get(entry, Value::Number(index as f64))? {
                    Value::Nil => break,
                    member @ Value::Table(_) if only == Value::Nil || only == member => {
                        only = member;
                    }
                    Value::Table(_) if key != wanted => {}
                    _ => return Ok(None),
                }
            }
            if key == wanted {
                if only == Value::Nil {
                    return Ok(None);
                }
                found = only;
            }
        }
    }
    let mut swept = false;
    for index in 1.. {
        let child = match call.raw_table_get(actor, Value::Number(index as f64))? {
            Value::Nil => break,
            Value::Table(child) => child,
            _ => continue,
        };
        // `GetName` runs for every table child and sweeps retired scenes first.
        if !swept {
            swept = true;
            if call.store_ref().collected_scenes.pending() {
                retirement::sweep(call.store());
            }
        }
        let Some(node) = call.store_ref().actors.get(&child) else {
            return Ok(None);
        };
        if node.raw_instance
            || node.pending_init
            || node.offline_instance
            || node.state.class == "OptionRow"
        {
            return Ok(None);
        }
        if node.state.name != name {
            continue;
        }
        match found {
            Value::Table(old) if old == child => {
                if is_group(call, old)? != Some(false) {
                    return Ok(None);
                }
            }
            // A second child of that name joins or builds a group.
            Value::Table(_) => return Ok(None),
            _ => found = Value::Table(child),
        }
    }
    Ok(Some(found))
}

pub(super) fn remove(call: &mut HostCall<'_>, actor: TableId, all: bool) -> Result<(), Error> {
    let name = if all {
        Value::Nil
    } else {
        let Some(name) = call.coerce_string(arg(call, 1))? else {
            return Ok(());
        };
        Value::String(name)
    };
    call.with_roots(&[name], |call| {
        let remove = helper_function(call, b"native_remove_children")?;
        let result = call
            .call(remove, &[Value::Table(actor), name, Value::Boolean(all)])
            .map(|_| ());
        if result
            .as_ref()
            .is_err_and(|error| matches!(error.kind(), ErrorKind::Limit | ErrorKind::Memory))
        {
            return result;
        }
        let roots = result
            .as_ref()
            .err()
            .and_then(Error::value)
            .into_iter()
            .collect::<Vec<_>>();
        call.with_roots(&roots, |call| {
            // Keep the native traversal consistent with partial Lua writes,
            // while retaining the removed actors and their saved references.
            let synced = (|| {
                let mut children = Vec::new();
                for index in 1..=call.raw_table_len(actor)? {
                    if let Value::Table(child) =
                        call.raw_table_get(actor, Value::Number(index as f64))?
                        && call.store().actors.contains_key(&child)
                    {
                        children.push(child);
                    }
                }
                call.store().actors.get_mut(&actor).unwrap().children = children;
                lifecycle::invalidate_plans(call)
            })();
            result.and(synced)
        })
    })
}

pub(super) fn children(call: &mut HostCall<'_>, actor: TableId) -> Result<Vec<TableId>, Error> {
    let function = helper_function(call, b"native_children")?;
    let values = call.call(function, &[Value::Table(actor)])?[0];
    call.with_roots(&[values], |call| {
        let values = table(call, values)?;
        let mut result = Vec::new();
        for i in 1..=call.raw_table_len(values)? {
            let value = call.raw_table_get(values, Value::Number(i as f64))?;
            let id = table(call, value)?;
            if !call.store().actors.contains_key(&id) {
                return Err(call.error(b"native child is not owned by this runtime"));
            }
            result.push(id);
        }
        Ok(result)
    })
}

pub(super) fn state(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<crate::SongLuaOverlayState, Error> {
    if !call.store().actors.contains_key(&actor) {
        let is_group = helper_function(call, b"native_is_group")?;
        if call.call(is_group, &[Value::Table(actor)])?[0] == Value::Boolean(true) {
            return Ok(crate::SongLuaOverlayState::default());
        }
    }
    let store = call.store_ref();
    let actor = store
        .actors
        .get(&actor)
        .ok_or_else(|| call.error(b"graph render child has been released"))?;
    let mut state = actor.state.render;
    actor.destination().write_to(&mut state);
    [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = actor.state.rotation;
    state.visible = actor.state.visible;
    state.draw_order = actor.state.draw_order;
    state.hibernating = actor.hibernate > 0.0;
    Ok(state)
}

pub(super) fn insert(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
    value: Value,
) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let registry = helper_function(call, b"native_registry")?;
        let registry = call.call(registry, &[Value::Table(actor)])?[0];
        call.with_roots(&[registry], |call| {
            set(call, table(call, registry)?, key, value)
        })
    })
}

pub(super) fn group_members(
    call: &mut HostCall<'_>,
    group: TableId,
) -> Result<Vec<TableId>, Error> {
    let check = helper_function(call, b"native_is_group")?;
    if call.call(check, &[Value::Table(group)])?[0] != Value::Boolean(true) {
        return Err(call.error(b"native child is not an actor or a generated child group"));
    }
    let mut members = Vec::new();
    for i in 1..=call.raw_table_len(group)? {
        members.push(table(
            call,
            call.raw_table_get(group, Value::Number(i as f64))?,
        )?);
    }
    Ok(members)
}

pub(super) fn adopt(call: &mut HostCall<'_>, actor: TableId, parent: TableId) -> Result<(), Error> {
    set(call, actor, "__songlua_parent", Value::Table(parent))?;
    if let Some(actor) = call.store().actors.get_mut(&actor) {
        actor.parent = Some(parent);
    }
    Ok(())
}

pub(super) fn adopt_declared(
    call: &mut HostCall<'_>,
    actor: TableId,
    parent: TableId,
) -> Result<(), Error> {
    let kind = call.try_intern(b"__songlua_actor_type")?;
    let value = call.raw_table_get(parent, kind)?;
    if value == Value::Nil {
        return Ok(());
    }
    assets::text(call, value)?;
    let key = call.try_intern(b"__songlua_parent")?;
    call.raw_table_set(actor, key, Value::Table(parent))?;
    if let Some(actor) = call.store().actors.get_mut(&actor) {
        actor.parent = Some(parent);
    }
    Ok(())
}

pub(super) fn group_lifecycle(
    call: &mut HostCall<'_>,
    group: TableId,
    startup: bool,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(group)], |call| {
        let children = group_members(call, group)?;
        // A fresh group forwards the lifecycle guard to its last child, just
        // like any other instance field. Authored children have already run.
        if children.last().is_some_and(|child| {
            call.store()
                .actors
                .get(child)
                .is_some_and(|a| if startup { a.started } else { a.initialized })
        }) {
            return Ok(());
        }
        if startup && let Some((function, _)) = command(call, group, b"On")? {
            run_actor_function(call, group, function, None)?;
        }
        for child in children {
            adopt_declared(call, child, group)?;
            if startup {
                lifecycle::startup(call, child)?;
            } else {
                initialize(call, child)?;
            }
        }
        if !startup && let Some((function, _)) = command(call, group, b"Init")? {
            run_actor_function(call, group, function, None)?;
        }
        Ok(())
    })
}

pub(super) fn with_children(
    call: &mut HostCall<'_>,
    actor: TableId,
    mut action: impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    let helper = native_children::helper_function(call, b"native_children")?;
    let children = call.call(helper, &[Value::Table(actor)])?[0];
    // Native direct-child enumeration owns its snapshot even when a callback
    // removes a plain table child from the current registry and collects.
    call.with_roots(&[children], |call| {
        let children = table(call, children)?;
        for index in 1..=call.raw_table_len(children)? {
            let value = call.raw_table_get(children, Value::Number(index as f64))?;
            action(call, table(call, value)?)?;
        }
        Ok(())
    })
}

pub(super) fn run_command(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
    params: Option<Value>,
) -> Result<(), Error> {
    run_command_with_drain(call, actor, name, params, true)
}
pub(super) fn run_command_with_drain(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
    params: Option<Value>,
    drain: bool,
) -> Result<(), Error> {
    if call.store().actors.contains_key(&actor) {
        return run_named_with_drain(call, actor, name, params, drain);
    }
    // These setters accept a table receiver, and native named registries can
    // contain plain callback tables. Preserve their guard without treating an
    // unowned table as an entry in the Rust actor store.
    let Some((function, command_field)) = command(call, actor, name)? else {
        return Ok(());
    };
    call.with_roots(
        &[Value::Table(actor), function, params.unwrap_or(Value::Nil)],
        |call| {
            let active = sprites::field(call, actor, "__songlua_active_commands")?;
            let active = if active == Value::Nil {
                let value = Value::Table(call.try_new_table()?);
                sprites::set(call, actor, "__songlua_active_commands", value)?;
                value
            } else {
                active
            };
            table(call, active)?;
            call.with_roots(&[active], |call| {
                let key = call.try_intern(&command_field)?;
                call.with_roots(&[key], |call| {
                    if !matches!(
                        call.table_get(active, key)?,
                        Value::Nil | Value::Boolean(false)
                    ) {
                        return Ok(());
                    }
                    call.table_set(active, key, Value::Boolean(true))?;
                    #[cfg(feature = "test-support")]
                    let observed_command =
                        song_messages::update_command(call.store(), &command_field);
                    let result = (|| {
                        let theme = theme_callback(call, function)?;
                        let observed = command_observed(call, actor, &command_field, function)?;
                        match run_callback(
                            call,
                            actor,
                            function,
                            params,
                            observed,
                            Some(&command_field),
                        ) {
                            Err(error) if theme && error.kind() == ErrorKind::Runtime => {
                                call.store().diagnostics.push(error.to_string());
                                Ok(())
                            }
                            result => result,
                        }
                    })();
                    #[cfg(feature = "test-support")]
                    song_messages::restore_command(call.store(), observed_command);
                    call.table_set(active, key, Value::Nil)?;
                    result
                })
            })
        },
    )
}
