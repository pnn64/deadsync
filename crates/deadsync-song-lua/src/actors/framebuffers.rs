//! ActorFrameTexture allocation requests and retained resource metadata.
use super::*;
use sprites::{field, set};

#[derive(Clone, Debug, PartialEq)]
pub struct FrameTexture {
    pub capture_name: String,
    pub capture_size: Option<[f32; 2]>,
    pub alpha_buffer: bool,
    pub depth_buffer: bool,
    pub float_buffer: bool,
}

pub(super) const METHODS: &[&str] = &[
    "Create",
    "EnableDepthBuffer",
    "EnableAlphaBuffer",
    "EnableFloat",
    "EnablePreserveTexture",
    "SetTextureName",
    "GetTexture",
];

fn allocation(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<TableId>, Error> {
    let value = get_static(call, actor, b"__songlua_aft_allocation")?;
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}

pub(super) fn capture_name(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<String>, Error> {
    if let Some(allocation) = allocation(call, actor)? {
        assets::field(call, allocation, b"name")
    } else {
        assets::field(call, actor, b"__songlua_aft_capture_name")
    }
}

pub(super) fn initialize(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if capture_name(call, actor)?.is_some() {
        return Ok(());
    }
    let globals = table(call, helper(call, 5)?)?;
    let count = field(call, globals, "__songlua_aft_counter")?;
    let count = if count == Value::Nil {
        0
    } else {
        let value = sprites::number(call, count)?;
        if !(i64::MIN as f64..9223372036854775808.0).contains(&value) {
            return Err(call.error(b"ActorFrameTexture counter out of range"));
        }
        value as i64
    }
    .saturating_add(1);
    set(
        call,
        globals,
        "__songlua_aft_counter",
        Value::Number(count as f64),
    )?;
    let name = call.try_intern(format!("ActorFrameTexture {count}").as_bytes())?;
    set(call, actor, "__songlua_aft_capture_name", name)
}

fn flag(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<bool, Error> {
    Ok(!matches!(
        field(call, actor, key)?,
        Value::Nil | Value::Boolean(false)
    ))
}

pub(super) fn read(call: &mut HostCall<'_>, actor: TableId) -> Result<FrameTexture, Error> {
    let allocation = allocation(call, actor)?;
    let flags = allocation.unwrap_or(actor);
    call.with_roots(&[Value::Table(flags)], |call| {
        let capture_name = capture_name(call, actor)?
            .ok_or_else(|| call.error(b"ActorFrameTexture has no texture name"))?;
        let capture_size = if let Some(allocation) = allocation {
            let size = field(call, allocation, "size")?;
            table(call, size)?;
            sprites::vector::<2>(call, size)?
        } else {
            None
        };
        Ok(FrameTexture {
            capture_name,
            capture_size,
            alpha_buffer: flag(call, flags, "__songlua_aft_alpha_buffer")?,
            depth_buffer: flag(call, flags, "__songlua_aft_depth_buffer")?,
            float_buffer: flag(call, flags, "__songlua_aft_float_buffer")?,
        })
    })
}

pub(super) fn report(call: &mut HostCall<'_>, message: &str) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = call.try_intern(b"__songlua_reporting_script_error")?;
    if !matches!(
        call.raw_table_get(globals, key)?,
        Value::Nil | Value::Boolean(false)
    ) {
        return Ok(());
    }
    call.with_roots(&[key], |call| {
        call.raw_table_set(globals, key, Value::Boolean(true))?;
        let result = (|| {
            messages::side_effect(call)?;
            let params = call.try_new_table()?;
            call.with_roots(&[Value::Table(params)], |call| {
                let text = call.try_intern(message.as_bytes())?;
                set(call, params, "message", text)?;
                messages::dispatch(call, b"ScriptError", Some(Value::Table(params)), true)
            })?;
            call.store().diagnostics.push(message.to_owned());
            Ok(())
        })();
        call.raw_table_set(globals, key, Value::Boolean(false))?;
        result
    })
}

fn immediate(
    call: &mut HostCall<'_>,
    actor: TableId,
    created: bool,
    value: bool,
) -> Result<(), Error> {
    action_capture::prepare_write(call, actor)?;
    let key = if created {
        "__songlua_state_aft_created"
    } else {
        "__songlua_state_aft_preserve"
    };
    set(call, actor, key, Value::Boolean(value))?;
    let guard = call.store();
    let state = &mut guard.actors.get_mut(&actor).unwrap().state.render;
    if created {
        state.aft_created = value;
    } else {
        state.aft_preserve = value;
    }
    song_immediate::record(
        call,
        actor,
        if created {
            crate::SongLuaOverlayUpdateTarget::AftCreated
        } else {
            crate::SongLuaOverlayUpdateTarget::AftPreserve
        },
        crate::SongLuaOverlayUpdateValue::Bool(value),
    );
    Ok(())
}

fn create(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if allocation(call, actor)?.is_some() {
        return report(call, "Can't Create an already created ActorFrameTexture");
    }
    let value = field(call, actor, "__songlua_aft_capture_name")?;
    let name = textures::collapse(&assets::text(call, value)?);
    let actors = call.store().actor_order.clone();
    for other in actors {
        if let Some(allocation) = allocation(call, other)? {
            let value = field(call, allocation, "name")?;
            if assets::text(call, value)? == name {
                return report(call, "ActorFrameTexture: Texture Name already in use.");
            }
        }
        if let Some(path) = assets::texture_path(call, other)?
            && textures::collapse(&crate::file_path_string(&path)) == name
        {
            return report(call, "ActorFrameTexture: Texture Name already in use.");
        }
    }
    let value = get_static(call, actor, b"__songlua_state_size")?;
    let requested = sprites::vector::<2>(call, value)?;
    {
        // Authored request vectors are also the native actor's render size.
        // The allocation takes a separate, truncated copy below.
        let store = call.store();
        let actor = store.actors.get_mut(&actor).unwrap();
        actor.state.size = requested.unwrap_or([1.0; 2]);
        actor.state.render.size = requested;
    }
    let size = requested.unwrap_or([1.0; 2]).map(f32::trunc);
    if size.iter().any(|v| !v.is_finite() || *v < 1.0) {
        return report(
            call,
            "ActorFrameTexture: Cannot have width or height less than 1",
        );
    }
    let allocation = call.try_new_table()?;
    call.with_roots(&[Value::Table(allocation)], |call| {
        let name = call.try_intern(name.as_bytes())?;
        set(call, allocation, "name", name)?;
        let size = sprites::vector_value(call, &size)?;
        set(call, allocation, "size", size)?;
        for key in [
            "__songlua_aft_alpha_buffer",
            "__songlua_aft_depth_buffer",
            "__songlua_aft_float_buffer",
        ] {
            let value = flag(call, actor, key)?;
            set(call, allocation, key, Value::Boolean(value))?;
        }
        let key = call.try_intern(b"__songlua_aft_allocation")?;
        call.raw_table_set(actor, key, Value::Table(allocation))?;
        messages::side_effect(call)?;
        immediate(call, actor, true, true)
    })
}

fn equal(call: &mut HostCall<'_>) -> Result<(), Error> {
    let left = table(call, arg(call, 0))?;
    let right = table(call, arg(call, 1))?;
    let left = get_static(call, left, b"__songlua_aft_capture_name")?;
    if left == Value::Nil {
        return call.push(Value::Boolean(false));
    }
    let left = assets::text(call, left)?;
    let right = get_static(call, right, b"__songlua_aft_capture_name")?;
    let result = right != Value::Nil && left == assets::text(call, right)?;
    call.push(Value::Boolean(result))
}

fn texture(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if allocation(call, actor)?.is_none() {
        return call.push(Value::Nil);
    }
    let texture = call.try_new_table()?;
    call.with_roots(&[Value::Table(texture)], |call| {
        let kind = call.try_intern(b"RageTexture")?;
        set(call, texture, "__songlua_native_type", kind)?;
        if let Some(name) = capture_name(call, actor)? {
            let name = call.try_intern(name.as_bytes())?;
            set(call, texture, "__songlua_aft_capture_name", name)?;
        }
        let globals = table(call, helper(call, 5)?)?;
        let meta = get_static(call, globals, b"__songlua_aft_texture_meta")?;
        let meta = if meta == Value::Nil {
            let meta = call.try_new_table()?;
            call.with_roots(&[Value::Table(meta)], |call| {
                let equal = call.new_host_function(equal, &[])?;
                set(call, meta, "__eq", equal)?;
                let key = call.try_intern(b"__songlua_aft_texture_meta")?;
                call.raw_table_set(globals, key, Value::Table(meta))?;
                Ok(meta)
            })?
        } else {
            table(call, meta)?
        };
        let helpers = table(call, call.captures()[1])?;
        let setmetatable = get_static(call, helpers, b"setmetatable")?;
        call.call(setmetatable, &[Value::Table(texture), Value::Table(meta)])?;
        let allocation = field(call, actor, "__songlua_aft_allocation")?;
        let allocation = table(call, allocation)?;
        call.with_roots(&[Value::Table(allocation)], |call| {
            let size = field(call, allocation, "size")?;
            let size = table(call, size)?;
            let dimensions = call.with_roots(&[Value::Table(size)], |call| {
                let mut dimensions = [0.; 2];
                for (i, dimension) in dimensions.iter_mut().enumerate() {
                    let value = call.raw_table_get(size, Value::Number((i + 1) as f64))?;
                    *dimension = sprites::number(call, value)? as f32;
                }
                Ok(dimensions)
            })?;
            let value = field(call, allocation, "name")?;
            let path = textures::collapse(&assets::text(call, value)?);
            textures::install(
                call,
                actor,
                texture,
                textures::Info {
                    path,
                    source: dimensions,
                    image: dimensions,
                    allocation: dimensions
                        .map(|n| (n as u32).checked_next_power_of_two().unwrap_or(u32::MAX) as f32),
                    frames: 1,
                },
                (1, 1),
            )?;
            let captures = [
                call.captures()[0],
                call.captures()[1],
                Value::Table(actor),
                Value::Table(texture),
            ];
            for (name, begin) in [("BeginRenderingTo", true), ("FinishRenderingTo", false)] {
                let function = call.new_host_function_with_data(
                    render_target,
                    &captures,
                    TargetBinding { begin },
                )?;
                set(call, texture, name, function)?;
            }
            call.push(Value::Table(texture))
        })
    })
}

#[derive(Clone)]
struct TargetBinding {
    begin: bool,
}

fn render_target(call: &mut HostCall<'_>) -> Result<(), Error> {
    let TargetBinding { begin } = call.function_data::<TargetBinding>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    if begin {
        let offset = usize::from(matches!(arg(call, 0), Value::Table(_)));
        let preserve = !matches!(arg(call, offset), Value::Nil | Value::Boolean(false));
        draw::begin(call, actor, preserve)?;
    } else {
        draw::finish(call, actor)?;
    }
    call.push(call.captures()[3])
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    match name {
        "Create" => create(call, actor)?,
        "GetTexture" => return texture(call, actor),
        "SetTextureName" => {
            let id = call
                .coerce_string(arg(call, 1))?
                .ok_or_else(|| call.error(b"SetTextureName: string expected"))?;
            set(call, actor, "__songlua_aft_capture_name", Value::String(id))?;
        }
        _ => {
            let Value::Boolean(value) = arg(call, 1) else {
                return Err(call.error(b"boolean expected"));
            };
            let key = match name {
                "EnableAlphaBuffer" => "__songlua_aft_alpha_buffer",
                "EnableDepthBuffer" => "__songlua_aft_depth_buffer",
                "EnableFloat" => "__songlua_aft_float_buffer",
                "EnablePreserveTexture" => {
                    immediate(call, actor, false, value)?;
                    return call.push(Value::Table(actor));
                }
                _ => unreachable!(),
            };
            set(call, actor, key, Value::Boolean(value))?;
        }
    }
    call.push(Value::Table(actor))
}
