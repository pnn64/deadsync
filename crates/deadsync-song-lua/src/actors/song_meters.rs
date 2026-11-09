//! SongMeterDisplay's stream construction, width and owned render state.
use super::*;
use sprites::{field, set};

#[derive(Clone, Debug)]
pub struct SongMeterDisplay {
    pub stream_width: f32,
    pub stream_state: crate::SongLuaOverlayState,
}

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let methods = native_children::helper_function(call, b"meter_methods")?;
    let methods = table(call, methods)?;
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    for name in ["Load", "SetStreamWidth", "GetStreamWidth"] {
        let method = get(call, methods, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(invoke_bound, &captures, name)?;
        set(call, bound, name, function)?;
    }
    Ok(())
}

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    actor: TableId,
) -> Result<(), Error> {
    let width = field(call, actor, "StreamWidth")?;
    let width = float_scalar(call, width, false).unwrap_or(0.0);
    set(
        call,
        actor,
        "__songlua_stream_width",
        Value::Number(width as f64),
    )?;
    let stream = field(call, actor, "Stream")?;
    if stream == Value::Nil {
        return Ok(());
    }
    let stream = table(call, stream)?;
    if store.lock().unwrap().actors[&actor].offline_instance {
        courses::attach(call, store, scene, stream)?;
        let name = call.try_intern(b"Stream")?;
        set(call, stream, "Name", name)?;
        native_children::adopt(call, store, stream, actor)?;
        return native_children::insert(call, actor, "Stream", Value::Table(stream));
    }
    let node = call.call(helper(call, 2)?, &[Value::Table(stream)])?[0];
    let first = store.lock().unwrap().scenes[&scene].actors.len();
    let stream = call.with_roots(&[node], |call| {
        build(
            call,
            store,
            scene,
            table(call, node)?,
            Some(actor),
            false,
            false,
        )?
        .ok_or_else(|| call.error(b"SongMeterDisplay Stream must load an actor"))
    })?;
    {
        let mut store = store.lock().unwrap();
        // Unlike GraphDisplay's dummy children, this declaration subscribes
        // when its own Init traversal begins, after its meter's Init command.
        let deferred: HashSet<_> = store.scenes[&scene].actors[first..]
            .iter()
            .copied()
            .filter(|id| {
                !matches!(
                    store.actors[id].state.class,
                    "GraphDisplayBody" | "GraphDisplayLine"
                )
            })
            .collect();
        store.actor_order.retain(|id| !deferred.contains(id));
        for id in deferred {
            store.actors.get_mut(&id).unwrap().registered = false;
        }
        let stream = store.actors.get_mut(&stream).unwrap();
        stream.load_name = Some(b"Stream".to_vec());
        stream.state.name = b"Stream".to_vec();
    }
    let name = call.try_intern(b"Stream")?;
    set(call, stream, "Name", name)?;
    native_children::adopt(call, store, stream, actor)?;
    set(call, actor, "Stream", Value::Table(stream))?;
    native_children::insert(call, actor, "Stream", Value::Table(stream))
}

pub(super) fn stream(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<Option<TableId>, Error> {
    if store.lock().unwrap().actors[&actor].state.class != "SongMeterDisplay" {
        return Ok(None);
    }
    if !assets::field(call, actor, b"__songlua_actor_type")?
        .is_some_and(|s| s.eq_ignore_ascii_case("SongMeterDisplay"))
    {
        return Ok(None);
    }
    let map = native_children::named_children(call, actor)?;
    call.with_roots(&[map], |call| {
        let stream = field(call, table(call, map)?, "Stream")?;
        if stream == Value::Nil {
            return Ok(None);
        }
        let stream = table(call, stream)?;
        if !store.lock().unwrap().actors.contains_key(&stream) {
            native_children::group_members(call, stream)?;
        }
        native_children::adopt(call, store, stream, actor)?;
        Ok(Some(stream))
    })
}

pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    match name {
        "Load" => return sprites::method(call, store, actor, name),
        "GetStreamWidth" => {
            let value = field(call, actor, "__songlua_stream_width")?;
            let width = if value == Value::Nil {
                0.0
            } else {
                sprites::number(call, value)? as f32
            };
            return call.push(Value::Number(width as f64));
        }
        _ => {
            if assets::field(call, actor, b"__songlua_actor_type")?
                .is_some_and(|s| s.eq_ignore_ascii_case("SongMeterDisplay"))
                && let Some(width) = float_scalar(call, arg(call, 1), false)
            {
                set(call, actor, "StreamWidth", Value::Number(width as f64))?;
                set(
                    call,
                    actor,
                    "__songlua_stream_width",
                    Value::Number(width as f64),
                )?;
            }
        }
    }
    call.push(Value::Table(actor))
}

pub(super) fn read(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<Option<SongMeterDisplay>, Error> {
    let width = field(call, actor, "__songlua_stream_width")?;
    let width = if width == Value::Nil {
        match field(call, actor, "StreamWidth") {
            Ok(width) => float_scalar(call, width, false).unwrap_or(0.0),
            Err(error) if error.kind() == ErrorKind::Runtime => 0.0,
            Err(error) => return Err(error),
        }
    } else {
        sprites::number(call, width)? as f32
    }
    .max(0.0);
    if width <= f32::EPSILON {
        return Ok(None);
    }
    let map = native_children::named_children(call, actor)?;
    call.with_roots(&[map], |call| {
        let stream = field(call, table(call, map)?, "Stream")?;
        if stream == Value::Nil {
            return Ok(None);
        }
        let stream = table(call, stream)?;
        let stream_state = native_children::state(call, store, stream)?;
        Ok(Some(SongMeterDisplay {
            stream_width: width,
            stream_state,
        }))
    })
}
