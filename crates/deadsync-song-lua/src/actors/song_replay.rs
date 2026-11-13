//! Restore the compiler's chronological queue after speculative discovery.
use super::*;
use crate::{SongLuaOverlayState, SongStartupState};

// Before the first positive update, preparation getters still expose the
// exported frame-zero fields, while tween interpolation owns a different tail.
#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) getters: Option<TweenPose>,
    pub(super) progress: Vec<Vec<[f32; 2]>>,
    pub(super) targets: u128,
    pub(super) capture_open: bool,
}

#[derive(Clone)]
struct Setup {
    scene: u64,
    overlays: Vec<(TableId, SongLuaOverlayState)>,
    startup: HashMap<TableId, SongStartupState>,
    spin: HashMap<TableId, [f32; 3]>,
}

impl Actors {
    pub(crate) fn begin_song_replay(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        overlays: &[(TableId, SongLuaOverlayState)],
        startup: &HashMap<TableId, SongStartupState>,
    ) -> Result<(), Error> {
        let invalid = {
            let store = vm.store();
            if store.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
                Some(b"song preparation replay is unavailable in live mode".as_slice())
            } else if store.playback {
                Some(b"song replay has already started".as_slice())
            } else if !store.scenes[&screen.id]
                .song_session
                .as_ref()
                .is_some_and(|session| session.activated && session.discovery_started)
            {
                Some(b"song replay requires startup discovery".as_slice())
            } else if overlays
                .iter()
                .map(|(id, _)| id)
                .chain(startup.keys())
                .any(|id| {
                    store
                        .actors
                        .get(id)
                        .is_none_or(|actor| actor.scene != screen.id)
                })
            {
                Some(b"song replay actor belongs to another scene".as_slice())
            } else {
                None
            }
        };
        if let Some(message) = invalid {
            return vm.with_native_stack(|stack| Err(stack.error(message)));
        }
        let spin = {
            let store = vm.store();
            overlays
                .iter()
                .filter_map(|(id, _)| {
                    let actor = &store.actors[id];
                    actor.effects.spin_baked.then_some((
                        *id,
                        [
                            actor.current.rot_x_deg,
                            actor.current.rot_y_deg,
                            actor.current.rot_z_deg,
                        ],
                    ))
                })
                .collect()
        };
        self.restore_song_startup_locals(vm, screen)?;
        let (roots, pin) = retained::values(
            vm,
            overlays
                .iter()
                .map(|(id, _)| Value::Table(*id))
                .chain(startup.keys().map(|id| Value::Table(*id))),
        )?;
        let function = vm.new_host_function_with_data(
            setup,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(roots),
            ],
            Setup {
                scene: screen.id,
                overlays: overlays.to_vec(),
                startup: startup.clone(),
                spin,
            },
        );
        vm.unpin(pin)?;
        let function = function?;
        // General playback removes discovery queues. The saved Init/On queues
        // below are the only queues which may enter chronological preparation.
        self.begin_playback(vm);
        vm.call(function, &[])?;
        let store = vm.store();
        store.replay_frame = Some((screen.id, 0.));
        store
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .messages = Some(song_messages::Capture::new(
            overlays.iter().enumerate().map(|(i, (id, _))| (*id, i)),
        ));
        Ok(())
    }
}

