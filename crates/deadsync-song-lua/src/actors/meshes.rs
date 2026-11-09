//! ActorMultiVertex uploads and renderer-facing mesh reads over the Rust VM.
use super::*;
use crate::{SongLuaActorMultiVertexPoint as Point, SongLuaOverlayMeshVertex as Vertex};
use sprites::{field, set};
use std::{path::PathBuf, sync::OnceLock};
#[path = "mesh_geometry.rs"]
mod geometry;

#[derive(Clone, Debug)]
pub struct ActorMesh {
    pub vertices: Arc<[Vertex]>,
    pub texture_path: Option<PathBuf>,
}

pub(super) const METHODS: &[&str] = &[
    "SetVertex",
    "SetVertices",
    "SetEffectMode",
    "SetTextureMode",
    "SetLineWidth",
    "SetDrawState",
    "SetNumVertices",
    "GetNumVertices",
    "GetDestDrawMode",
    "GetDestFirstToDraw",
    "GetDestNumToDraw",
    "GetCurrDrawMode",
    "GetCurrFirstToDraw",
    "GetCurrNumToDraw",
    "GetSpline",
    "SetVertsFromSplines",
    "GetUseAnimationState",
    "SetUseAnimationState",
    "GetNumStates",
    "GetNumQuadStates",
    "AddState",
    "RemoveState",
    "GetState",
    "SetState",
    "GetStateData",
    "SetStateData",
    "SetStateProperties",
    "SetAllStateDelays",
    "SetSecondsIntoAnimation",
    "AddQuadState",
    "RemoveQuadState",
    "GetQuadState",
    "SetQuadState",
    "ForceStateUpdate",
    "GetDecodeMovie",
    "SetDecodeMovie",
    "SetTexture",
    "GetTexture",
    "LoadTexture",
];

fn color(call: &mut HostCall<'_>, value: Value) -> Result<[f32; 4], Error> {
    match value {
        Value::Table(t) => call.with_roots(&[value], |call| {
            let mut color = [1.0; 4];
            for (i, component) in color.iter_mut().enumerate() {
                let value = call.raw_table_get(t, Value::Number((i + 1) as f64))?;
                if i == 3 && value == Value::Nil {
                    continue;
                }
                let converted = if matches!(value, Value::Number(_)) {
                    value
                } else {
                    call.call(helper(call, 6)?, &[value])?[0]
                };
                let Value::Number(n) = converted else {
                    return Ok([1.0; 4]);
                };
                *component = n as f32;
            }
            Ok(color)
        }),
        Value::String(_) => Ok(render::text(call, value)
            .and_then(crate::parse_color_text)
            .unwrap_or([1.0; 4])),
        _ => Ok([1.0; 4]),
    }
}

