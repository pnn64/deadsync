//! GraphDisplay's native children, life-record capture and owned render payload.
use super::*;
use sprites::{field, set};

#[derive(Clone, Debug)]
pub struct GraphDisplay {
    pub size: [f32; 2],
    pub body_values: Arc<[f32]>,
    pub body_state: crate::SongLuaOverlayState,
    pub line_state: crate::SongLuaOverlayState,
}

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let methods = native_children::helper_function(call, b"graph_methods")?;
    let methods = table(call, methods)?;
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    for name in ["Load", "Set"] {
        let method = get(call, methods, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(invoke_bound, &captures, name)?;
        let key = call.try_intern(name.as_bytes())?;
        call.raw_table_set(bound, key, function)?;
    }
    Ok(())
}

fn child(
    call: &mut HostCall<'_>,
    scene: u64,
    parent: TableId,
    kind: &str,
    name: &str,
) -> Result<TableId, Error> {
    let attrs = call.try_new_table()?;
    call.with_roots(&[Value::Table(attrs)], |call| {
        let class = call.try_intern(kind.as_bytes())?;
        set(call, attrs, "Class", class)?;
        // Native dummy actors inherit the active file scope, not the parent's
        // declaration directory, and are registered before that parent's Init.
        for key in ["__songlua_script_dir", "__songlua_song_dir"] {
            let value = field(call, table(call, helper(call, 5)?)?, key)?;
            if value != Value::Nil {
                assets::text(call, value)?;
                set(call, attrs, key, value)?;
            }
        }
        let node = call.call(helper(call, 2)?, &[Value::Table(attrs)])?[0];
        let child = call.with_roots(&[node], |call| {
            Ok(build(call, scene, table(call, node)?, Some(parent), false, true)?.unwrap())
        })?;
        {
            let store = call.store();
            // Native dummy graph children register during materialization,
            // before Init registers the ordinary nodes of this new scene.
            store.actor_order.retain(|id| *id != child);
            let position = store
                .actor_order
                .iter()
                .position(|id| {
                    let a = &store.actors[id];
                    a.scene == scene
                        && !matches!(a.state.class, "GraphDisplayBody" | "GraphDisplayLine")
                })
                .unwrap_or(store.actor_order.len());
            store.actor_order.insert(position, child);
        }
        let value = call.try_intern(name.as_bytes())?;
        set(call, child, "Name", value)?;
        call.store().actors.get_mut(&child).unwrap().state.name = name.as_bytes().to_vec();
        Ok(child)
    })
}

pub(super) fn initialize(call: &mut HostCall<'_>, scene: u64, actor: TableId) -> Result<(), Error> {
    {
        let store = call.store();
        let size = store.graph_size;
        let actor = store.actors.get_mut(&actor).unwrap();
        actor.state.size = size;
        actor.state.render.size = Some(size);
    }
    let first = child(call, scene, actor, "GraphDisplayBody", "")?;
    let second = child(call, scene, actor, "GraphDisplayBody", "")?;
    let line = child(call, scene, actor, "GraphDisplayLine", "Line")?;
    let bodies = group(call, &[first, second])?;
    native_children::insert(call, actor, "", bodies)?;
    native_children::insert(call, actor, "Line", Value::Table(line))?;
    Ok(())
}

fn tolerate<T>(result: Result<T, Error>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == ErrorKind::Runtime => Ok(None),
        Err(error) => Err(error),
    }
}

