//! Custom draw traversal and render-target scopes over the Rust host API.
use super::*;
use crate::draw_commands::{
    Capture, Context, DRAW_INSTALLED_KEY, MAX_BYTES, MAX_OPS, RawCall, RawFrame, RawOp,
    partially_opaque,
};
use sprites::{field, set};

const ACTIVE: &str = "__songlua_draw_callback_active";
const FUNCTION: &[u8] = b"__songlua_draw_function";

fn live(store: &Shared) -> bool {
    store
        .lock()
        .unwrap()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
}
fn pointer(call: &mut HostCall<'_>, actor: TableId) -> usize {
    call.object_pointer(Value::Table(actor))
        .expect("rooted actor identity")
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn raw_text(call: &mut HostCall<'_>, actor: TableId, key: &[u8]) -> Result<Option<String>, Error> {
    let value = get(call, actor, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        assets::text(call, value).map(Some)
    }
}
fn raw_bytes(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &[u8],
) -> Result<Option<Vec<u8>>, Error> {
    let value = get(call, actor, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        bytes(call, value).map(Some)
    }
}
fn function(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    let value = get(call, actor, FUNCTION)?;
    if matches!(value, Value::Nil | Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"actor draw function must be a function"))
    }
}
fn budget(call: &HostCall<'_>) -> Error {
    call.error(b"custom draw capture exceeds its song budget")
}
fn push(call: &HostCall<'_>, store: &Shared, op: RawOp) -> Result<(), Error> {
    let live = live(store);
    let mut store = store.lock().unwrap();
    let Some(capture) = &mut store.draw_capture else {
        return Ok(());
    };
    let Some(context) = &capture.context else {
        return Ok(());
    };
    let owner = context.owner;
    let extra = match &op {
        RawOp::Draw(_, _, context, vertices) => {
            context.parents.len() * size_of::<crate::SongLuaOverlayState>()
                + vertices.as_ref().map_or(0, |vertices| {
                    vertices.len() * size_of::<crate::SongLuaOverlayMeshVertex>()
                })
        }
        _ => 0,
    };
    let bytes = size_of::<RawCall>() + extra;
    if (!live && capture.frame_ops >= MAX_OPS) || capture.bytes.saturating_add(bytes) > MAX_BYTES {
        return Err(budget(call));
    }
    capture.bytes += bytes;
    capture.frame_ops += 1;
    capture.calls.push(RawCall { owner, op });
    Ok(())
}
pub(super) fn begin(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    preserve: bool,
) -> Result<(), Error> {
    let pointer = pointer(call, actor);
    push(call, store, RawOp::Begin(pointer, preserve))?;
    if let Some(capture) = &mut store.lock().unwrap().draw_capture
        && let Some(prior) = capture.context.take()
    {
        let owner = prior.owner;
        capture.targets.push((pointer, prior));
        capture.context = Some(Context {
            owner,
            ..Context::default()
        });
    }
    Ok(())
}
pub(super) fn finish(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let pointer = pointer(call, actor);
    if let Some(capture) = &store.lock().unwrap().draw_capture
        && capture.context.is_some()
        && capture
            .targets
            .last()
            .is_none_or(|(target, _)| *target != pointer)
    {
        return Err(call.error(b"FinishRenderingTo does not match the active target"));
    }
    push(call, store, RawOp::Finish(pointer))?;
    if let Some(capture) = &mut store.lock().unwrap().draw_capture
        && let Some((_, prior)) = capture.targets.pop()
    {
        capture.context = Some(prior);
    }
    Ok(())
}
fn state(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<crate::SongLuaOverlayState, Error> {
    // The native preparation reader resolves an uninitialized Sprite's
    // Texture/AFT getter before reading later scalar fields. Keep this read
    // at draw time; snapshotting raw semantic state must not invoke it early.
    if !live(store) {
        let kind = get(call, actor, b"__songlua_actor_type")?;
        let sprite = matches!(kind,Value::String(id)if call.string_bytes(id).is_some_and(|s|s.eq_ignore_ascii_case(b"Sprite")));
        if sprite && get(call, actor, b"__songlua_state_sprite_texture")? == Value::Nil {
            return initial_state::read(call, store, actor);
        }
    }
    let store = store.lock().unwrap();
    let live = store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
    let actor = store
        .actors
        .get(&actor)
        .ok_or_else(|| call.error(b"draw actor has been released"))?;
    let mut state = actor.state.render;
    // The existing preparation reader exports authored destination fields.
    // Live drawing reads the native current tween pose and its baked spin flag.
    // Keep that distinction at this boundary, without changing queue state.
    if live {
        actor.current.write_to(&mut state);
    } else {
        actor.destination().write_to(&mut state);
        // Spin updates its native pose without publishing the preparation
        // reader's rotation fields until a setter/queue synchronization does.
        [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = actor.state.rotation;
    }
    state.visible = actor.state.visible;
    state.draw_order = actor.state.draw_order;
    state.hibernating = actor.hibernate > 0.0;
    state.spin_baked = live && actor.effects.spin_baked;
    Ok(state)
}
fn scoped_context(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    prior: &Context,
) -> Result<Option<Context>, Error> {
    let transition = get(call, actor, b"__songlua_transition_state")?;
    if transition != Value::Nil {
        let transition = sprites::number(call, transition)?.trunc();
        if !(0.0..256.0).contains(&transition) {
            return Err(call.error(b"transition state out of range"));
        }
        if transition == 0.0 {
            return Ok(None);
        }
    }
    if truthy(get(call, actor, b"__songlua_theme_hibernating")?) {
        return Ok(None);
    }
    let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
    if kind.eq_ignore_ascii_case("ActorFrameTexture") {
        let allocation = get(call, actor, b"__songlua_aft_allocation")?;
        if allocation == Value::Nil {
            return Ok(None);
        }
        table(call, allocation)?;
    }
    let (wrappers, live) = {
        let store = store.lock().unwrap();
        let native = store
            .actors
            .get(&actor)
            .ok_or_else(|| call.error(b"draw actor has been released"))?;
        (
            native.wrappers.clone(),
            store.clock.live.load(std::sync::atomic::Ordering::Relaxed),
        )
    };
    let mut parents = Vec::with_capacity(prior.parents.len() + wrappers.len() + 1);
    parents.extend_from_slice(&prior.parents);
    for wrapper in wrappers.into_iter().rev() {
        let wrapper = state(call, store, wrapper)?;
        if !wrapper.draw_visible() || (live && !partially_opaque(wrapper, &parents)) {
            return Ok(None);
        }
        parents.push(wrapper);
    }
    let state = state(call, store, actor)?;
    if !state.draw_visible() || (live && !partially_opaque(state, &parents)) {
        return Ok(None);
    }
    parents.push(state);
    Ok(Some(Context {
        owner: prior.owner,
        parents: parents.into(),
        camera: if state.fov.is_some() {
            Some(state)
        } else {
            prior.camera
        },
    }))
}
fn children_each(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    mut action: impl FnMut(&mut HostCall<'_>, &Shared, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut index = 1;
    loop {
        let value = call.raw_table_get(actor, Value::Number(index as f64))?;
        if value == Value::Nil {
            return Ok(());
        }
        let child = table(call, value)?;
        call.with_roots(&[value], |call| action(call, store, child))?;
        index += 1;
    }
}
fn report(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    error: Error,
) -> Result<(), Error> {
    diagnostics::report(call, store, actor, diagnostics::Callback::Draw, error)
}
fn draw(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let prior = store
        .lock()
        .unwrap()
        .draw_capture
        .as_ref()
        .and_then(|capture| capture.context.clone());
    let Some(prior) = prior else {
        return immediate(call, store, actor);
    };
    let Some(context) = scoped_context(call, store, actor, &prior)? else {
        return Ok(());
    };
    store.lock().unwrap().draw_capture.as_mut().unwrap().context = Some(context.clone());
    let result = (|| {
        let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
        let target = kind.eq_ignore_ascii_case("ActorFrameTexture");
        if target {
            let preserve = truthy(field(call, actor, "__songlua_state_aft_preserve")?);
            begin(call, store, actor, preserve)?;
        }
        let result = (|| {
            let callback = function(call, actor)?;
            if callback != Value::Nil {
                let result = call.with_roots(&[callback], |call| {
                    let sources = store.lock().unwrap().live_sources.is_some();
                    let observed = sources && !theme_callback(call, callback)?;
                    if observed {
                        store.lock().unwrap().song_callback_depth += 1;
                    }
                    let result = run_scoped(call, actor, callback, None);
                    if observed {
                        store.lock().unwrap().song_callback_depth -= 1;
                    }
                    result
                });
                return if live(store) {
                    result.or_else(|error| report(call, store, actor, error))
                } else {
                    // Owned actors already use tween replay in preparation as
                    // well as live playback. Like the native replay path, Draw
                    // does not dispatch their queued commands before Update.
                    result
                };
            }
            let screen = raw_text(call, actor, b"__songlua_top_screen_child_name")?;
            let player = raw_text(call, actor, b"__songlua_player_child_name")?;
            let external = matches!(
                screen.as_deref(),
                Some("Underlay" | "Overlay" | "SongBackground" | "SongForeground" | "In")
            ) || matches!(player.as_deref(), Some("Judgment" | "Combo"));
            if kind.eq_ignore_ascii_case("Actor") {
                return Ok(());
            }
            if kind.eq_ignore_ascii_case("BitmapText")
                && raw_bytes(call, actor, b"Text")?.is_none_or(|text| text.is_empty())
            {
                return Ok(());
            }
            let local = *context
                .parents
                .last()
                .expect("actor included in draw context");
            if kind.eq_ignore_ascii_case("Sprite")
                && raw_bytes(call, actor, b"Texture")?.as_deref()
                    == Some(crate::blank_texture::NATIVE_BLANK_TEXTURE_PATH.as_bytes())
                && local.blend == crate::SongLuaOverlayBlendMode::Alpha
                && !local.depth_write
                && !local.mask_source
                && !local.mask_dest
            {
                return Ok(());
            }
            if !external && kind.eq_ignore_ascii_case("ActorProxy") {
                let target = get(call, actor, b"__songlua_proxy_target_actor")?;
                if target == Value::Nil {
                    return Ok(());
                }
                table(call, target)?;
            }
            if !external
                && (kind.eq_ignore_ascii_case("ActorFrame")
                    || kind.eq_ignore_ascii_case("ActorFrameTexture"))
            {
                return children_each(call, store, actor, draw);
            }
            let pointer = pointer(call, actor);
            let missing = !store
                .lock()
                .unwrap()
                .draw_capture
                .as_ref()
                .unwrap()
                .labels
                .contains_key(&pointer);
            if missing {
                let name = raw_text(call, actor, b"Name")?.unwrap_or_default();
                let label = format!(
                    "{kind} '{name}' (screen child '{}')",
                    screen.unwrap_or_default()
                );
                let bytes = label.len() + size_of::<(usize, String)>();
                let mut store = store.lock().unwrap();
                let capture = store.draw_capture.as_mut().unwrap();
                if capture.bytes.saturating_add(bytes) > MAX_BYTES {
                    return Err(budget(call));
                }
                capture.bytes += bytes;
                capture.labels.insert(pointer, label);
            }
            let vertices = meshes::read(call, actor)?.map(|mesh| mesh.vertices);
            let parents = Arc::from(&context.parents[..context.parents.len() - 1]);
            push(
                call,
                store,
                RawOp::Draw(pointer, local, Context { parents, ..context }, vertices),
            )
        })();
        if target {
            finish(call, store, actor)?;
        }
        result
    })();
    store.lock().unwrap().draw_capture.as_mut().unwrap().context = Some(prior);
    result
}
fn second(call: &mut HostCall<'_>, store: &Shared) -> Result<f32, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let runtime = field(call, globals, crate::SONG_LUA_RUNTIME_KEY)?;
    let table = table(call, runtime)?;
    call.with_roots(&[runtime], |call| {
        let value = field(call, table, crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
        let elapsed = sprites::number(call, value)? as f32;
        let value = field(call, table, crate::SONG_LUA_RUNTIME_RATE_KEY)?;
        let rate = sprites::number(call, value)? as f32;
        let origin = store
            .lock()
            .unwrap()
            .clock
            .timing
            .as_ref()
            .map_or(0.0, |clock| clock.get_time_for_beat_exact(0.0));
        Ok(elapsed * rate + origin)
    })
}
fn immediate(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let second = second(call, store)?;
    let owner = pointer(call, actor);
    let start = {
        let mut store = store.lock().unwrap();
        let capture = store.draw_capture.get_or_insert_with(Capture::default);
        capture.context = Some(Context {
            owner,
            ..Context::default()
        });
        capture.calls.len()
    };
    let result = draw(call, store, actor);
    let mut store = store.lock().unwrap();
    let capture = store.draw_capture.as_mut().unwrap();
    capture.context = None;
    let calls = capture.calls.split_off(start);
    if !calls.is_empty() {
        let bytes = size_of::<RawFrame>() + size_of::<usize>();
        if capture.bytes.saturating_add(bytes) > MAX_BYTES {
            return Err(budget(call));
        }
        capture.bytes += bytes;
        capture.frames.push(RawFrame {
            second,
            immediate: true,
            owners: vec![owner],
            calls,
        });
    }
    result
}
fn visit(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    prior: &Context,
) -> Result<(), Error> {
    if live(store) && truthy(get(call, actor, b"__songlua_live_inactive")?) {
        return Ok(());
    }
    if function(call, actor)? != Value::Nil {
        let owner = pointer(call, actor);
        {
            let mut store = store.lock().unwrap();
            let capture = store.draw_capture.get_or_insert_with(Capture::default);
            capture.owners.push(owner);
            capture.context = Some(Context {
                owner,
                ..prior.clone()
            });
        }
        let result = draw(call, store, actor);
        let mut store = store.lock().unwrap();
        let capture = store.draw_capture.as_mut().unwrap();
        capture.context = None;
        if !capture.targets.is_empty() {
            capture.targets.clear();
            return Err(call.error(b"draw callback left a render target active"));
        }
        return result;
    }
    if !store.lock().unwrap().actors.contains_key(&actor)
        && get(call, actor, b"__songlua_actor_type")? == Value::Nil
    {
        let state = initial_state::read(call, store, actor)?;
        let mut parents = prior.parents.to_vec();
        parents.push(state);
        let context = Context {
            owner: prior.owner,
            parents: parents.into(),
            camera: if state.fov.is_some() {
                Some(state)
            } else {
                prior.camera
            },
        };
        return children_each(call, store, actor, |call, store, child| {
            visit(call, store, child, &context)
        });
    }
    let Some(context) = scoped_context(call, store, actor, prior)? else {
        return Ok(());
    };
    children_each(call, store, actor, |call, store, child| {
        visit(call, store, child, &context)
    })
}
fn run(call: &mut HostCall<'_>, store: &Shared, root: TableId) -> Result<(), Error> {
    if live(store)
        && let Some(capture) = &mut store.lock().unwrap().draw_capture
    {
        capture.context = None;
        capture.targets.clear();
        capture.calls.clear();
        capture.owners.clear();
        capture.frames.retain(|frame| frame.immediate);
        if capture.frames.is_empty() {
            capture.labels.clear();
            capture.bytes = 0;
        }
        capture.frame_ops = 0;
    }
    let globals = table(call, helper(call, 5)?)?;
    if !truthy(get(call, globals, DRAW_INSTALLED_KEY.as_bytes())?) {
        if let Some(capture) = &mut store.lock().unwrap().draw_capture {
            capture.frame_ops = 0;
        }
        return Ok(());
    }
    let screen = get(call, globals, b"__songlua_top_screen")?;
    let screen = if screen == Value::Nil {
        None
    } else {
        Some(state(call, store, table(call, screen)?)?)
    };
    let parents = screen.as_ref().map_or_else(
        || Arc::from([]),
        |state| Arc::from(std::slice::from_ref(state)),
    );
    let result = visit(
        call,
        store,
        root,
        &Context {
            parents,
            camera: screen.filter(|state| state.fov.is_some()),
            ..Context::default()
        },
    );
    let second = second(call, store)?;
    if let Some(capture) = &mut store.lock().unwrap().draw_capture {
        let bytes = size_of::<RawFrame>() + capture.owners.len() * size_of::<usize>();
        if capture.bytes.saturating_add(bytes) > MAX_BYTES {
            capture.calls.clear();
            capture.owners.clear();
            return Err(budget(call));
        }
        capture.bytes += bytes;
        let calls = std::mem::take(&mut capture.calls);
        let owners = std::mem::take(&mut capture.owners);
        capture.frames.push(RawFrame {
            second,
            immediate: false,
            owners,
            calls,
        });
        capture.frame_ops = 0;
    }
    result
}
pub(super) fn scene(call: &mut HostCall<'_>, store: &Shared, root: TableId) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = call.try_intern(ACTIVE.as_bytes())?;
    let prior = call.raw_table_get(globals, key)?;
    call.with_roots(&[key, prior], |call| {
        call.raw_table_set(globals, key, Value::Boolean(true))?;
        let result = run(call, store, root);
        call.raw_table_set(globals, key, prior)?;
        result
    })
}
pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    match name {
        "SetDrawFunction" => {
            let function = arg(call, 1);
            if !matches!(function, Value::Nil | Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"SetDrawFunction requires a function or nil"));
            }
            if function != Value::Nil {
                let globals = table(call, helper(call, 5)?)?;
                let key = call.try_intern(DRAW_INSTALLED_KEY.as_bytes())?;
                call.raw_table_set(globals, key, Value::Boolean(true))?;
            }
            set(call, actor, "__songlua_draw_function", function)?;
        }
        "GetDrawFunction" => {
            let value = field(call, actor, "__songlua_draw_function")?;
            if !matches!(value, Value::Nil | Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"actor draw function must be a function"));
            }
            return call.push(value);
        }
        "Draw" => {
            let value = field(call, actor, "__songlua_visible")?;
            if value == Value::Nil || truthy(value) {
                let globals = table(call, helper(call, 5)?)?;
                let active = field(call, globals, ACTIVE)?;
                let screen = assets::field(call, actor, b"__songlua_top_screen_child_name")?;
                let player = screen
                    .as_deref()
                    .is_some_and(|name| matches!(name, "PlayerP1" | "PlayerP2"));
                if truthy(active) && !player {
                    let state = native_children::state(call, store, actor)?;
                    let snapshot = state_snapshot::write(call, store, actor, &state)?;
                    song_replay::raw_set(
                        call,
                        actor,
                        "__songlua_manual_draw_state",
                        Value::Table(snapshot),
                    )?;
                }
                draw(call, store, actor)?;
            }
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}