pub(super) fn points(call: &mut HostCall<'_>, vertices: TableId) -> Result<Vec<Point>, Error> {
    call.with_roots(&[Value::Table(vertices)], |call| {
        let mut points = Vec::with_capacity(call.raw_table_len(vertices)?);
        for i in 1usize.. {
            let value = call.raw_table_get(vertices, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            let Value::Table(vertex) = value else {
                continue;
            };
            let point = call.with_roots(&[value], |call| {
                let position = call.raw_table_get(vertex, Value::Number(1.0))?;
                let Value::Table(position_table) = position else {
                    return Ok(None);
                };
                call.with_roots(&[position], |call| {
                    let x = call.raw_table_get(position_table, Value::Number(1.0))?;
                    let y = call.raw_table_get(position_table, Value::Number(2.0))?;
                    let (Some(x), Some(y)) =
                        (float_scalar(call, x, true), float_scalar(call, y, true))
                    else {
                        return Ok(None);
                    };
                    let value = call.raw_table_get(vertex, Value::Number(2.0))?;
                    let color = color(call, value)?;
                    let value = call.raw_table_get(vertex, Value::Number(3.0))?;
                    let mut uv = [0.0; 2];
                    if let Value::Table(t) = value {
                        for (i, component) in uv.iter_mut().enumerate() {
                            let value = call.raw_table_get(t, Value::Number((i + 1) as f64))?;
                            *component = float_scalar(call, value, false).unwrap_or(0.0);
                        }
                    }
                    let z = call.raw_table_get(position_table, Value::Number(3.0))?;
                    Ok(Some(Point {
                        pos: [x, y],
                        z: float_scalar(call, z, true).unwrap_or(0.0),
                        color,
                        uv,
                    }))
                })
            })?;
            if let Some(point) = point {
                points.push(point);
            }
        }
        Ok(points)
    })
}

const POINT_BYTES: usize = 9 * size_of::<f32>();
pub(super) const BUFFERS_KEY: &str = "mesh_buffers";
fn buffers(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    let helpers = table(call, call.captures()[1])?;
    let key = call.try_intern(BUFFERS_KEY.as_bytes())?;
    table(call, call.raw_table_get(helpers, key)?)
}
pub(super) fn upload(
    call: &mut HostCall<'_>,
    actor: TableId,
    vertices: TableId,
) -> Result<(), Error> {
    // Count is published before conversion, including when a later write fails.
    let count = call.raw_table_len(vertices)?;
    set(
        call,
        actor,
        "__songlua_vertex_count",
        Value::Number(count as f64),
    )?;
    let points = points(call, vertices)?;
    let size = points
        .len()
        .checked_mul(POINT_BYTES)
        .ok_or_else(|| call.error(b"vertex buffer too large"))?;
    let buffer = call.try_new_userdata(size)?;
    let Value::Userdata(id) = buffer else {
        unreachable!()
    };
    let bytes = call.userdata_bytes_mut(id).expect("new vertex storage");
    for (bytes, point) in bytes
        .as_chunks_mut::<POINT_BYTES>()
        .0
        .iter_mut()
        .zip(points)
    {
        for (bytes, scalar) in bytes.as_chunks_mut::<4>().0.iter_mut().zip([
            point.pos[0],
            point.pos[1],
            point.z,
            point.color[0],
            point.color[1],
            point.color[2],
            point.color[3],
            point.uv[0],
            point.uv[1],
        ]) {
            bytes.copy_from_slice(&scalar.to_le_bytes());
        }
    }
    // A private weak-key set authenticates this userdata type without retaining
    // uploaded buffers. The VM accounts for and frees all owned scalar storage.
    call.with_roots(&[buffer], |call| {
        let buffers = buffers(call)?;
        call.raw_table_set(buffers, buffer, Value::Boolean(true))?;
        set(call, actor, "__songlua_vertices", buffer)
    })
}

pub(super) fn source(call: &mut HostCall<'_>, value: Value) -> Result<Option<Vec<Point>>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Table(t) => points(call, t).map(Some),
        Value::Userdata(id) => {
            let buffers = buffers(call)?;
            if call.raw_table_get(buffers, value)? != Value::Boolean(true) {
                return Err(call.error(b"invalid ActorMultiVertex vertex buffer"));
            }
            let bytes = call.userdata_bytes(id).expect("live vertex storage");
            Ok(Some(
                bytes
                    .as_chunks::<POINT_BYTES>()
                    .0
                    .iter()
                    .map(|bytes| {
                        let mut values = [0.0; 9];
                        for (value, bytes) in values.iter_mut().zip(bytes.as_chunks::<4>().0) {
                            *value = f32::from_le_bytes(*bytes);
                        }
                        Point {
                            pos: [values[0], values[1]],
                            z: values[2],
                            color: [values[3], values[4], values[5], values[6]],
                            uv: [values[7], values[8]],
                        }
                    })
                    .collect(),
            ))
        }
        _ => Err(call.error(b"invalid ActorMultiVertex vertex buffer")),
    }
}

