//! Activation uses the loader list; the screen owns its siblings separately.
use super::super::managers::screen::Requests;
use super::super::song_activation::SongActivation;
use super::*;
use crate::{SongLuaCompileContext, SongLuaStartup};

pub(super) struct Session {
    pub roots: TableId,
    pub ready: bool,
    pub activated: bool,
    pub discovery_started: bool,
    pub locals: Option<super::super::function_snapshot::Snapshot>,
    pub queued: HashMap<TableId, super::super::SongStartupState>,
    pub messages: Option<song_messages::Capture>,
}
#[derive(Clone)]
struct Activate {
    scene: u64,
    context: SongLuaCompileContext,
    requests: Arc<Mutex<Requests>>,
    output: Arc<Mutex<Option<SongActivation>>>,
}
impl Actors {
    pub(crate) fn complete_song_activation(&self, vm: &mut Vm, screen: ActorScene) {
        vm.store()
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .unwrap()
            .activated = true;
    }
    pub(crate) fn discover_song_startup(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        activation: &SongActivation,
        live: bool,
    ) -> Result<(), Error> {
        let invalid = {
            let guard = vm.store();
            match &guard.scenes[&screen.id].song_session {
                None => Some(b"song layers have not been initialized".as_slice()),
                Some(session) if !session.activated || session.discovery_started => {
                    Some(b"song startup discovery requires one completed activation".as_slice())
                }
                Some(_) => None,
            }
        };
        if let Some(message) = invalid {
            return vm.with_native_stack(|stack| Err(stack.error(message)));
        }
        let (root, actors, queued) = {
            let guard = vm.store();
            let scene = guard.scenes.get_mut(&screen.id).unwrap();
            let session = scene.song_session.as_mut().unwrap();
            session.discovery_started = true;
            let root = session.roots;
            let mut actors: Vec<_> = scene
                .initial_states
                .as_ref()
                .unwrap()
                .actors
                .keys()
                .copied()
                .collect();
            actors.extend(activation.wrapper_initials.keys().copied());
            let queued = guard.startup_queues.take().unwrap_or_default();
            if live || activation.new_screen.is_some() {
                return Ok(());
            }
            (root, actors, queued)
        };
        let has_queued = !queued.is_empty();
        let locals = if has_queued {
            Some(super::super::function_snapshot::Snapshot::capture(
                vm, root, &actors,
            )?)
        } else {
            None
        };
        // Keep the attachment reachable even if a probe removes actors from
        // the authored array or replaces the global bindings that owned them.
        let first = vm.store().next;
        let result = (|| {
            if !has_queued {
                return Ok(HashMap::new());
            }
            let before = self.before_song_discovery(vm, screen)?;
            self.discover_queued(vm, queued)?;
            self.after_song_discovery(vm, screen, before)
        })();
        adopt_created(vm.store(), screen.id, first);
        match result {
            Ok(queued) => {
                let guard = vm.store();
                let session = guard
                    .scenes
                    .get_mut(&screen.id)
                    .unwrap()
                    .song_session
                    .as_mut()
                    .unwrap();
                session.locals = locals;
                session.queued = queued;
                Ok(())
            }
            Err(error) => {
                if let Some(locals) = locals {
                    locals.discard(vm)?;
                }
                Err(error)
            }
        }
    }

    pub(crate) fn song_startup_queued_states(
        &self,
        vm: &Vm,
        screen: ActorScene,
    ) -> Option<HashMap<TableId, super::super::SongStartupState>> {
        let guard = vm.store_ref();
        Some(
            guard
                .scenes
                .get(&screen.id)?
                .song_session
                .as_ref()?
                .queued
                .clone(),
        )
    }