fn setup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let setup = call.function_data::<Setup>().unwrap().clone();
    for &(id, state) in &setup.overlays {
        for key in [
            "__songlua_capture_cursor",
            "__songlua_capture_duration",
            "__songlua_capture_tween_time_left",
        ] {
            raw_set(call, id, key, Value::Number(0.))?;
        }
        for key in [
            "__songlua_startup_queue_seconds",
            "__songlua_capture_easing",
            "__songlua_capture_opt1",
            "__songlua_capture_opt2",
            "__songlua_capture_block",
            "__songlua_capture_immediate_block",
        ] {
            raw_set(call, id, key, Value::Nil)?;
        }
        let blocks = call.try_new_table()?;
        raw_set(call, id, "__songlua_capture_blocks", Value::Table(blocks))?;
        let binding = get_static(call, id, b"__songlua_startup_sprite")?;
        if binding != Value::Nil {
            call.with_roots(&[binding], |call| {
                let binding = table(call, binding)?;
                for key in [
                    "Texture",
                    "Frames",
                    "__songlua_state_sprite_default_states",
                    "__songlua_state_sprite_texture_epoch",
                    "__songlua_state_sprite_frame_sheet",
                ] {
                    let value = get(call, binding, key.as_bytes())?;
                    raw_set(call, id, key, value)?;
                }
                Ok(())
            })?;
        }
        restore_getters(call, id, state)?;
    }
    let store = call.store();
    store.replay_clock = None;
    // Native chronological queues are installed only for actors with startup
    // steps. Other actors retain command capture until frame-driven queueing.
    for actor in store.actors.values_mut().filter(|a| a.scene == setup.scene) {
        actor.capture = Some(capture::Capture::default());
        effects::prepare_spin_capture(actor);
    }
    for (&id, startup) in setup
        .startup
        .iter()
        .filter(|(_, state)| !state.steps.is_empty())
    {
        let mut current = setup
            .overlays
            .iter()
            .find(|(actor, _)| *actor == id)
            .map_or_else(
                || crate::overlay_state_after_blocks(startup.initial, &startup.blocks, 0.),
                |(_, state)| *state,
            );
        let mut to = current;
        let actor = store.actors.get_mut(&id).unwrap();
        actor.capture = None;
        let getters = *actor.destination();
        let replay = actor.replay.get_or_insert(State {
            getters: Some(getters),
            ..Default::default()
        });
        replay.progress = vec![Vec::new(); startup.steps.len()];
        replay.targets = 0;
        actor.queue.clear();
        for (index, &duration) in startup.steps.iter().enumerate() {
            let mut easing = None;
            let mut opt1 = None;
            for (block, step) in startup.blocks.iter().zip(&startup.block_steps) {
                if *step == Some(index + 1) {
                    crate::apply_overlay_delta(&mut to, &block.delta);
                    for target in crate::tween_pose::TARGETS {
                        if block.delta.has_update_target(target) {
                            replay.targets |= 1u128 << target as usize;
                        }
                    }
                    easing.clone_from(&block.easing);
                    opt1 = block.opt1;
                }
            }
            actor.queue.push_back(TweenStep::startup(
                index,
                duration,
                to.into(),
                easing,
                opt1,
                startup
                    .commands
                    .get(index)
                    .cloned()
                    .flatten()
                    .map(String::into_bytes),
            ));
        }
        if let Some(rotation) = setup.spin.get(&id) {
            [current.rot_x_deg, current.rot_y_deg, current.rot_z_deg] = *rotation;
        }
        actor.current = current.into();
    }
    Ok(())
}

