//! Load-worker frontend for source-verified native ECS kernels. The executable
//! result lives in runtime.rs and contains only owned Rust state and resources.
//! Source hashes select semantics, never a song title, path or oracle outputs.
mod runtime;
use crate::{SongLuaCompileContext, SongLuaOverlayMeshVertex};
use mlua::{Function, Lua, MultiValue, Table, Value};
use runtime::{Ball, Binding, Entity, Mesh, Physics};
pub use runtime::{CompiledMusicAction, CompiledSceneRestart, CompiledSongScene};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::path::{Path, PathBuf};

fn source_hash(source: &str) -> String {
    let digest = Sha256::digest(source.replace("\r\n", "\n").as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("String formatting");
    }
    out
}

const SOURCES: &[(&str, &str)] = &[
    (
        "fof.lua",
        "dd80ae2c82c2d8483d540dce2d29b2ad1c636726e254f5bcc0e9bd662d970b0f",
    ),
    (
        "ECS.lua",
        "216a95142bbee256a9de0a037b855751e047349e7c4ad43976b5562ffbc57491",
    ),
    (
        "FofGuys.lua",
        "53e602286a99261eb9d4a7dd6b258305d7b5214715eecc4f3d5ab7a23b5fc9e5",
    ),
    (
        "SpinSystem.lua",
        "d0aa9d88355760037a08aa1c698bc6a1bc829520e740660963f0434991b8edc0",
    ),
    (
        "MoveSystem.lua",
        "466ab517b76ff4d0c1e1ce1e3b34727f13c291b71cf9684a06a35e1e9468dc18",
    ),
    (
        "InputSystem.lua",
        "5d7d434c6b2716735d449344e7741c1a47069d5939e90a40fc7cb861e281bfa3",
    ),
    (
        "PhysicsSystem.lua",
        "1b918dbde0a749f045a74233e5539590a57e5794a8e4099affa8afd6e2c96cb1",
    ),
    (
        "FollowSystem.lua",
        "76463185f89bbdfc741bbf3171abe999c5c2dc97488a5d006e0fd0e2f7e09d99",
    ),
    (
        "CheckpointSystem.lua",
        "9062a53f1f3992b083f6216ee56f87189e0717d7ec9256989bb0d2d64fd51064",
    ),
    (
        "GoalSystem.lua",
        "0e418f4d0a76076958f40fecd493960368cb00170bb7247a821be07d1df704ad",
    ),
    (
        "BounceAudioSystem.lua",
        "f0571c5a6c6bb6eb8fac95f8b1a6b500c67581bcda76fa7075e88cd35e2deb41",
    ),
    (
        "RenderSystem.lua",
        "a07cf8b9a0db32d0ee5c304692d6830cb85b9809872b11ad1bcd528554dc6ee3",
    ),
    (
        "Math3D.lua",
        "178a11dd5d82397d86d42d463287e42e0e516df1f9c9e9417c25486c5af21ba1",
    ),
    (
        "Audio.lua",
        "e579e4fd45091864e31d08c80be786bb89ba887117f35b61804d68bda58045ad",
    ),
    (
        "LimitChartDuration.lua",
        "175eb77b15de0d260861bd0c6b92522416e8c4c74bcc0ae23dc1f0e66c620ded",
    ),
];
fn upvalue(lua: &Lua, function: &Function, wanted: &str) -> mlua::Result<Option<Value>> {
    let get = lua
        .globals()
        .get::<Table>("debug")?
        .get::<Function>("getupvalue")?;
    for index in 1..=256 {
        let values = get.call::<MultiValue>((function.clone(), index))?;
        let Some(Value::String(name)) = values.front() else {
            break;
        };
        if name.to_str()?.as_ref() == wanted {
            return Ok(values.get(1).cloned());
        }
    }
    Ok(None)
}
fn find_ecs(lua: &Lua, root: &Table) -> mlua::Result<Option<(Table, PathBuf)>> {
    if let Some(update) = root.raw_get::<Option<Function>>("UpdateCommand")? {
        if let Some(Value::Table(ecs)) = upvalue(lua, &update, "ECS")? {
            if ecs.raw_get::<Option<Table>>("entities")?.is_some() {
                let source = update.info().source.unwrap_or_default();
                let path =
                    crate::files::collapse_actor_path(Path::new(source.trim_start_matches('@')));
                let supported = std::fs::read_to_string(&path).ok().is_some_and(|text| {
                    source_hash(&text)
                        == "53e602286a99261eb9d4a7dd6b258305d7b5214715eecc4f3d5ab7a23b5fc9e5"
                });
                if !supported {
                    // A changed FOF driver must not be mistaken for its
                    // nested countdown-only scene and silently lose the ECS.
                    if let Some(systems) = ecs.raw_get::<Option<Table>>("_systems")? {
                        if let Some(render) = systems.raw_get::<Option<Table>>(9)? {
                            if let Some(function) = render.raw_get::<Option<Function>>("update")? {
                                let source = function.info().source.unwrap_or_default();
                                let path = crate::files::collapse_actor_path(Path::new(
                                    source.trim_start_matches('@'),
                                ));
                                if std::fs::read_to_string(path).ok().is_some_and(|text|
                                    source_hash(&text)=="a07cf8b9a0db32d0ee5c304692d6830cb85b9809872b11ad1bcd528554dc6ee3") {
                                    return Err(mlua::Error::runtime("unsupported ECS driver revision; refusing partial native lowering"));
                                }
                            }
                        }
                    }
                    return Ok(None);
                }
                return Ok(Some((
                    ecs,
                    path.parent().unwrap_or(Path::new("")).to_path_buf(),
                )));
            }
        }
    }
    for child in root.clone().sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            if let Some(found) = find_ecs(lua, &child)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}