    pub(crate) fn restore_song_startup_locals(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Result<(), Error> {
        let locals = vm
            .store()
            .scenes
            .get_mut(&screen.id)
            .unwrap()
            .song_session
            .as_mut()
            .and_then(|session| session.locals.take());
        if let Some(locals) = locals {
            locals.restore(vm)?;
        }
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn song_startup_queue(&self, vm: &Vm) -> Vec<Value> {
        vm.store_ref()
            .startup_queues
            .as_deref()
            .unwrap_or_default()
            .iter()
            .copied()
            .map(Value::Table)
            .collect()
    }

    pub(crate) fn activate_song_layers(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        context: &SongLuaCompileContext,
        requests: Arc<Mutex<Requests>>,
    ) -> Result<SongActivation, Error> {
        let output = Arc::new(Mutex::new(None));
        let function = vm.new_host_function_with_data(
            activate,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Activate {
                scene: screen.id,
                context: context.clone(),
                requests,
                output: output.clone(),
            },
        )?;
        let first = vm.store().next;
        let result = vm.call(function, &[]).map(|_| ());
        adopt_created(vm.store(), screen.id, first);
        result?;
        Ok(output.lock().unwrap().take().unwrap())
    }
    pub(crate) fn zero_song_update(&self, vm: &mut Vm, screen: ActorScene) -> Result<(), Error> {
        let function = vm.new_host_function_with_data(
            zero_update,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            screen.id,
        )?;
        let first = vm.store().next;
        let result = vm.call(function, &[]).map(|_| ());
        adopt_created(vm.store(), screen.id, first);
        result
    }
}
fn zero_update(call: &mut HostCall<'_>) -> Result<(), Error> {
    let id = call.function_data::<u64>().unwrap().clone();
    let roots = call.store().scenes[&id]
        .song_session
        .as_ref()
        .unwrap()
        .roots;
    zero_node(call, roots)
}
fn activate(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Activate>().unwrap().clone();
    let roots = {
        let live = call
            .store_ref()
            .clock
            .live
            .load(std::sync::atomic::Ordering::Relaxed);
        if data.context.live != live {
            return Err(call.error(b"song activation context and runtime modes differ"));
        }
        let Some(session) = call
            .store()
            .scenes
            .get_mut(&data.scene)
            .unwrap()
            .song_session
            .as_mut()
        else {
            return Err(call.error(b"song layers have not been initialized"));
        };
        if !session.ready {
            return Err(call.error(b"song initialization failed or activation already began"));
        }
        session.ready = false;
        session.roots
    };
    call.with_roots(&[Value::Table(roots)], |call| {
        let globals = table(call, helper(call, 5)?)?;
        let retained = call.try_new_table()?;
        call.with_roots(&[Value::Table(retained)], |call| {
            // Retain initialized globals as each typed read succeeds. A later
            // getter may collect or replace the earlier global table.
            if let Some(previous) = call.table_host_state(roots)? {
                call.raw_table_set(retained, Value::Number(5.), Value::Table(previous))?;
            }
            call.set_table_host_state(roots, Some(retained))?;
            let mut initialized = [None; 4];
            for (i, key) in ["mod_actions", "mod_perframes", "mods", "mods_ease"]
                .into_iter()
                .enumerate()
            {
                let value = sprites::field(call, globals, key)?;
                if value != Value::Nil {
                    table(call, value)?;
                    call.raw_table_set(retained, Value::Number((i + 1) as f64), value)?;
                    initialized[i] = Some(value);
                }
                if i == 0 && data.requests.lock().unwrap().next.take().is_some() {
                    log::warn!("Setting a new screen during an InitCommand is not allowed.");
                }
            }
            raw_set(
                call,
                globals,
                "__songlua_screen_loading",
                Value::Boolean(false),
            )?;
            call.store().startup_queues.get_or_insert_with(Vec::new);
            {
                // The compiler starts its capture after the initialized-global
                // reads. Constructor/Init/previous-End events precede it.
                let store = call.store();
                store.prepared_sounds.clear();
                store.sprite_textures.clear();
                store.judgment_textures.clear();
                store.sound_capture = Some(false);
                if let Some(capture) = &mut store.song_broadcasts {
                    if !data.context.live {
                        capture.events.clear();
                    }
                    capture.overlay_started = true;
                    capture.zero_update = false;
                }
            }

            // Loader children have no native parent of their own. Begin walks
            // direct unique roots; On below follows the raw sequence including aliases.
            {
                let previous = song_capture::capture_startup_commands(call.store());
                let result = native_children::with_children(call, roots, |call, actor| {
                    let beat = get_static(call, actor, b"__songlua_background_start_beat")?;
                    if beat != Value::Nil {
                        sprites::number(call, beat)?;
                        return Ok(());
                    }
                    named_tree(call, actor, b"Begin")
                });
                song_capture::restore_startup_commands(call.store(), previous);
                result?;
            }
            let top = get_static(call, globals, b"__songlua_top_screen")?;
            if top != Value::Nil {
                call.with_roots(&[top], |call| {
                    let children = sprites::field(call, table(call, top)?, "__songlua_children")?;
                    call.with_roots(&[children], |call| {
                        let actor = get_static(call, table(call, children)?, b"In")?;
                        if actor != Value::Nil {
                            call.with_roots(&[actor], |call| {
                                transitions::start(call, table(call, actor)?)
                            })?;
                        }
                        Ok(())
                    })
                })?;
            }
            #[cfg(feature = "test-support")]
            if let Some(&[music, beat, _]) = data.context.compile_clock_inputs.first() {
                let origin = data
                    .context
                    .song_timing
                    .as_ref()
                    .map_or(0., |t| f64::from(t.get_time_for_beat_exact(0.)));
                let runtime = sprites::field(call, globals, crate::SONG_LUA_RUNTIME_KEY)?;
                call.with_roots(&[runtime], |call| {
                    let runtime = table(call, runtime)?;
                    sprites::set(
                        call,
                        runtime,
                        crate::SONG_LUA_RUNTIME_BEAT_KEY,
                        Value::Number(f64::from(beat as f32)),
                    )?;
                    sprites::set(
                        call,
                        runtime,
                        crate::SONG_LUA_RUNTIME_SECONDS_KEY,
                        Value::Number(f64::from(
                            ((music - origin) / f64::from(crate::song_music_rate(&data.context)))
                                as f32,
                        )),
                    )?;
                    Ok(())
                })?;
            }
            messages::record(call, b"ScreenChanged", false)?;
            messages::dispatch(call, b"ScreenChanged", None, true)?;
            {
                let previous = song_capture::capture_startup_commands(call.store());
                let result = startup(call, roots);
                song_capture::restore_startup_commands(call.store(), previous);
                result?;
            }
            let wrapper_initials = song_capture::wrapper_initials(call, roots)?;
            let options = read_options(call, globals, &data.context)?;
            *data.output.lock().unwrap() = Some(SongActivation {
                initialized_globals: initialized,
                wrapper_initials,
                updates: HashMap::new(),
                tweens: HashMap::new(),
                broadcasts: Vec::new(),
                options,
                new_screen: None,
            });
            Ok(())
        })
    })
}
fn named_tree(call: &mut HostCall<'_>, actor: TableId, name: &[u8]) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        native_children::run_command(call, actor, name, None)?;
        native_children::with_children(call, actor, |call, child| named_tree(call, child, name))?;
        Ok(())
    })
}
pub(super) fn startup(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        if truthy(get_static(call, actor, b"__songlua_defer_on")?) {
            return Ok(());
        }
        if truthy(sprites::field(
            call,
            actor,
            "__songlua_startup_commands_ran",
        )?) {
            return Ok(());
        }
        sprites::set(
            call,
            actor,
            "__songlua_startup_command_started",
            Value::Boolean(true),
        )?;
        sprites::set(
            call,
            actor,
            "__songlua_startup_children_walked",
            Value::Boolean(false),
        )?;
        native_children::run_command_with_drain(call, actor, b"On", None, false)?;
        // Fetch each slot after the preceding child's On: callbacks may append
        // or remove children during this walk.
        let mut i = 1;
        loop {
            let child = call.raw_table_get(actor, Value::Number(i as f64))?;
            if child == Value::Nil {
                break;
            }
            if let Value::Table(child) = child {
                call.with_roots(&[Value::Table(child)], |call| {
                    native_children::adopt_declared(call, child, actor)?;
                    startup(call, child)
                })?;
            }
            i += 1;
        }
        if call.store().actors.contains_key(&actor)
            && let Some(stream) = song_meters::stream(call, actor)?
        {
            startup(call, stream)?;
        }
        sprites::set(
            call,
            actor,
            "__songlua_startup_children_walked",
            Value::Boolean(true),
        )?;
        if call.store().actors.contains_key(&actor) {
            capture::drain_commands(call, actor)?;
        }
        sprites::set(
            call,
            actor,
            "__songlua_startup_commands_ran",
            Value::Boolean(true),
        )?;
        if let Some(actor) = call.store().actors.get_mut(&actor) {
            actor.started = true;
        }
        Ok(())
    })
}
fn read_options(
    call: &mut HostCall<'_>,
    globals: TableId,
    context: &SongLuaCompileContext,
) -> Result<SongLuaStartup, Error> {
    let mut options = SongLuaStartup::default();
    for (i, key) in crate::SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        if !context.players[i].enabled {
            continue;
        }
        let table = sprites::field(call, globals, key)?;
        call.with_roots(&[table], |call| {
            let value = get_static(
                call,
                super::table(call, table)?,
                b"__songlua_noteskin_override",
            )?;
            options.noteskins[i] = initial_state::value_text(call, value)?;
            Ok(())
        })?;
    }
    let top = sprites::field(call, globals, "__songlua_top_screen")?;
    call.with_roots(&[top], |call| {
        let top = table(call, top)?;
        let value = get_static(call, top, b"__songlua_min_seconds_to_music")?;
        if value != Value::Nil {
            options.min_seconds_to_music = Some(sprites::number(call, value)? as f32);
        }
        let children = sprites::field(call, top, "__songlua_children")?;
        call.with_roots(&[children], |call| {
            let actor = sprites::field(call, table(call, children)?, "In")?;
            if actor != Value::Nil {
                call.with_roots(&[actor], |call| {
                    let visible = sprites::field(call, table(call, actor)?, "__songlua_visible")?;
                    options.hide_in = visible == Value::Boolean(false);
                    Ok(())
                })?;
            }
            Ok(())
        })
    })?;
    Ok(options)
}
fn raw_set(call: &mut HostCall<'_>, actor: TableId, key: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_set(actor, key, value)
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

pub(super) fn adopt_created(store: &mut Store, owner: u64, first: u64) {
    let scenes = store
        .scenes
        .keys()
        .copied()
        .filter(|id| *id > first)
        .collect::<Vec<_>>();
    for scene in scenes {
        gameplay_theme::adopt_owned_scene(store, owner, scene);
    }
}

// The real startup Update(0) uses dynamic raw-array traversal, not the cached
// chronological plan. It does not dispatch future queues or transition jobs.
fn zero_node(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        if truthy(get_static(call, actor, b"__songlua_live_inactive")?)
            || truthy(get_static(call, actor, b"__songlua_theme_hibernating")?)
        {
            return Ok(());
        }
        let hibernate = get_static(call, actor, b"__songlua_hibernate_seconds")?;
        if hibernate != Value::Nil && sprites::number(call, hibernate)? > 0. {
            return Ok(());
        }
        let owned = call.store().actors.contains_key(&actor);
        let wrappers = sprites::field(call, actor, "__songlua_wrappers")?;
        if wrappers == Value::Nil && owned {
            // Rust actor bindings own their default wrapper list in metadata.
            // Resolve each slot after the previous callback, as the native raw
            // sequence walker does, so appended wrappers run in this update.
            let mut index = 0;
            loop {
                let wrapper = call.store().actors[&actor].wrappers.get(index).copied();
                let Some(wrapper) = wrapper else {
                    break;
                };
                zero_node(call, wrapper)?;
                index += 1;
            }
        }
        if wrappers != Value::Nil {
            call.with_roots(&[wrappers], |call| {
                let wrappers = table(call, wrappers)?;
                let mut i = 1;
                loop {
                    let wrapper = call.raw_table_get(wrappers, Value::Number(i as f64))?;
                    if wrapper == Value::Nil {
                        break;
                    }
                    zero_node(call, table(call, wrapper)?)?;
                    i += 1;
                }
                Ok(())
            })?;
        }
        if owned {
            let active = {
                let guard = call.store();
                let render = guard.actors[&actor].state.render;
                (!render.effect_timer
                    && render.effect_clock == deadlib_present::anim::EffectClock::Time)
                    || render.effect_mode != deadlib_present::anim::EffectMode::None
                    || render.rainbow
            };
            if active {
                effects::advance(call, actor, 0.)?;
            }
        }
        let mut i = 1;
        loop {
            let child = call.raw_table_get(actor, Value::Number(i as f64))?;
            if child == Value::Nil {
                break;
            }
            if let Value::Table(child) = child {
                call.with_roots(&[Value::Table(child)], |call| {
                    native_children::adopt_declared(call, child, actor)?;
                    zero_node(call, child)
                })?;
            }
            i += 1;
        }
        if owned && let Some(stream) = song_meters::stream(call, actor)? {
            zero_node(call, stream)?;
        }
        let key = if truthy(get_static(call, actor, b"__songlua_update_pending")?) {
            b"__songlua_update_previous".as_slice()
        } else {
            b"__songlua_update_function".as_slice()
        };
        let function = get(call, actor, key)?;
        if function != Value::Nil {
            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"actor update function must be a function"));
            }
            let result = call.with_roots(&[function], |call| {
                run_callback(call, actor, function, Some(Value::Number(0.)), true, None)
            });
            let drain = capture::drain_commands(call, actor);
            if let Err(error) = result {
                diagnostics::report(call, actor, diagnostics::Callback::Update, error)?;
            }
            if let Err(error) = drain {
                diagnostics::report(call, actor, diagnostics::Callback::Queue, error)?;
            }
        }
        Ok(())
    })
}