// Match the preparation getter export, not a wholesale render-state reset:
// discovery also writes non-tween properties which the compiler keeps.
fn restore_getters(
    call: &mut HostCall<'_>,
    id: TableId,
    state: SongLuaOverlayState,
) -> Result<(), Error> {
    for (key, value) in [
        ("__songlua_visible", state.visible),
        ("__songlua_state_sprite_texture", state.sprite_texture),
        ("__songlua_state_aft_preserve", state.aft_preserve),
        ("__songlua_state_aft_created", state.aft_created),
        ("__songlua_state_effect_timer", state.effect_timer),
        ("__songlua_state_effect_no_offset", state.effect_no_offset),
    ] {
        sprites::set(call, id, key, Value::Boolean(value))?;
    }
    for (key, value) in [
        ("__songlua_state_aux", state.aux),
        ("__songlua_aux", state.aux),
        ("__songlua_state_x", state.x),
        ("__songlua_state_y", state.y),
        ("__songlua_state_z", state.z),
        ("__songlua_current_x", state.x),
        ("__songlua_current_y", state.y),
        ("__songlua_current_z", state.z),
        ("__songlua_state_zoom", state.zoom),
        ("__songlua_state_zoom_x", state.zoom_x),
        ("__songlua_state_zoom_y", state.zoom_y),
        ("__songlua_state_zoom_z", state.zoom_z),
        ("__songlua_state_basezoom", state.basezoom),
        ("__songlua_state_basezoom_x", state.basezoom_x),
        ("__songlua_state_basezoom_y", state.basezoom_y),
        ("__songlua_state_basezoom_z", state.basezoom_z),
        ("__songlua_state_base_alpha", state.base_alpha),
        ("__songlua_state_rot_x_deg", state.rot_x_deg),
        ("__songlua_state_rot_y_deg", state.rot_y_deg),
        ("__songlua_state_rot_z_deg", state.rot_z_deg),
        ("__songlua_state_halign", state.halign),
        ("__songlua_state_valign", state.valign),
        ("__songlua_state_effect_period", state.effect_period),
    ] {
        sprites::set(call, id, key, Value::Number(f64::from(value)))?;
    }
    for (key, values) in [
        ("__songlua_state_quaternion", state.quaternion.as_slice()),
        (
            "__songlua_state_base_rotation",
            state.base_rotation.as_slice(),
        ),
        ("__songlua_state_glow", state.glow.as_slice()),
        (
            "__songlua_state_effect_magnitude",
            state.effect_magnitude.as_slice(),
        ),
    ]
    .into_iter()
    .chain(
        state
            .size
            .as_ref()
            .map(|size| ("__songlua_state_size", size.as_slice())),
    )
    .chain(
        state
            .stretch_rect
            .as_ref()
            .map(|rect| ("__songlua_state_stretch_rect", rect.as_slice())),
    ) {
        let value = sprites::vector_value(call, values)?;
        call.with_roots(&[value], |call| sprites::set(call, id, key, value))?;
    }
    let diffuse = sprites::vector_value(call, &state.diffuse)?;
    call.with_roots(&[diffuse], |call| {
        sprites::set(call, id, "__songlua_state_diffuse", diffuse)?;
        sprites::set(call, id, "__songlua_diffuse", diffuse)
    })?;
    for (key, label) in [
        (
            "__songlua_state_effect_mode",
            crate::effect_mode_label(state.effect_mode),
        ),
        (
            "__songlua_state_effect_clock",
            crate::effect_clock_label(state.effect_clock),
        ),
    ] {
        let value = call.try_intern(label.as_bytes())?;
        call.with_roots(&[value], |call| sprites::set(call, id, key, value))?;
    }
    let store = call.store();
    let actor = store.actors.get_mut(&id).unwrap();
    actor.current.x = state.x;
    actor.current.y = state.y;
    actor.current.z = state.z;
    actor.current.aux = state.aux;
    actor.current.zoom = state.zoom;
    actor.current.zoom_x = state.zoom_x;
    actor.current.zoom_y = state.zoom_y;
    actor.current.zoom_z = state.zoom_z;
    actor.current.rot_x_deg = state.rot_x_deg;
    actor.current.rot_y_deg = state.rot_y_deg;
    actor.current.rot_z_deg = state.rot_z_deg;
    actor.current.quaternion = state.quaternion;
    actor.current.diffuse = state.diffuse;
    actor.current.glow = state.glow;
    actor.replay = Some(State {
        getters: Some(actor.current),
        ..Default::default()
    });
    actor.preparation_getters = Some([state.x, state.y, state.z, state.aux]);
    actor.state.rotation = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
    actor.state.visible = state.visible;
    let render = &mut actor.state.render;
    render.sprite_texture = state.sprite_texture;
    render.aft_preserve = state.aft_preserve;
    render.aft_created = state.aft_created;
    render.basezoom = state.basezoom;
    render.basezoom_x = state.basezoom_x;
    render.basezoom_y = state.basezoom_y;
    render.basezoom_z = state.basezoom_z;
    render.base_alpha = state.base_alpha;
    render.base_rotation = state.base_rotation;
    render.halign = state.halign;
    render.valign = state.valign;
    render.effect_mode = state.effect_mode;
    render.effect_clock = state.effect_clock;
    render.effect_timer = state.effect_timer;
    render.effect_no_offset = state.effect_no_offset;
    render.effect_period = state.effect_period;
    render.effect_magnitude = state.effect_magnitude;
    if let Some(size) = state.size {
        actor.state.size = size;
        render.size = Some(size);
    }
    if state.stretch_rect.is_some() {
        render.stretch_rect = state.stretch_rect;
    }
    Ok(())
}

pub(super) fn raw_set(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &'static str,
    value: Value,
) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let key = call.intern_static(key.as_bytes())?;
        call.raw_table_set(actor, key, value)
    })
}
// `raw_set` for a field name built at run time.
pub(super) fn raw_set_named(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
    value: Value,
) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let key = call.try_intern(key.as_bytes())?;
        call.raw_table_set(actor, key, value)
    })
}