fn capture(
    call: &mut HostCall<'_>,
    actor: TableId,
    stage: Value,
    player: Value,
) -> Result<(), Error> {
    let seconds = if let Value::Table(stage) = stage {
        tolerate((|| {
            let value = field(call, stage, "__songlua_total_steps_seconds")?;
            if value == Value::Nil {
                Ok(0.0)
            } else {
                sprites::number(call, value).map(|n| n as f32)
            }
        })())?
        .unwrap_or(0.0)
    } else {
        0.0
    };
    let record = if let Value::Table(player) = player {
        tolerate((|| {
            let method = field(call, player, "GetLifeRecord")?;
            if !matches!(method, Value::Function(_) | Value::Native(_)) {
                return Ok(Value::Nil);
            }
            Ok(call
                .call(
                    method,
                    &[
                        Value::Table(player),
                        Value::Number(seconds as f64),
                        Value::Number(crate::GRAPH_DISPLAY_VALUE_RESOLUTION as f64),
                    ],
                )?
                .first()
                .copied()
                .unwrap_or(Value::Nil))
        })())?
        .unwrap_or(Value::Nil)
    } else {
        Value::Nil
    };
    call.with_roots(&[record], |call| {
        // Production eagerly creates the fallback even after a successful
        // callback. Keep the returned table rooted across that allocation.
        let fallback = call.try_new_table()?;
        for i in 1..=crate::GRAPH_DISPLAY_VALUE_RESOLUTION {
            call.raw_table_set(
                fallback,
                Value::Number(i as f64),
                Value::Number(crate::SONG_LUA_INITIAL_LIFE as f64),
            )?;
        }
        let values = if matches!(record, Value::Table(_)) {
            record
        } else {
            Value::Table(fallback)
        };
        set(call, actor, "__songlua_graph_display_values", values)
    })
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let graph = assets::field(call, actor, b"__songlua_actor_type")?
        .is_some_and(|kind| kind.eq_ignore_ascii_case("GraphDisplay"));
    if name == "Load" {
        if !graph {
            return sprites::method(call, actor, name);
        }
        if let Some(metric) = render::text(call, arg(call, 1)).map(str::to_owned) {
            let metric = call.try_intern(metric.as_bytes())?;
            set(call, actor, "__songlua_graph_display_metric", metric)?;
        }
    } else if graph {
        set(
            call,
            actor,
            "__songlua_graph_display_set",
            Value::Boolean(true),
        )?;
        for (i, key) in [
            (1, "__songlua_graph_display_stage_stats"),
            (2, "__songlua_graph_display_player_stats"),
        ] {
            if let Some(value) = call.arguments().get(i).copied() {
                set(call, actor, key, value)?;
            }
        }
        capture(call, actor, arg(call, 1), arg(call, 2))?;
    }
    call.push(Value::Table(actor))
}

fn child_state(
    call: &mut HostCall<'_>,
    actor: TableId,
    body: bool,
) -> Result<crate::SongLuaOverlayState, Error> {
    let map = native_children::named_children(call, actor)?;
    call.with_roots(&[map], |call| {
        let child = field(call, table(call, map)?, if body { "" } else { "Line" })?;
        if child == Value::Nil {
            return Ok(crate::SongLuaOverlayState::default());
        }
        call.with_roots(&[child], |call| {
            let child = table(call, child)?;
            let child = if body {
                let second = call.raw_table_get(child, Value::Number(2.0))?;
                if second != Value::Nil {
                    Some(table(call, second)?)
                } else {
                    let first = call.raw_table_get(child, Value::Number(1.0))?;
                    if let Value::Table(first) = first {
                        Some(first)
                    } else {
                        None
                    }
                }
            } else {
                Some(child)
            };
            child.map_or(Ok(crate::SongLuaOverlayState::default()), |child| {
                native_children::state(call, child)
            })
        })
    })
}

pub(super) fn read(call: &mut HostCall<'_>, actor: TableId) -> Result<GraphDisplay, Error> {
    let size = native_children::state(call, actor)?
        .size
        .unwrap_or(call.store().graph_size)
        .map(|n| n.abs().max(1.0));
    let values = field(call, actor, "__songlua_graph_display_values")?;
    let body_values = call.with_roots(&[values], |call| {
        let mut result = vec![crate::SONG_LUA_INITIAL_LIFE; crate::GRAPH_DISPLAY_VALUE_RESOLUTION];
        if values != Value::Nil {
            let values = table(call, values)?;
            for (i, out) in result.iter_mut().enumerate() {
                let value = call.raw_table_get(values, Value::Number((i + 1) as f64))?;
                *out = float_scalar(call, value, false)
                    .unwrap_or(crate::SONG_LUA_INITIAL_LIFE)
                    .clamp(0.0, 1.0);
            }
        }
        Ok(Arc::from(result))
    })?;
    Ok(GraphDisplay {
        size,
        body_values,
        body_state: child_state(call, actor, true)?,
        line_state: child_state(call, actor, false)?,
    })
}
