//! Custom draw traversal and render-target scopes over the Rust host API.
use super::*;
use crate::draw_commands::{
    Capture, Color, Context, DRAW_INSTALLED_KEY, MAX_BYTES, MAX_OPS, RawCall, RawFrame, RawOp,
};
use sprites::{field, set};

const ACTIVE: &str = "__songlua_draw_callback_active";
const FUNCTION: &[u8] = b"__songlua_draw_function";

fn live(store: &mut Store) -> bool {
    store.clock.live.load(std::sync::atomic::Ordering::Relaxed)
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
fn push(call: &mut HostCall<'_>, op: RawOp) -> Result<(), Error> {
    let live = live(call.store());
    let store = call.store();
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
pub(super) fn begin(call: &mut HostCall<'_>, actor: TableId, preserve: bool) -> Result<(), Error> {
    let pointer = pointer(call, actor);
    push(call, RawOp::Begin(pointer, preserve))?;
    if let Some(capture) = &mut call.store().draw_capture
        && let Some(prior) = capture.context.take()
    {
        let owner = prior.owner;
        let color = prior.color;
        let native_skipped = prior.native_skipped;
        capture.targets.push((pointer, prior));
        capture.context = Some(Context {
            owner,
            // Native BeginRenderingTo resets matrices, not Actor internal colors.
            color,
            native_skipped,
            ..Context::default()
        });
    }
    Ok(())
}
pub(super) fn finish(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let pointer = pointer(call, actor);
    if let Some(capture) = &call.store().draw_capture
        && capture.context.is_some()
        && capture
            .targets
            .last()
            .is_none_or(|(target, _)| *target != pointer)
    {
        return Err(call.error(b"FinishRenderingTo does not match the active target"));
    }
    push(call, RawOp::Finish(pointer))?;
    if let Some(capture) = &mut call.store().draw_capture
        && let Some((_, prior)) = capture.targets.pop()
    {
        capture.context = Some(prior);
    }
    Ok(())
}
fn state(call: &mut HostCall<'_>, actor: TableId) -> Result<crate::SongLuaOverlayState, Error> {
    let actor_id = actor;
    // The native preparation reader resolves an uninitialized Sprite's
    // Texture/AFT getter before reading later scalar fields. Keep this read
    // at draw time; snapshotting raw semantic state must not invoke it early.
    if !live(call.store()) {
        let kind = get_static(call, actor, b"__songlua_actor_type")?;
        let sprite = matches!(kind,Value::String(id)if call.string_bytes(id).is_some_and(|s|s.eq_ignore_ascii_case(b"Sprite")));
        if sprite && get_static(call, actor, b"__songlua_state_sprite_texture")? == Value::Nil {
            return initial_state::read(call, actor);
        }
    }
    let store = call.store_ref();
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
    if state.vibrate {
        state.vibrate_zoom = Some(actor.destination().zoom_x);
    }
    if live {
        effects::render_live_time(call, actor_id, &mut state)?;
        if let Some(actor) = call.store().actors.get(&actor_id) {
            sprite_animation::render(actor, &mut state);
        }
        initial_state::normalize_live_scale(&mut state);
    }
    Ok(state)
}
fn color(
    call: &HostCall<'_>,
    actor: TableId,
    mut state: crate::SongLuaOverlayState,
    inherited: Color,
) -> (Color, bool) {
    if let Some(actor) = call.store_ref().actors.get(&actor) {
        // Bind the actor's current effect accumulator without reading Lua clocks
        // or changing the raw pose retained in the draw payload.
        effects::render_time(actor, [0.0; 2], &mut state);
    }
    let state = crate::playback::song_lua_pre_draw_local_colors(state, [0.0; 2]);
    inherited.pre_draw(state)
}
fn scoped_context(
    call: &mut HostCall<'_>,
    actor: TableId,
    prior: &Context,
) -> Result<Option<Context>, Error> {
    scoped_context_for(call, actor, prior, false, false)
}
fn scoped_context_for(
    call: &mut HostCall<'_>,
    actor: TableId,
    prior: &Context,
    native: bool,
    force_visible: bool,
) -> Result<Option<Context>, Error> {
    let transition = get_static(call, actor, b"__songlua_transition_state")?;
    if transition != Value::Nil {
        let transition = sprites::number(call, transition)?.trunc();
        if !(0.0..256.0).contains(&transition) {
            return Err(call.error(b"transition state out of range"));
        }
        if transition == 0.0 {
            return Ok(None);
        }
    }
    if truthy(get_static(call, actor, b"__songlua_theme_hibernating")?) {
        return Ok(None);
    }
    let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
    let native = native || kind.eq_ignore_ascii_case("PlayerActor");
    if kind.eq_ignore_ascii_case("ActorFrameTexture") {
        let allocation = get_static(call, actor, b"__songlua_aft_allocation")?;
        if allocation == Value::Nil {
            return Ok(None);
        }
        table(call, allocation)?;
    }
    let (wrappers, live) = {
        let store = call.store_ref();
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
    let mut native_skipped = prior.native_skipped;
    // Actor::Draw starts the outermost direct wrapper with fresh internal
    // colors, then replaces the owner's incoming tint with the inner result.
    let mut inherited_color = if wrappers.is_empty() {
        prior.color
    } else {
        Color::default()
    };
    for wrapper in wrappers.into_iter().rev() {
        let wrapper_state = state(call, wrapper)?;
        let (final_color, partially_opaque) = color(call, wrapper, wrapper_state, inherited_color);
        if !wrapper_state.draw_visible() || ((live || native) && !partially_opaque) {
            return Ok(None);
        }
        native_skipped |= !partially_opaque;
        inherited_color = final_color;
        parents.push(wrapper_state);
    }
    let mut state = state(call, actor)?;
    if force_visible {
        state.visible = true;
    }
    let (color, partially_opaque) = color(call, actor, state, inherited_color);
    if !state.draw_visible() || ((live || native) && !partially_opaque) {
        return Ok(None);
    }
    native_skipped |= !partially_opaque;
    parents.push(state);
    Ok(Some(Context {
        owner: prior.owner,
        parents: parents.into(),
        camera: if state.fov.is_some() {
            Some(state)
        } else {
            prior.camera
        },
        color,
        inherited_color,
        native_skipped,
    }))
}

fn native_player_boundary(call: &mut HostCall<'_>, root: TableId) -> Result<Option<usize>, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let Value::Table(top) = get_static(call, globals, b"__songlua_top_screen")? else {
        return Ok(None);
    };
    let store = call.store_ref();
    Ok(store
        .actors
        .get(&top)
        .and_then(|actor| store.scenes.get(&actor.scene))
        .and_then(|scene| scene.song_session.as_ref())
        .filter(|session| session.roots == root)
        .map(|session| session.background_layer_count))
}

fn native_players(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let Value::Table(top) = get_static(call, globals, b"__songlua_top_screen")? else {
        return Ok(());
    };
    let Some(context) = scoped_context_for(call, top, &Context::default(), true, false)? else {
        return Ok(());
    };
    for index in 0..crate::LUA_PLAYERS {
        let Some(actor) = live_feedback::attached_player(call, top, index)? else {
            continue;
        };
        if scoped_context_for(call, actor, &context, true, false)?.is_some() {
            // Player's board pass owns the positioner even when NoteField itself
            // is invisible. Player/wrapper early-abort guards still apply.
            live_feedback::draw_note_field(call, actor)?;
        }
    }
    Ok(())
}

fn native_player_proxy(
    call: &mut HostCall<'_>,
    actor: TableId,
    prior: &Context,
) -> Result<(), Error> {
    if prior.native_skipped {
        return Ok(());
    }
    let Value::Table(target) = get_static(call, actor, b"__songlua_proxy_target_actor")? else {
        return Ok(());
    };
    if !assets::field(call, target, b"__songlua_actor_type")?
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
    {
        return Ok(());
    }
    // Keep generic preparation draw payloads, including transparent proxies.
    // Native publication separately follows strict draw-entry guards, starting
    // from the pre-proxy context so wrappers and colors are applied only once.
    let Some(context) = scoped_context_for(call, actor, prior, true, false)? else {
        return Ok(());
    };
    if scoped_context_for(call, target, &context, true, true)?.is_some() {
        // ActorProxy temporarily forces only the target's own visibility. It
        // leaves wrappers, alpha and hibernation intact, and submits it once.
        live_feedback::draw_note_field(call, target)?;
    }
    Ok(())
}
fn children_each(
    call: &mut HostCall<'_>,
    actor: TableId,
    mut action: impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut index = 1;
    loop {
        let value = call.raw_table_get(actor, Value::Number(index as f64))?;
        if value == Value::Nil {
            return Ok(());
        }
        let child = table(call, value)?;
        call.with_roots(&[value], |call| action(call, child))?;
        index += 1;
    }
}
fn report(call: &mut HostCall<'_>, actor: TableId, error: Error) -> Result<(), Error> {
    diagnostics::report(call, actor, diagnostics::Callback::Draw, error)
}
fn draw(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let prior = call
        .store()
        .draw_capture
        .as_ref()
        .and_then(|capture| capture.context.clone());
    let Some(prior) = prior else {
        return immediate(call, actor);
    };
    let Some(context) = scoped_context(call, actor, &prior)? else {
        return Ok(());
    };
    call.store().draw_capture.as_mut().unwrap().context = Some(context.clone());
    let result = (|| {
        let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
        let target = kind.eq_ignore_ascii_case("ActorFrameTexture");
        if target {
            let preserve = truthy(field(call, actor, "__songlua_state_aft_preserve")?);
            begin(call, actor, preserve)?;
        }
        let result = (|| {
            let callback = function(call, actor)?;
            if callback != Value::Nil {
                // ActorFrame::DrawPrimitives returns after a custom function;
                // its SetInternalDiffuse/Glow child loop has not run.
                call.store()
                    .draw_capture
                    .as_mut()
                    .unwrap()
                    .context
                    .as_mut()
                    .unwrap()
                    .color = Color::default();
                let result = call.with_roots(&[callback], |call| {
                    let sources = call.store().live_sources.is_some();
                    let observed = sources && !theme_callback(call, callback)?;
                    if observed {
                        call.store().song_callback_depth += 1;
                    }
                    let result = run_scoped(call, actor, callback, None);
                    if observed {
                        call.store().song_callback_depth -= 1;
                    }
                    result
                });
                return if live(call.store()) {
                    result.or_else(|error| report(call, actor, error))
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
            if kind.eq_ignore_ascii_case("PlayerActor") && !context.native_skipped {
                live_feedback::draw_note_field(call, actor)?;
            }
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
                let target = get_static(call, actor, b"__songlua_proxy_target_actor")?;
                if target == Value::Nil {
                    return Ok(());
                }
                table(call, target)?;
                native_player_proxy(call, actor, &prior)?;
            }
            if !external
                && (kind.eq_ignore_ascii_case("ActorFrame")
                    || kind.eq_ignore_ascii_case("ActorFrameTexture"))
            {
                return children_each(call, actor, draw);
            }
            let pointer = pointer(call, actor);
            let missing = !call
                .store()
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
                let store = call.store();
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
                RawOp::Draw(pointer, local, Context { parents, ..context }, vertices),
            )
        })();
        if target {
            finish(call, actor)?;
        }
        result
    })();
    call.store().draw_capture.as_mut().unwrap().context = Some(prior);
    result
}
fn second(call: &mut HostCall<'_>) -> Result<f32, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let runtime = field(call, globals, crate::SONG_LUA_RUNTIME_KEY)?;
    let table = table(call, runtime)?;
    call.with_roots(&[runtime], |call| {
        let value = field(call, table, crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
        let elapsed = sprites::number(call, value)? as f32;
        let value = field(call, table, crate::SONG_LUA_RUNTIME_RATE_KEY)?;
        let rate = sprites::number(call, value)? as f32;
        let origin = call
            .store()
            .clock
            .timing
            .as_ref()
            .map_or(0.0, |clock| clock.get_time_for_beat_exact(0.0));
        Ok(elapsed * rate + origin)
    })
}
fn immediate(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let second = second(call)?;
    let owner = pointer(call, actor);
    let start = {
        let store = call.store();
        let capture = store.draw_capture.get_or_insert_with(Capture::default);
        capture.context = Some(Context {
            owner,
            ..Context::default()
        });
        capture.calls.len()
    };
    let result = draw(call, actor);
    let store = call.store();
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
fn visit(call: &mut HostCall<'_>, actor: TableId, prior: &Context) -> Result<(), Error> {
    if live(call.store()) && truthy(get_static(call, actor, b"__songlua_live_inactive")?) {
        return Ok(());
    }
    if function(call, actor)? != Value::Nil {
        let owner = pointer(call, actor);
        {
            let store = call.store();
            let capture = store.draw_capture.get_or_insert_with(Capture::default);
            capture.owners.push(owner);
            capture.context = Some(Context {
                owner,
                ..prior.clone()
            });
        }
        let result = draw(call, actor);
        let store = call.store();
        let capture = store.draw_capture.as_mut().unwrap();
        capture.context = None;
        if !capture.targets.is_empty() {
            capture.targets.clear();
            return Err(call.error(b"draw callback left a render target active"));
        }
        return result;
    }
    if !call.store().actors.contains_key(&actor)
        && get_static(call, actor, b"__songlua_actor_type")? == Value::Nil
    {
        let state = initial_state::read(call, actor)?;
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
            color: prior
                .color
                .pre_draw(crate::playback::song_lua_pre_draw_local_colors(
                    state, [0.0; 2],
                ))
                .0,
            inherited_color: prior.color,
            native_skipped: prior.native_skipped,
        };
        let boundary = native_player_boundary(call, actor)?;
        let mut index = 0;
        let mut positioned = false;
        children_each(call, actor, |call, child| {
            if !positioned && boundary.is_some_and(|boundary| index >= boundary) {
                // ScreenGameplay draws song backgrounds, then Player boards,
                // before drawing its foreground children.
                native_players(call)?;
                positioned = true;
            }
            visit(call, child, &context)?;
            index += 1;
            Ok(())
        })?;
        if boundary.is_some() && !positioned {
            native_players(call)?;
        }
        return Ok(());
    }
    let Some(context) = scoped_context(call, actor, prior)? else {
        return Ok(());
    };
    if assets::field(call, actor, b"__songlua_actor_type")?
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorProxy"))
    {
        native_player_proxy(call, actor, prior)?;
    }
    children_each(call, actor, |call, child| visit(call, child, &context))
}
fn run(call: &mut HostCall<'_>, root: TableId) -> Result<(), Error> {
    if live(call.store())
        && let Some(capture) = &mut call.store().draw_capture
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
    let helpers = table(call, call.captures()[1])?;
    let custom = truthy(get(call, globals, DRAW_INSTALLED_KEY.as_bytes())?);
    if !custom
        && !truthy(get_static(
            call,
            helpers,
            proxies::PLAYER_DRAW_KEY.as_bytes(),
        )?)
    {
        if native_player_boundary(call, root)?.is_some() {
            native_players(call)?;
        }
        if let Some(capture) = &mut call.store().draw_capture {
            capture.frame_ops = 0;
        }
        return Ok(());
    }
    let screen = get_static(call, globals, b"__songlua_top_screen")?;
    let (screen, color, native_skipped) = if screen == Value::Nil {
        (None, Color::default(), false)
    } else {
        let actor = table(call, screen)?;
        let state = state(call, actor)?;
        let color = color(call, actor, state, Color::default()).0;
        // Capture keeps its existing screen transform/color. Native side
        // effects also obey ScreenGameplay's outer wrapper entry guards.
        let native_skipped =
            scoped_context_for(call, actor, &Context::default(), true, false)?.is_none();
        (Some(state), color, native_skipped)
    };
    let parents = screen.as_ref().map_or_else(
        || Arc::from([]),
        |state| Arc::from(std::slice::from_ref(state)),
    );
    let result = visit(
        call,
        root,
        &Context {
            parents,
            camera: screen.filter(|state| state.fov.is_some()),
            color,
            native_skipped,
            ..Context::default()
        },
    );
    if !custom {
        return result;
    }
    let second = second(call)?;
    if let Some(capture) = &mut call.store().draw_capture {
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
pub(super) fn scene(call: &mut HostCall<'_>, root: TableId) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = call.try_intern(ACTIVE.as_bytes())?;
    let prior = call.raw_table_get(globals, key)?;
    call.with_roots(&[key, prior], |call| {
        call.raw_table_set(globals, key, Value::Boolean(true))?;
        let result = run(call, root);
        call.raw_table_set(globals, key, prior)?;
        result
    })
}
pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
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
                    let state = native_children::state(call, actor)?;
                    let snapshot = state_snapshot::write(call, actor, &state)?;
                    song_replay::raw_set(
                        call,
                        actor,
                        "__songlua_manual_draw_state",
                        Value::Table(snapshot),
                    )?;
                }
                draw(call, actor)?;
            }
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}