/// Preparation exports the sampled current pose and queued destination to
/// Lua. No store borrow survives allocation or a host API operation.
pub(super) fn sync_getters(call: &mut HostCall<'_>, id: TableId) -> Result<(), Error> {
    let poses = {
        let store = call.store();
        let fallback = song_pose::mode(&store, id);
        let actor = store.actors.get_mut(&id).unwrap();
        if actor.replay.is_some() {
            actor.sync_rotation_getters();
            let dest = if fallback {
                *actor.getter_destination()
            } else {
                *actor.destination()
            };
            if fallback {
                actor.state.rotation = [dest.rot_x_deg, dest.rot_y_deg, dest.rot_z_deg];
            }
            Some((actor.current, dest))
        } else {
            None
        }
    };
    let Some((current, dest)) = poses else {
        return Ok(());
    };
    for (key, value) in [
        ("aux", dest.aux),
        ("x", dest.x),
        ("y", dest.y),
        ("z", dest.z),
        ("zoom", dest.zoom),
        ("zoom_x", dest.zoom_x),
        ("zoom_y", dest.zoom_y),
        ("zoom_z", dest.zoom_z),
        ("rot_x_deg", dest.rot_x_deg),
        ("rot_y_deg", dest.rot_y_deg),
        ("rot_z_deg", dest.rot_z_deg),
        ("skew_x", dest.skew_x),
        ("skew_y", dest.skew_y),
        ("cropleft", dest.cropleft),
        ("cropright", dest.cropright),
        ("croptop", dest.croptop),
        ("cropbottom", dest.cropbottom),
        ("fadeleft", dest.fadeleft),
        ("faderight", dest.faderight),
        ("fadetop", dest.fadetop),
        ("fadebottom", dest.fadebottom),
    ] {
        raw_set_named(
            call,
            id,
            &format!("__songlua_state_{key}"),
            Value::Number(f64::from(value)),
        )?;
    }
    for (key, values) in [
        ("__songlua_diffuse", dest.diffuse),
        ("__songlua_state_diffuse", dest.diffuse),
        ("__songlua_state_glow", dest.glow),
        ("__songlua_state_quaternion", dest.quaternion),
    ] {
        let value = sprites::vector_value(call, &values)?;
        raw_set(call, id, key, value)?;
    }
    let colors = if let Some(colors) = dest.vertex_colors {
        let table = call.try_new_table()?;
        call.with_roots(&[Value::Table(table)], |call| {
            for (i, color) in colors.iter().enumerate() {
                let value = sprites::vector_value(call, color)?;
                call.raw_table_set(table, Value::Number((i + 1) as f64), value)?;
            }
            raw_set(
                call,
                id,
                "__songlua_state_vertex_colors",
                Value::Table(table),
            )
        })?;
        true
    } else {
        false
    };
    if !colors {
        raw_set(call, id, "__songlua_state_vertex_colors", Value::Nil)?;
    }
    for (key, value) in [
        ("__songlua_aux", current.aux),
        ("__songlua_current_x", current.x),
        ("__songlua_current_y", current.y),
        ("__songlua_current_z", current.z),
    ] {
        raw_set(call, id, key, Value::Number(f64::from(value)))?;
    }
    Ok(())
}

// Scalar setters export the fields they wrote, without synchronizing unrelated
// destinations. Current XYZ/aux still come from the replay's current pose.
pub(super) fn publish_scalar(
    call: &mut HostCall<'_>,
    id: TableId,
    name: &str,
) -> Result<(), Error> {
    let (poses, initial_live) = {
        let store = call.store();
        let actor = &store.actors[&id];
        let initial_live = store.live_sources.is_some() && !store.playback && !actor.raw_live_epoch;
        let live_rotation = store.live_sources.is_some()
            && matches!(
                name,
                "rotationx"
                    | "rotationy"
                    | "rotationz"
                    | "addrotationx"
                    | "addrotationy"
                    | "addrotationz"
            );
        (
            (actor.replay.is_some() || actor.raw_live_epoch || initial_live || live_rotation)
                .then(|| (actor.current, *actor.getter_destination(), actor.state.size)),
            initial_live,
        )
    };
    let Some((current, dest, size)) = poses else {
        return Ok(());
    };
    let keys: &[(&str, f32)] = match name {
        "x" | "addx" => &[("x", dest.x)],
        "y" | "addy" => &[("y", dest.y)],
        "z" | "addz" => &[("z", dest.z)],
        "xy" => &[("x", dest.x), ("y", dest.y)],
        "aux" => &[("aux", dest.aux)],
        "zoom" => &[
            ("zoom", dest.zoom),
            ("zoom_x", dest.zoom_x),
            ("zoom_y", dest.zoom_y),
            ("zoom_z", dest.zoom_z),
        ],
        "zoomx" => &[("zoom_x", dest.zoom_x)],
        "zoomy" => &[("zoom_y", dest.zoom_y)],
        "zoomz" => &[("zoom_z", dest.zoom_z)],
        "rotationx" | "addrotationx" => &[("rot_x_deg", dest.rot_x_deg)],
        "rotationy" | "addrotationy" => &[("rot_y_deg", dest.rot_y_deg)],
        "rotationz" | "addrotationz" => &[("rot_z_deg", dest.rot_z_deg)],
        _ => &[],
    };
    for &(key, value) in keys {
        raw_set_named(
            call,
            id,
            &format!("__songlua_state_{key}"),
            Value::Number(f64::from(value)),
        )?;
        let current = match key {
            "x" => Some(("__songlua_current_x", current.x)),
            "y" => Some(("__songlua_current_y", current.y)),
            "z" => Some(("__songlua_current_z", current.z)),
            "aux" => Some(("__songlua_aux", current.aux)),
            _ => None,
        };
        if let Some((key, value)) = current
            && !initial_live
        {
            raw_set(call, id, key, Value::Number(f64::from(value)))?;
        }
    }
    if matches!(name, "SetWidth" | "SetHeight" | "setsize") {
        let value = sprites::vector_value(call, &size)?;
        raw_set(call, id, "__songlua_state_size", value)?;
    }
    Ok(())
}