fn range_value(call: &mut HostCall<'_>, state: TableId, key: &str) -> Result<Option<i32>, Error> {
    match field(call, state, key) {
        Ok(value) => Ok(integer_scalar(call, value)),
        // Production ignores ordinary authored errors in First/Num getters.
        Err(error) if error.kind() == ErrorKind::Runtime => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn vertices(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<Arc<[Vertex]>>, Error> {
    let value = field(call, actor, "__songlua_vertices")?;
    let Some(points) = source(call, value)?.filter(|points| !points.is_empty()) else {
        return Ok(None);
    };
    let value = field(call, actor, "__songlua_draw_state")?;
    let (first, num) = if value == Value::Nil {
        (1, -1)
    } else {
        let state = table(call, value)?;
        call.with_roots(&[value], |call| {
            Ok((
                range_value(call, state, "First")?.unwrap_or(1),
                range_value(call, state, "Num")?.unwrap_or(-1),
            ))
        })?
    };
    let start = (first.max(1) as usize - 1).min(points.len());
    let end = if num < 0 {
        points.len()
    } else {
        start.saturating_add(num as usize).min(points.len())
    };
    if end <= start {
        return Ok(None);
    }
    let mode = assets::field(call, actor, b"__songlua_draw_state_mode")?
        .unwrap_or_else(|| "DrawMode_Triangles".into());
    let value = field(call, actor, "__songlua_line_width")?;
    let width = if value == Value::Nil {
        1.0
    } else {
        sprites::number(call, value)? as f32
    }
    .max(0.0);
    let Some(vertices) = geometry::actor_multi_vertex_mesh_arc(&points[start..end], &mode, width)
    else {
        return Ok(None);
    };
    Ok(Some(vertices))
}
pub(super) fn texture_path(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<PathBuf>, Error> {
    let texture_path = if sprites::default_texture(call, actor)? {
        const KEY: &str = "__songlua_native_default_texture.png";
        static PREPARED: OnceLock<()> = OnceLock::new();
        PREPARED.get_or_init(|| {
            deadlib_assets::register_prepared_generated_texture(
                KEY,
                image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
                [1, 1],
                Default::default(),
            )
        });
        Some(PathBuf::from(KEY))
    } else {
        assets::texture_path(call, actor)?
    };
    Ok(texture_path)
}
pub(super) fn read(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<ActorMesh>, Error> {
    let Some(vertices) = vertices(call, actor)? else {
        return Ok(None);
    };
    let texture_path = texture_path(call, actor)?;
    Ok(Some(ActorMesh {
        vertices,
        texture_path,
    }))
}

pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let value = arg(call, 1);
    match name {
        "SetVertices" => {
            if let Value::Table(vertices) = value {
                upload(call, actor, vertices)?;
            }
        }
        "SetNumVertices" => {
            let count = integer_scalar(call, value).unwrap_or(0).max(0);
            set(
                call,
                actor,
                "__songlua_vertex_count",
                Value::Number(count as f64),
            )?;
        }
        "GetNumVertices" => {
            let value = field(call, actor, "__songlua_vertex_count")?;
            let count = if value == Value::Nil {
                let value = field(call, actor, "__songlua_vertices")?;
                if value == Value::Nil {
                    0.0
                } else {
                    call.raw_table_len(table(call, value)?)? as f64
                }
            } else {
                let n = sprites::number(call, value)?;
                if !(i64::MIN as f64..9223372036854775808.0).contains(&n) {
                    return Err(call.error(b"vertex count out of range"));
                }
                (n as i64).max(0) as f64
            };
            return call.push(Value::Number(count));
        }
        "SetDrawState" => {
            if let Value::Table(state) = value {
                let value = field(call, actor, "__songlua_draw_state")?;
                let destination = if value == Value::Nil {
                    call.try_new_table()?
                } else {
                    table(call, value)?
                };
                call.with_roots(&[Value::Table(destination)], |call| {
                    for key in ["Mode", "First", "Num"] {
                        let value = field(call, state, key)?;
                        if value != Value::Nil {
                            set(call, destination, key, value)?;
                        }
                    }
                    set(
                        call,
                        actor,
                        "__songlua_draw_state",
                        Value::Table(destination),
                    )?;
                    if let Some(mode) = assets::field(call, state, b"Mode")? {
                        let mode = call.try_intern(mode.as_bytes())?;
                        set(call, actor, "__songlua_draw_state_mode", mode)?;
                    }
                    Ok(())
                })?;
            }
        }
        "GetDestDrawMode" => {
            let mode = assets::field(call, actor, b"__songlua_draw_state_mode")?;
            let value = match mode {
                Some(mode) => call.try_intern(mode.as_bytes())?,
                None => Value::Nil,
            };
            return call.push(value);
        }
        "SetLineWidth" => {
            if let Some(width) = float_scalar(call, value, false).filter(|w| *w >= 0.0) {
                set(
                    call,
                    actor,
                    "__songlua_line_width",
                    Value::Number(width as f64),
                )?;
            }
        }
        "LoadTexture" => {
            super::action_capture::prepare(call, store, actor)?;
            if value == Value::Nil {
                sprites::set_texture(call, store, actor, value, true)?;
                let key = call.try_intern(b"__songlua_default_texture")?;
                call.raw_table_set(actor, key, Value::Boolean(true))?;
            } else {
                let path = call
                    .coerce_string(value)?
                    .ok_or_else(|| call.error(b"LoadTexture: string expected"))?;
                call.with_roots(&[Value::String(path)], |call| {
                    sprites::set_texture(call, store, actor, Value::String(path), false)
                })?;
            }
        }
        "SetEffectMode"
        | "GetNumStates"
        | "GetState"
        | "SetStateProperties"
        | "SetAllStateDelays"
        | "SetSecondsIntoAnimation"
        | "GetDecodeMovie"
        | "SetDecodeMovie"
        | "SetTexture"
        | "GetTexture" => return sprites::method(call, store, actor, name),
        _ => {
            return Err(call.error(
                format!("native actor method {name} has no semantic implementation").as_bytes(),
            ));
        }
    }
    call.push(Value::Table(actor))
}
