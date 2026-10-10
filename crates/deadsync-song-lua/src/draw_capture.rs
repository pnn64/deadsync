//! Bake custom ActorFrame draw stacks without retaining mutable Lua tables.

use std::sync::Arc;

use mlua::{Function, Lua, Table};
use rustc_hash::FxHashMap;

use crate::lua_util::{
    SongLuaOverlayCompileActor, actor_overlay_initial_state, call_actor_function,
    drain_actor_command_queue, read_actor_multi_vertex_mesh,
};
use crate::{
    SongLuaOverlayMeshVertex, SongLuaOverlayState, SongLuaTrackedActor, SongLuaTrackedActorTarget,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawSource {
    Overlay(usize),
    Player(usize),
    Judgment(usize),
    Combo(usize),
    SongForeground,
    ScreenLayer(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    Begin {
        capture: usize,
        preserve: bool,
    },
    Finish {
        capture: usize,
    },
    Draw {
        source: DrawSource,
        state: SongLuaOverlayState,
        /// Caller draw stack, outermost first; BeginRenderingTo resets it.
        parents: Arc<[SongLuaOverlayState]>,
        camera: Option<SongLuaOverlayState>,
        vertices: Option<Arc<[SongLuaOverlayMeshVertex]>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawFrame {
    pub second: f32,
    pub owners: Vec<usize>,
    /// Exclusive operation end for each owner, in the same order as `owners`.
    pub owner_ends: Vec<usize>,
    pub ops: Vec<DrawOp>,
}

#[derive(Clone, Default)]
struct Context {
    owner: usize,
    parents: Arc<[SongLuaOverlayState]>,
    camera: Option<SongLuaOverlayState>,
}

enum RawOp {
    Begin(usize, bool),
    Finish(usize),
    Draw(
        usize,
        SongLuaOverlayState,
        Context,
        Option<Arc<[SongLuaOverlayMeshVertex]>>,
    ),
}

struct RawCall {
    owner: usize,
    op: RawOp,
}

struct RawFrame {
    second: f32,
    owners: Vec<usize>,
    calls: Vec<RawCall>,
}

/// Song-load worker only, no synchronization or gameplay maintenance. Retains
/// immutable frames until compilation hands them to song-lifetime playback.
/// At most 256 operations/frame and 256 MiB/song, including vertex payloads
/// and caller stacks. Overflow reports unsupported capture; it never prunes or
/// silently reuses the final pass. No I/O or GPU work occurs here.
#[derive(Default)]
struct Capture {
    context: Option<Context>,
    targets: Vec<(usize, Context)>,
    calls: Vec<RawCall>,
    owners: Vec<usize>,
    frames: Vec<RawFrame>,
    labels: FxHashMap<usize, String>,
    bytes: usize,
}

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_OPS: usize = 256;
pub(crate) const DRAW_INSTALLED_KEY: &str = "__songlua_draw_installed";

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
    if capture.calls.len() >= MAX_OPS || capture.bytes.saturating_add(bytes) > MAX_BYTES {
        return Err(mlua::Error::external(
            "custom draw capture exceeds its song budget",
        ));
    }
    capture.bytes += bytes;
    capture.calls.push(RawCall { owner, op });
    Ok(())
}

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

fn state(actor: &Table) -> mlua::Result<SongLuaOverlayState> {
    actor_overlay_initial_state(actor).map_err(mlua::Error::external)
}

fn scoped_context(actor: &Table, prior: &Context) -> mlua::Result<Option<Context>> {
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
    let mut parents = prior.parents.to_vec();
    if let Some(wrappers) = actor.raw_get::<Option<Table>>("__songlua_wrappers")? {
        for index in (1..=wrappers.raw_len()).rev() {
            let wrapper = state(&wrappers.raw_get::<Table>(index)?)?;
            if !wrapper.draw_visible() {
                return Ok(None);
            }
            parents.push(wrapper);
        }
    }
    let state = state(actor)?;
    if !state.draw_visible() {
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

/// Execute an explicit Actor:Draw using the caller's frozen draw context.
pub(crate) fn draw(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let prior = lua
        .app_data_ref::<Capture>()
        .and_then(|capture| capture.context.clone());
    let Some(prior) = prior else { return Ok(()) };
    let Some(context) = scoped_context(actor, &prior)? else {
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
                call_actor_function(lua, actor, &callback, None)?;
                drain_actor_command_queue(lua, actor)
            } else {
                let kind = actor
                    .get::<Option<String>>("__songlua_actor_type")?
                    .unwrap_or_default();
                let external = actor
                    .raw_get::<Option<String>>("__songlua_top_screen_child_name")?
                    .is_some()
                    || actor.raw_get::<Option<String>>("__songlua_player_child_name")?.is_some();
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
                    let local = *context
                        .parents
                        .last()
                        .expect("draw context includes the actor");
                    // The leaf is built separately; wrappers remain in its caller stack.
                    let mut parents = context.parents.to_vec();
                    parents.pop();
                    push(
                        lua,
                        RawOp::Draw(
                            pointer,
                            local,
                            Context {
                                parents: parents.into(),
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

fn visit(lua: &Lua, actor: &Table, prior: &Context) -> mlua::Result<()> {
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
    let Some(context) = scoped_context(actor, prior)? else {
        return Ok(());
    };
    for child in actor.sequence_values::<Table>() {
        visit(lua, &child?, &context)?;
    }
    Ok(())
}

pub(crate) fn run(lua: &Lua, root: &Table) -> mlua::Result<()> {
    // Monotone across callback removal: existing frames still need quiet tails.
    // Ordinary charts avoid traversing and reading every actor on each update.
    if !lua
        .globals()
        .raw_get::<Option<bool>>(DRAW_INSTALLED_KEY)?
        .unwrap_or(false)
    {
        return Ok(());
    }
    let screen = lua
        .globals()
        .raw_get::<Option<Table>>("__songlua_top_screen")?;
    let screen = screen.as_ref().map(state).transpose()?;
    let parents = screen.into_iter().collect::<Vec<_>>().into();
    let result = visit(
        lua,
        root,
        &Context {
            parents,
            camera: screen.filter(|state| state.fov.is_some()),
            ..Context::default()
        },
    );
    let runtime = lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
    let elapsed = runtime.get::<f32>(crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
    let rate = runtime.get::<f32>(crate::SONG_LUA_RUNTIME_RATE_KEY)?;
    let origin = lua
        .app_data_ref::<crate::runtime::SongLuaClock>()
        .map_or(0.0, |clock| clock.0.get_time_for_beat_exact(0.0));
    let second = elapsed * rate + origin;
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
            owners,
            calls,
        });
    }
    result
}

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
    let overlay_index = |pointer| match sources.get(&pointer) {
        Some(DrawSource::Overlay(index)) => Ok(*index),
        _ => Err("custom draw references an uncompiled capture or callback owner".to_owned()),
    };
    let mut owners = Vec::new();
    for frame in &capture.frames {
        for &owner in &frame.owners {
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
    }
    let mut frames = Vec::new();
    for raw in capture.frames {
        let mut frame_owners = raw.owners;
        for &owner in &owners {
            if !frame_owners.contains(&owner) {
                frame_owners.push(owner);
            }
        }
        let mut by_owner = frame_owners
            .into_iter()
            .map(|pointer| {
                let owner = overlay_index(pointer)?;
                Ok((
                    owner,
                    DrawFrame {
                        second: raw.second,
                        owners: vec![owner],
                        owner_ends: Vec::new(),
                        ops: Vec::new(),
                    },
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for call in raw.calls {
            let owner = overlay_index(call.owner)?;
            let op = match call.op {
                RawOp::Begin(pointer, preserve) => DrawOp::Begin {
                    capture: overlay_index(pointer)?,
                    preserve,
                },
                RawOp::Finish(pointer) => DrawOp::Finish {
                    capture: overlay_index(pointer)?,
                },
                RawOp::Draw(pointer, state, context, vertices) => DrawOp::Draw {
                    source: *sources.get(&pointer).ok_or_else(|| {
                        format!(
                            "custom draw references an uncompiled actor {} at {:.6}s",
                            capture
                                .labels
                                .get(&pointer)
                                .map_or("<unnamed>", String::as_str),
                            raw.second,
                        )
                    })?,
                    state,
                    parents: context.parents,
                    camera: context.camera,
                    vertices,
                },
            };
            if let Some((_, frame)) = by_owner.iter_mut().find(|(index, _)| *index == owner) {
                frame.ops.push(op);
            } else {
                return Err("custom draw lost its callback owner".to_owned());
            }
        }
        for (_, frame) in &mut by_owner {
            frame.owner_ends.push(frame.ops.len());
        }
        frames.extend(by_owner);
    }
    Ok(frames)
}
