//! Bake custom ActorFrame draw stacks without retaining mutable Lua tables.

use std::sync::Arc;

#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Function, Lua, Table};
use rustc_hash::FxHashMap;

#[cfg(any(test, feature = "legacy-oracle"))]
use crate::lua_util::{
    SongLuaOverlayCompileActor, actor_overlay_initial_state, call_actor_function,
    drain_actor_command_queue, read_actor_multi_vertex_mesh,
};
use crate::SongLuaOverlayMeshVertex;
use crate::SongLuaOverlayState;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::SongLuaTrackedActor;
use crate::SongLuaTrackedActorTarget;

pub use crate::draw_commands::{DrawFrame, DrawOp, DrawSource};
use crate::draw_commands::{Capture, Context, RawCall, RawFrame, RawOp, MAX_BYTES, MAX_OPS, convert, partially_opaque};
pub(crate) use crate::draw_commands::DRAW_INSTALLED_KEY;

#[cfg(any(test, feature = "legacy-oracle"))]
fn push(lua: &Lua, op: RawOp) -> mlua::Result<()> {
    let Some(mut capture) = lua.app_data_mut::<Capture>() else {
        return Ok(());
    };
    let Some(context) = &capture.context else {
        return Ok(());
    };
    let owner = context.owner;
    let extra = match &op {
        RawOp::Draw(_, _, context, vertices) => {
            context.parents.len() * std::mem::size_of::<SongLuaOverlayState>()
                + vertices.as_ref().map_or(0, |vertices| {
                    vertices.len() * std::mem::size_of::<SongLuaOverlayMeshVertex>()
                })
        }
        _ => 0,
    };
    let bytes = std::mem::size_of::<RawCall>() + extra;
    // Baked whole-song payloads retain a per-frame operation bound. A live
    // replay discards the previous frame and uses the byte bound instead:
    // native actors have no 256-Draw limit (HOPE submits 355 explicit calls).
    if (!crate::live::active(lua) && capture.frame_ops >= MAX_OPS)
        || capture.bytes.saturating_add(bytes) > MAX_BYTES
    {
        return Err(mlua::Error::external(
            "custom draw capture exceeds its song budget",
        ));
    }
    capture.bytes += bytes;
    capture.frame_ops += 1;
    capture.calls.push(RawCall { owner, op });
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn begin(lua: &Lua, actor: &Table, preserve: bool) -> mlua::Result<()> {
    let pointer = actor.to_pointer() as usize;
    push(lua, RawOp::Begin(pointer, preserve))?;
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        if let Some(prior) = capture.context.take() {
            let owner = prior.owner;
            capture.targets.push((pointer, prior));
            capture.context = Some(Context {
                owner,
                ..Context::default()
            });
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn finish(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let pointer = actor.to_pointer() as usize;
    if let Some(capture) = lua.app_data_ref::<Capture>() {
        if capture.context.is_some()
            && capture
                .targets
                .last()
                .is_none_or(|(target, _)| *target != pointer)
        {
            return Err(mlua::Error::external(
                "FinishRenderingTo does not match the active target",
            ));
        }
    }
    push(lua, RawOp::Finish(pointer))?;
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        if let Some((_, prior)) = capture.targets.pop() {
            capture.context = Some(prior);
        }
    }
    Ok(())
}

struct IsSprite(bool);

#[cfg(any(test, feature = "legacy-oracle"))]
impl mlua::FromLua for IsSprite {
    fn from_lua(value: mlua::Value, _: &Lua) -> mlua::Result<Self> {
        Ok(Self(matches!(value, mlua::Value::String(kind)
            if kind.as_bytes().eq_ignore_ascii_case(b"Sprite"))))
    }

    unsafe fn from_stack(index: std::os::raw::c_int, raw: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: mlua supplies a live stack slot under its VM lock. Borrow
        // only an actual Lua string; no coercion, allocation or pointer escapes.
        // Non-strings and arbitrary binary strings retain Value's predicate.
        unsafe {
            let state = raw.state();
            if mlua::ffi::lua_type(state, index) != mlua::ffi::LUA_TSTRING {
                return Ok(Self(false));
            }
            let mut len = 0;
            let data = mlua::ffi::lua_tolstring(state, index, &mut len);
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
            Ok(Self(bytes.eq_ignore_ascii_case(b"Sprite")))
        }
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn state(lua: &Lua, actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    if crate::live::active(lua) {
        crate::live::read_state(lua, actor)
    } else {
        // An uninitialized Sprite may resolve Texture/AFT through authored
        // __index code before later scalar fields are read. Keep those reads
        // sequential so mutations and errors from that lookup stay observable.
        let sprite = actor.raw_get::<IsSprite>("__songlua_actor_type")?.0;
        if sprite && actor.raw_get::<mlua::Value>("__songlua_state_sprite_texture")?.is_nil() {
            actor_overlay_initial_state(actor).map_err(mlua::Error::external)
        } else {
            crate::lua_util::actor_overlay_state_batched(lua, actor).map_err(mlua::Error::external)
        }
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn scoped_context(lua: &Lua, actor: &Table, prior: &Context) -> mlua::Result<Option<Context>> {
    // Transition::EarlyAbortDraw: waiting transitions are not drawable, even
    // if their child has an opaque queued destination state.
    if actor.raw_get::<Option<u8>>("__songlua_transition_state")? == Some(0) {
        return Ok(None);
    }
    if actor
        .raw_get::<Option<bool>>("__songlua_theme_hibernating")?
        .unwrap_or(false)
    {
        return Ok(None);
    }
    if crate::lua_util::actor_type_is(actor, "ActorFrameTexture")?
        && actor
            .raw_get::<Option<Table>>("__songlua_aft_allocation")?
            .is_none()
    {
        return Ok(None);
    }
    let wrappers = actor.raw_get::<Option<Table>>("__songlua_wrappers")?;
    let mut parents = Vec::with_capacity(
        prior.parents.len() + wrappers.as_ref().map_or(0, Table::raw_len) + 1,
    );
    parents.extend_from_slice(&prior.parents);
    if let Some(wrappers) = wrappers {
        for index in (1..=wrappers.raw_len()).rev() {
            let wrapper = state(lua, &wrappers.raw_get::<Table>(index)?)?;
            if !wrapper.draw_visible()
                || (crate::live::active(lua) && !partially_opaque(wrapper, &parents)) {
                return Ok(None);
            }
            parents.push(wrapper);
        }
    }
    let state = state(lua, actor)?;
    if !state.draw_visible()
        || (crate::live::active(lua) && !partially_opaque(state, &parents)) {
        return Ok(None);
    }
    parents.push(state);
    let camera = if state.fov.is_some() {
        Some(state)
    } else {
        prior.camera
    };
    Ok(Some(Context {
        owner: prior.owner,
        parents: parents.into(),
        camera,
    }))
}

#[cfg(any(test, feature = "legacy-oracle"))]
/// Execute an explicit Actor:Draw using the caller's frozen draw context.
pub(crate) fn draw(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let prior = lua
        .app_data_ref::<Capture>()
        .and_then(|capture| capture.context.clone());
    let Some(prior) = prior else { return draw_immediate(lua, actor); };
    let Some(context) = scoped_context(lua, actor, &prior)? else {
        return Ok(());
    };
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        capture.context = Some(context.clone());
    }
    let result = (|| {
        let target = crate::lua_util::actor_type_is(actor, "ActorFrameTexture")?;
        if target {
            begin(
                lua,
                actor,
                actor
                    .get::<Option<bool>>("__songlua_state_aft_preserve")?
                    .unwrap_or(false),
            )?;
        }
        let result = (|| {
            if let Some(callback) = actor.raw_get::<Option<Function>>("__songlua_draw_function")? {
                crate::live::callback_phase(lua, true);
                let result = call_actor_function(lua, actor, &callback, None);
                crate::live::callback_phase(lua, false);
                if crate::live::active(lua) {
                    if let Err(error) = result {
                        crate::lua_util::report_update_error(lua, actor,
                            "__songlua_draw_error_reported", "draw callback", &error)?;
                    }
                    // Native draw callbacks do not update the actor queue.
                    // queuecommand from Draw waits for the next actor Update.
                    Ok(())
                } else {
                    result?;
                    drain_actor_command_queue(lua, actor)
                }
            } else {
                let kind = actor
                    .get::<Option<crate::lua_util::LuaFieldText<32>>>("__songlua_actor_type")?;
                let kind = kind.as_ref().map_or("", |kind| kind.as_str());
                // Only native frames backed by a compositor source are opaque
                // leaves. Other screen frames use ActorFrame::DrawPrimitives:
                // traverse their children, including empty transition frames.
                // Keep this set aligned with read_tracked_compile_actors.
                let screen_child = actor.raw_get::<Option<crate::lua_util::LuaFieldText<64>>>(
                    "__songlua_top_screen_child_name")?;
                let player_child = actor.raw_get::<Option<crate::lua_util::LuaFieldText<64>>>(
                    "__songlua_player_child_name")?;
                let external = matches!(screen_child.as_ref().map(|name| name.as_str()),
                    Some("Underlay" | "Overlay" | "SongBackground" | "SongForeground" | "In"))
                    || matches!(player_child.as_ref().map(|name| name.as_str()),
                        Some("Judgment" | "Combo"));
                // Actor::DrawPrimitives is empty. Logic actors still receive
                // explicit Draw calls, but submit no geometry or draw payload.
                if kind.eq_ignore_ascii_case("Actor") {
                    return Ok(());
                }
                // BitmapText::EarlyAbortDraw skips empty text. Native screen
                // placeholders are not compiled overlay leaves either.
                if kind.eq_ignore_ascii_case("BitmapText")
                    && actor.raw_get::<Option<mlua::LuaString>>("Text")?
                        .is_none_or(|text| text.as_bytes().is_empty())
                {
                    return Ok(());
                }
                let local = *context.parents.last().expect("draw context includes the actor");
                // The bundled fallback bitmap contains only transparent pixels.
                // Its ordinary alpha draw cannot change color, depth or masks.
                if kind.eq_ignore_ascii_case("Sprite")
                    && actor.raw_get::<Option<mlua::LuaString>>("Texture")?
                        .is_some_and(|path| path.as_bytes().as_ref()
                            == crate::lua_util::NATIVE_BLANK_TEXTURE_PATH.as_bytes())
                    && local.blend == crate::SongLuaOverlayBlendMode::Alpha
                    && !local.depth_write && !local.mask_source && !local.mask_dest
                {
                    return Ok(());
                }
                // ActorProxy::DrawPrimitives submits nothing until SetTarget.
                // A parent's early Draw must not borrow a later On target.
                if !external && kind.eq_ignore_ascii_case("ActorProxy")
                    && actor.raw_get::<Option<Table>>("__songlua_proxy_target_actor")?.is_none()
                {
                    return Ok(());
                }
                if !external
                    && (kind.eq_ignore_ascii_case("ActorFrame")
                        || kind.eq_ignore_ascii_case("ActorFrameTexture"))
                {
                    for child in actor.sequence_values::<Table>() {
                        draw(lua, &child?)?;
                    }
                    Ok(())
                } else {
                    let pointer = actor.to_pointer() as usize;
                    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
                        if !capture.labels.contains_key(&pointer) {
                            let name = actor.raw_get::<Option<String>>("Name")?.unwrap_or_default();
                            let child = actor
                                .raw_get::<Option<String>>("__songlua_top_screen_child_name")?
                                .unwrap_or_default();
                            let label = format!("{kind} '{name}' (screen child '{child}')");
                            let bytes = label.len() + std::mem::size_of::<(usize, String)>();
                            if capture.bytes.saturating_add(bytes) > MAX_BYTES {
                                return Err(mlua::Error::external(
                                    "custom draw capture exceeds its song budget",
                                ));
                            }
                            capture.bytes += bytes;
                            capture.labels.insert(pointer, label);
                        }
                    }
                    let vertices =
                        read_actor_multi_vertex_mesh(actor).map_err(mlua::Error::external)?;
                    // The leaf is built separately; wrappers remain in its caller stack.
                    let parents = Arc::from(&context.parents[..context.parents.len() - 1]);
                    push(
                        lua,
                        RawOp::Draw(
                            pointer,
                            local,
                            Context {
                                parents,
                                ..context
                            },
                            vertices,
                        ),
                    )
                }
            }
        })();
        if target {
            finish(lua, actor)?;
        }
        result
    })();
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        capture.context = Some(prior);
    }
    result
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn frame_second(lua: &Lua) -> mlua::Result<f32> {
    let runtime = lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
    let elapsed = runtime.get::<f32>(crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
    let rate = runtime.get::<f32>(crate::SONG_LUA_RUNTIME_RATE_KEY)?;
    let origin = lua.app_data_ref::<crate::runtime::SongLuaClock>()
        .map_or(0.0, |clock| clock.0.get_time_for_beat_exact(0.0));
    Ok(elapsed * rate + origin)
}

// Lua Actor:Draw executes immediately, including during a parent's OnCommand
// before its children's OnCommands. Freeze that submission at the call site;
// using the eventual overlay pose loses its size, crop, color and draw stack.
#[cfg(any(test, feature = "legacy-oracle"))]
fn draw_immediate(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let second = frame_second(lua)?;
    if lua.app_data_ref::<Capture>().is_none() { lua.set_app_data(Capture::default()); }
    let owner = actor.to_pointer() as usize;
    let start = {
        let mut capture = lua.app_data_mut::<Capture>().expect("capture installed");
        capture.context = Some(Context { owner, ..Context::default() });
        capture.calls.len()
    };
    let result = draw(lua, actor);
    let mut capture = lua.app_data_mut::<Capture>().expect("capture retained during Draw");
    capture.context = None;
    let calls = capture.calls.split_off(start);
    if !calls.is_empty() {
        let bytes = std::mem::size_of::<RawFrame>() + std::mem::size_of::<usize>();
        if capture.bytes.saturating_add(bytes) > MAX_BYTES {
            return Err(mlua::Error::external("custom draw capture exceeds its song budget"));
        }
        capture.bytes += bytes;
        capture.frames.push(RawFrame { second, immediate: true, owners: vec![owner], calls });
    }
    result
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn visit(lua: &Lua, actor: &Table, prior: &Context) -> mlua::Result<()> {
    if crate::live::active(lua) && actor.raw_get::<Option<bool>>("__songlua_live_inactive")?.unwrap_or(false) {
        return Ok(());
    }
    let custom = actor
        .raw_get::<Option<Function>>("__songlua_draw_function")?
        .is_some();
    if custom {
        if lua.app_data_ref::<Capture>().is_none() {
            lua.set_app_data(Capture::default());
        }
        if let Some(mut capture) = lua.app_data_mut::<Capture>() {
            capture.owners.push(actor.to_pointer() as usize);
            capture.context = Some(Context {
                owner: actor.to_pointer() as usize,
                ..prior.clone()
            });
        }
        let result = draw(lua, actor);
        if let Some(mut capture) = lua.app_data_mut::<Capture>() {
            capture.context = None;
            if !capture.targets.is_empty() {
                capture.targets.clear();
                return Err(mlua::Error::external(
                    "draw callback left a render target active",
                ));
            }
        }
        result?;
        return Ok(());
    }
    let Some(context) = scoped_context(lua, actor, prior)? else {
        return Ok(());
    };
    for child in actor.sequence_values::<Table>() {
        visit(lua, &child?, &context)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
/// The live VM retains only this frame's draw stack. Reuse the same owned
/// traversal and explicit Draw/render-target scopes as the baker, while
/// releasing prior frame payloads before any callback runs. Trace events and
/// pending actor queues are separate and are never cleared here.
pub(crate) fn run_live(lua: &Lua, root: &Table) -> mlua::Result<()> {
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
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
    run(lua, root)
}

#[cfg(test)]
pub(crate) fn live_storage(lua: &Lua) -> (usize, usize, usize) {
    lua.app_data_ref::<Capture>().map_or((0, 0, 0), |capture|
        (capture.frames.len(), capture.frames.iter().map(|frame| frame.calls.len()).sum(), capture.bytes))
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn run(lua: &Lua, root: &Table) -> mlua::Result<()> {
    // Monotone across callback removal: existing frames still need quiet tails.
    // Ordinary charts avoid traversing and reading every actor on each update.
    if !lua
        .globals()
        .raw_get::<Option<bool>>(DRAW_INSTALLED_KEY)?
        .unwrap_or(false)
    {
        if let Some(mut capture) = lua.app_data_mut::<Capture>() { capture.frame_ops = 0; }
        return Ok(());
    }
    let screen = lua
        .globals()
        .raw_get::<Option<Table>>("__songlua_top_screen")?;
    let screen = screen.as_ref().map(|actor| state(lua, actor)).transpose()?;
    let parents = screen.as_ref().map_or_else(
        || Arc::from([]),
        |state| Arc::from(std::slice::from_ref(state)),
    );
    let result = visit(
        lua,
        root,
        &Context {
            parents,
            camera: screen.filter(|state| state.fov.is_some()),
            ..Context::default()
        },
    );
    let second = frame_second(lua)?;
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        let bytes =
            std::mem::size_of::<RawFrame>() + capture.owners.len() * std::mem::size_of::<usize>();
        if capture.bytes.saturating_add(bytes) > MAX_BYTES {
            capture.calls.clear();
            capture.owners.clear();
            return Err(mlua::Error::external(
                "custom draw capture exceeds its song budget",
            ));
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

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn take<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    tracked: &[SongLuaTrackedActor],
) -> Result<Vec<(usize, DrawFrame)>, String> {
    let Some(capture) = lua.remove_app_data::<Capture>() else {
        return Ok(Vec::new());
    };
    let mut sources = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| {
            (
                overlay.table.to_pointer() as usize,
                DrawSource::Overlay(index),
            )
        })
        .collect::<FxHashMap<_, _>>();
    for actor in tracked {
        let source = match actor.target {
            SongLuaTrackedActorTarget::Player(index) => DrawSource::Player(index),
            SongLuaTrackedActorTarget::PlayerJudgment(index) => DrawSource::Judgment(index),
            SongLuaTrackedActorTarget::PlayerCombo(index) => DrawSource::Combo(index),
            SongLuaTrackedActorTarget::SongForeground => DrawSource::SongForeground,
            SongLuaTrackedActorTarget::ScreenLayer(index) => DrawSource::ScreenLayer(index),
        };
        sources.insert(actor.table.to_pointer() as usize, source);
    }
    convert(capture.frames, &capture.labels, &sources)
}

pub(crate) struct LiveSources(pub FxHashMap<usize, DrawSource>);

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn take_live(lua: &Lua) -> Result<Vec<DrawFrame>, String> {
    if lua.app_data_ref::<Capture>().is_none_or(|capture|
        !capture.frames.iter().any(|frame| frame.immediate) || capture.context.is_some()) {
        return Ok(Vec::new());
    }
    if lua.app_data_ref::<LiveSources>().is_none() { return Ok(Vec::new()); }
    let mut capture = lua.remove_app_data::<Capture>().expect("complete capture frames");
    let immediate = capture.frames.extract_if(.., |frame| frame.immediate).collect();
    // Conversion reads labels only for errors and invokes no Lua callbacks.
    // Borrow them before restoring capture, including when conversion fails.
    let result = {
        let sources = lua.app_data_ref::<LiveSources>().expect("live source bindings");
        convert(immediate, &capture.labels, &sources.0)
    };
    if capture.frames.is_empty() && capture.calls.is_empty() && capture.targets.is_empty() {
        capture.bytes = 0;
        capture.frame_ops = 0;
        capture.labels.clear();
    }
    lua.set_app_data(capture);
    result.map(|frames| frames.into_iter().map(|(_, frame)| frame).collect())
}


#[cfg(test)]
pub(crate) mod perf {
    include!("../tests/perf/draw_capture.rs");
}

#[cfg(test)]
#[path = "../tests/perf/live_extract.rs"]
mod live_extract_perf;