fn find_join(root: &Table) -> mlua::Result<Option<(Table, PathBuf)>> {
    if let Some(on) = root.raw_get::<Option<Function>>("OnCommand")? {
        let source = on.info().source.unwrap_or_default();
        let path = crate::files::collapse_actor_path(Path::new(source.trim_start_matches('@')));
        if let Ok(text) = std::fs::read_to_string(&path) {
            if source_hash(&text)
                == "7ce80ee51ad09984a854b1d76ba385ea1aea004fbd4a06cb3abdc132fb248e11"
            {
                return Ok(Some((root.clone(), path.parent().unwrap().to_path_buf())));
            }
        }
    }
    for child in root.clone().sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            if let Some(found) = find_join(&child)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

fn find_auxiliary(root: &Table) -> mlua::Result<Option<(PathBuf, bool)>> {
    for command in ["TickCommand", "EndChartCommand"] {
        if let Some(function) = root.raw_get::<Option<Function>>(command)? {
            let source = function.info().source.unwrap_or_default();
            let path = crate::files::collapse_actor_path(Path::new(source.trim_start_matches('@')));
            if let Ok(text) = std::fs::read_to_string(&path) {
                let hash = source_hash(&text);
                if hash == "175eb77b15de0d260861bd0c6b92522416e8c4c74bcc0ae23dc1f0e66c620ded" {
                    return Ok(Some((path.parent().unwrap().to_path_buf(), false)));
                }
                if hash == "6c7495034c6f17d0216328a769260e70738b4958c651db0825cea392d5ed9550" {
                    return Ok(Some((path.parent().unwrap().to_path_buf(), true)));
                }
            }
        }
    }
    for child in root.clone().sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            if let Some(found) = find_auxiliary(&child)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}
fn validate_sources(dir: &Path) -> Result<(), String> {
    for &(file, expected) in SOURCES {
        let source = std::fs::read_to_string(dir.join(file))
            .map_err(|e| format!("ECS source {file}: {e}"))?;
        let actual = source_hash(&source);
        if actual != expected {
            return Err(format!(
                "unsupported ECS source revision for {file}; refusing native lowering"
            ));
        }
    }
    Ok(())
}
fn has_supported_wrapper(root: &Table) -> mlua::Result<bool> {
    if let Some(function) = root.raw_get::<Option<Function>>("OnCommand")? {
        let source = function.info().source.unwrap_or_default();
        let path = crate::files::collapse_actor_path(Path::new(source.trim_start_matches('@')));
        if std::fs::read_to_string(path)
            .ok()
            .is_some_and(|text| source_hash(&text) == SOURCES[0].1)
        {
            return Ok(true);
        }
    }
    for child in root.clone().sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            if has_supported_wrapper(&child)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn detected(lua: &Lua, root: &Value) -> Result<bool, String> {
    let Value::Table(root) = root else {
        return Ok(false);
    };
    if !has_supported_wrapper(root).map_err(|e| e.to_string())? {
        return Ok(false);
    }
    let found = find_ecs(lua, root)
        .map_err(|e| e.to_string())?
        .or(find_join(root).map_err(|e| e.to_string())?);
    let Some((_, dir)) = found else {
        return Ok(find_auxiliary(root).map_err(|e| e.to_string())?.is_some());
    };
    validate_sources(&dir)?;
    Ok(true)
}
fn array<const N: usize>(table: &Table) -> mlua::Result<[f64; N]> {
    let mut out = [0.0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = table.raw_get(i + 1)?;
    }
    Ok(out)
}
fn component(t: &Table, name: &str) -> mlua::Result<Option<Table>> {
    t.raw_get(name)
}
fn entity(t: &Table) -> mlua::Result<Entity> {
    let transform = t.raw_get::<Table>("transform")?;
    let mut e = Entity {
        position: array(&transform.raw_get::<Table>("position")?)?,
        rotation: component(&transform, "rotation")?
            .map(|t| array(&t))
            .transpose()?,
        spin: component(t, "spin")?.map(|t| array(&t)).transpose()?,
        ..Entity::default()
    };
    if let Some(p) = component(t, "physics")? {
        e.physics = Some(Physics {
            velocity: array(&p.raw_get::<Table>("velocity")?)?,
            gravity: p.raw_get::<Option<bool>>("gravity")?.unwrap_or(false),
            collision: false,
        });
    }
    if let Some(b) = component(t, "ball")? {
        e.ball = Some(Ball {
            radius: b.raw_get("radius")?,
            bounce_height: b.raw_get("bounce_height")?,
            spawn: array(&b.raw_get::<Table>("spawn_position")?)?,
            slippy: b.raw_get::<Option<bool>>("slippy")?.unwrap_or(false),
        });
    }
    if let Some(b) = component(t, "box")? {
        e.half_box = Some(array(&b.raw_get::<Table>("half_size")?)?);
        e.box_slippy = b.raw_get::<Option<bool>>("slippy")?.unwrap_or(false);
    }
    if let Some(m) = component(t, "move")? {
        e.move_index = m.raw_get::<usize>("index")? - 1;
        e.move_progress = m.raw_get("progress")?;
        for stop in m.raw_get::<Table>("stops")?.sequence_values::<Table>() {
            let s = stop?;
            e.moves.push((
                array(&s.raw_get::<Table>("velocity")?)?,
                s.raw_get("duration")?,
            ));
        }
        if e.move_index >= e.moves.len() {
            return Err(mlua::Error::runtime("ECS move stop index outside list"));
        }
    }
    if let Some(i) = component(t, "input")? {
        e.input = Some(i.raw_get::<usize>("player")? - 1);
    }
    if let Some(f) = component(t, "follow")? {
        e.follow = Some((
            f.raw_get::<usize>("target_id")? - 1,
            array(&f.raw_get::<Table>("offset")?)?,
        ));
    }
    if let Some(c) = component(t, "camera")? {
        e.camera = Some(c.raw_get::<usize>("player_index")? - 1);
    }
    if let Some(c) = component(t, "checkpoint")? {
        e.checkpoint = Some(array(&c.raw_get::<Table>("half_size")?)?);
    }
    if let Some(g) = component(t, "goal")? {
        e.goal = Some(array(&g.raw_get::<Table>("half_size")?)?);
    }
    if let Some(m) = component(t, "material")? {
        e.color = component(&m, "color")?
            .map(|t| array::<4>(&t).map(|v| v.map(|n| n as f32)))
            .transpose()?;
        e.translate = component(&m, "translate")?.map(|t| array(&t)).transpose()?;
    }
    if let Some(m) = component(t, "mesh")? {
        if m.raw_get::<String>("draw_mode")? != "DrawMode_Quads" {
            return Err(mlua::Error::runtime("unsupported ECS primitive"));
        }
        let points = m
            .raw_get::<Table>("verts")?
            .sequence_values::<Table>()
            .map(|t| array(&t?))
            .collect::<mlua::Result<Vec<_>>>()?;
        let indices = m
            .raw_get::<Table>("indices")?
            .sequence_values::<usize>()
            .collect::<mlua::Result<Vec<_>>>()?;
        let uv = m
            .raw_get::<Table>("uv")?
            .sequence_values::<Table>()
            .map(|t| array::<2>(&t?).map(|v| v.map(|n| n as f32)))
            .collect::<mlua::Result<Vec<_>>>()?;
        if indices.len() != uv.len()
            || indices.len() % 4 != 0
            || indices.iter().any(|&i| i == 0 || i > points.len())
        {
            return Err(mlua::Error::runtime("invalid ECS mesh topology"));
        }
        let mut corners = Vec::with_capacity(indices.len() / 4 * 6);
        for base in (0..indices.len()).step_by(4) {
            for corner in [0, 1, 2, 2, 3, 0] {
                corners.push((indices[base + corner] - 1, uv[base + corner]));
            }
        }
        e.mesh = Some(Mesh { points, corners });
    }
    Ok(e)
}
pub(crate) fn lower(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    tables: &[Table],
) -> Result<CompiledSongScene, String> {
    let Value::Table(root) = root else {
        return Err("ECS root is not an actor tree".into());
    };
    let ecs = find_ecs(lua, root).map_err(|e| e.to_string())?;
    let join = find_join(root).map_err(|e| e.to_string())?;
    let auxiliary = if ecs.is_none() && join.is_none() {
        find_auxiliary(root).map_err(|e| e.to_string())?
    } else {
        None
    };
    let dir = ecs
        .as_ref()
        .map(|(_, d)| d)
        .or_else(|| join.as_ref().map(|(_, d)| d))
        .or_else(|| auxiliary.as_ref().map(|(d, _)| d))
        .ok_or("missing native scene state")?;
    if auxiliary.is_none() {
        validate_sources(&dir)?;
    }
    let result = (|| -> mlua::Result<CompiledSongScene> {
        let mut program = CompiledSongScene {
            timer_only: auxiliary.as_ref().is_some_and(|(_, bruh)| !*bruh),
            bruh_mode: auxiliary.as_ref().is_some_and(|(_, bruh)| *bruh),
            initial_remaining: 120,
            screen: [
                f64::from(context.screen_width),
                f64::from(context.screen_height),
            ],
            enabled: context.players.each_ref().map(|p| p.enabled),
            music_rate: crate::song_music_rate(context),
            start_second: context.song_timing.as_ref().map_or(0.0, |t| {
                t.get_song_time_for_beat(0.0, crate::song_music_rate(context))
            }),
            random: crate::compat::compiled_random(lua),
            ..Default::default()
        };
        let prefs = lua.globals().get::<Table>("PREFSMAN")?;
        if prefs
            .get::<Function>("GetPreference")?
            .call::<Option<bool>>((prefs.clone(), "RateModsAffectFGChanges"))?
            .unwrap_or(false)
        {
            program.music_rate = 1.0;
        }
        if let Some((ecs, _)) = &ecs {
            for t in ecs.raw_get::<Table>("entities")?.sequence_values::<Table>() {
                program.entities.push(entity(&t?)?);
            }
        }
        if program.entities.len() > 4096 {
            return Err(mlua::Error::runtime("ECS level exceeds 4096 entities"));
        }
        for e in &program.entities {
            if (e.ball.is_some() || !e.moves.is_empty()) && e.physics.is_none() {
                return Err(mlua::Error::runtime("ECS ball/move requires physics"));
            }
            if e.input.is_some_and(|p| p >= 2)
                || e.camera.is_some_and(|p| p >= 2)
                || e.follow.is_some_and(|(target, _)| {
                    program
                        .entities
                        .get(target)
                        .is_none_or(|e| e.ball.is_none())
                })
            {
                return Err(mlua::Error::runtime("invalid ECS player/follow binding"));
            }
        }
        if program
            .entities
            .iter()
            .filter_map(|e| e.mesh.as_ref())
            .map(|m| m.corners.len())
            .sum::<usize>()
            > 1_000_000
        {
            return Err(mlua::Error::runtime(
                "ECS geometry exceeds one million corners",
            ));
        }
        let scene_actors = if let Some((ecs, _)) = &ecs {
            let systems = ecs.raw_get::<Table>("_systems")?;
            let render = systems.raw_get::<Table>(9)?.raw_get::<Function>("update")?;
            let Some(Value::Table(scene_actors)) = upvalue(lua, &render, "scene_actors")? else {
                return Err(mlua::Error::runtime("missing ECS render bindings"));
            };
            Some(scene_actors)
        } else {
            None
        };
        let state = lua.globals().raw_get::<Table>("FOF_GUYS_GLOBAL_STATE")?;
        if program.timer_only {
            program.initial_remaining = state.raw_get("seconds_remaining")?;
        }
        let joined = state.raw_get::<Table>("is_player_joined")?;
        program.joined = [joined.raw_get(1)?, joined.raw_get(2)?];
        if ecs.is_some() {
            for player in 0..2 {
                if program.joined[player]
                    && !program.entities.iter().any(|e| e.camera == Some(player))
                {
                    return Err(mlua::Error::runtime("ECS joined player has no camera"));
                }
            }
        }
        for (i, t) in tables.iter().enumerate() {
            program.states.push(crate::live::read_state(lua, t)?);
            #[cfg(feature = "test-support")]
            program.paths.push(crate::lua_util::message_actor_path(t)?);
            #[cfg(not(feature = "test-support"))]
            program.paths.push(String::new());
            program.meshes.push(None);
            program.texts.push(None);
            if t.raw_get::<Option<Function>>("TickCommand")?.is_some() {
                program.timers.push(i);
            }
            if t.raw_get::<Option<String>>("Name")?.as_deref() == Some("fof/fof.lua")
                || t.raw_get::<Option<f32>>("__songlua_foreground_start_beat")?
                    .is_some()
            {
                program.foregrounds.push(i);
            }
        }
        if let Some((join, _)) = &join {
            program.join_mode = true;
            for p in 0..2 {
                let actor = join.raw_get::<Table>(p + 3)?;
                program.join_players[p] = tables
                    .iter()
                    .position(|t| t.to_pointer() == actor.to_pointer());
            }
        }
        if program.foregrounds.is_empty() {
            // Compiled preparation tags foreground entries separately from
            // actor names; root pointer ownership is independent of identity.
            if let Some(Value::Table(first)) = root.raw_get::<Option<Value>>(1)? {
                if let Some(i) = tables
                    .iter()
                    .position(|t| t.to_pointer() == first.to_pointer())
                {
                    program.foregrounds.push(i);
                }
            }
        }
        for p in 0..2 {
            if !program.joined[p] {
                continue;
            }
            let Some(scene_actors) = &scene_actors else {
                continue;
            };
            let actors = scene_actors.raw_get::<Table>(p + 1)?;
            for (entity, e) in program.entities.iter().enumerate() {
                if e.mesh.is_none() && e.ball.is_none() {
                    continue;
                }
                let actor = actors.raw_get::<Table>(entity + 1)?;
                let i = tables
                    .iter()
                    .position(|t| t.to_pointer() == actor.to_pointer())
                    .ok_or_else(|| {
                        mlua::Error::runtime("ECS actor was dropped during compilation")
                    })?;
                if let Some(mesh) = &e.mesh {
                    program.meshes[i] = Some(
                        mesh.corners
                            .iter()
                            .map(|&(_, uv)| SongLuaOverlayMeshVertex {
                                pos: [0.0; 2],
                                z: 0.0,
                                color: [1.0; 4],
                                uv,
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    );
                } else {
                    let size = [
                        actor
                            .get::<Function>("GetWidth")?
                            .call::<f32>(actor.clone())?,
                        actor
                            .get::<Function>("GetHeight")?
                            .call::<f32>(actor.clone())?,
                    ];
                    program.states[i].size = Some(size);
                }
                program.bindings.push(Binding {
                    player: p,
                    entity,
                    actor: i,
                });
            }
        }
        program.sounds = [
            dir.join("../music.ogg"),
            dir.join("audio/checkpoint.ogg"),
            dir.join("audio/goal.ogg"),
            dir.join("audio/jump1.ogg"),
            dir.join("audio/jump2.ogg"),
            dir.join("audio/jump3.ogg"),
        ];
        if program.join_mode {
            program.sounds[0] = dir.join("audio/join.ogg");
        }
        if program.bruh_mode {
            program.sounds[0] = dir.join("audio/bruh.ogg");
        }
        program.sounds = program
            .sounds
            .map(|p| crate::files::collapse_actor_path(&p));
        program.steps_by_difficulty = context
            .song_steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.steps_type == "StepsType_Dance_Single")
            .map(|(i, s)| (s.difficulty, i))
            .collect();
        program.step_chart_indices = (0..context.song_steps.len())
            .map(|i| {
                if context.song_step_chart_indices.is_empty() {
                    Some(i)
                } else {
                    context.song_step_chart_indices.get(i).copied().flatten()
                }
            })
            .collect();
        program.selected_charts = context
            .player_step_indices
            .map(|i| i.and_then(|i| program.step_chart_indices.get(i).copied().flatten()));
        program.challenge_chart = program
            .steps_by_difficulty
            .iter()
            .find(|(d, _)| *d == crate::SongLuaDifficulty::Challenge)
            .and_then(|(_, i)| program.step_chart_indices[*i]);
        program.initialize();
        Ok(program)
    })();
    result.map_err(|e| format!("native ECS lowering: {e}"))
}

pub(crate) fn resume_context(context: &mut SongLuaCompileContext) {
    let Some(restart) = &context.compiled_scene_restart else {
        return;
    };
    context.player_step_indices = restart.steps;
    for p in 0..2 {
        if let Some(step) = restart.steps[p].and_then(|i| context.song_steps.get(i)) {
            context.players[p].difficulty = step.difficulty;
        }
    }
}
impl SongLuaCompileContext {
    pub fn resume_compiled_scene(&mut self, restart: CompiledSceneRestart) {
        self.compiled_scene_restart = Some(restart);
        resume_context(self);
    }
}
pub(crate) fn install_initial_state(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<()> {
    let Some(restart) = &context.compiled_scene_restart else {
        return Ok(());
    };
    crate::compat::restore_compiled_random(lua, &restart.random);
    let state = lua.create_table()?;
    let joined = lua.create_sequence_from(restart.joined)?;
    state.raw_set("is_player_joined", joined)?;
    state.raw_set("start_time", 0)?;
    state.raw_set("seconds_remaining", restart.remaining)?;
    lua.globals().raw_set("FOF_GUYS_GLOBAL_STATE", state)
}