impl Actors {
    pub(crate) fn advance_song_replay(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        seconds: f64,
        delta: f64,
    ) -> Result<bool, Error> {
        let first = {
            let store = vm.store();
            if store
                .replay_frame
                .is_none_or(|(owner, _)| owner != screen.id)
            {
                return vm.with_native_stack(|stack| {
                    Err(stack.error(b"song preparation replay has not started"))
                });
            }
            store.replay_frame = Some((screen.id, seconds));
            store.next
        };
        let previous = std::mem::replace(&mut vm.store().replay_advancing, true);
        let result = self.advance(vm, screen, delta);
        vm.store().replay_advancing = previous;
        song_activation::adopt_created(vm.store(), screen.id, first);
        result
    }

    pub(crate) fn capture_song_tween_replay(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Option<HashMap<TableId, super::super::SongTweenReplay>> {
        let store = vm.store();
        let (owner, time) = store.replay_frame?;
        if owner != screen.id {
            return None;
        }
        let ids = store.scenes.get(&screen.id)?.actors.clone();
        let mut result = HashMap::new();
        for id in ids {
            let actor = store.actors.get_mut(&id).unwrap();
            lifecycle::sample_replay(actor, time as f32);
            let Some(replay) = &actor.replay else {
                continue;
            };
            let values = crate::tween_pose::TARGETS
                .into_iter()
                .filter(|target| replay.targets & (1u128 << *target as usize) != 0)
                .map(|target| (target, actor.current.get(target)))
                .collect();
            result.insert(
                id,
                super::super::SongTweenReplay {
                    progress: replay.progress.clone(),
                    values,
                },
            );
        }
        Some(result)
    }

    pub(crate) fn publish_song_tween_frame(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        actors: &[Value],
    ) -> bool {
        let store = vm.store();
        let Some((owner, second)) = store.replay_frame else {
            return false;
        };
        if owner != screen.id {
            return false;
        }
        let Some(scene) = store.scenes.get(&screen.id) else {
            return false;
        };
        if !scene
            .song_session
            .as_ref()
            .is_some_and(|s| s.messages.is_some())
        {
            return false;
        }
        let ids = scene.actors.clone();
        // Getter-only fallback state also uses State, but has no native queue.
        // Sample the complete native replay set, including callback-only actors.
        for id in ids {
            let actor = store.actors.get_mut(&id).unwrap();
            if actor.capture.is_none() {
                lifecycle::sample_replay(actor, second as f32);
            }
        }
        for value in actors {
            let Value::Table(id) = *value else {
                continue;
            };
            let Some(actor) = store.actors.get(&id).filter(|a| a.capture.is_none()) else {
                continue;
            };
            let Some(replay) = &actor.replay else {
                continue;
            };
            let (targets, pose) = (replay.targets, actor.current);
            let capture = store
                .scenes
                .get_mut(&screen.id)
                .unwrap()
                .song_session
                .as_mut()
                .unwrap()
                .messages
                .as_mut()
                .unwrap();
            if let Some(&index) = capture.indices.get(&id) {
                capture.immediate.publish_pose(index, targets, pose);
            }
        }
        true
    }
}
