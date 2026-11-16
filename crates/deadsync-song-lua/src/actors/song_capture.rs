//! Owned startup output; actor identities are traced by the active loader.
use super::super::song_activation::SongStartupState;
use super::*;
use crate::{SongLuaOverlayCommandBlock, SongLuaOverlayState};

#[derive(Clone, Default)]
pub(crate) struct InitialUpdate {
    states: HashMap<TableId, SongLuaOverlayState>,
    prefixes: HashMap<TableId, Vec<SongLuaOverlayCommandBlock>>,
}
#[derive(Default)]
pub(super) struct Broadcasts {
    pub overlay_started: bool,
    pub zero_update: bool,
    pub startup_commands: bool,
    pub events: Vec<(f32, String, bool)>,
}

/// Broadcast capture set aside by [`pause_preparation_broadcasts`].
#[must_use = "pass the paused capture to resume_preparation_broadcasts"]
pub(super) struct PausedBroadcasts(Option<Broadcasts>);

/// Startup-command flag replaced by [`capture_startup_commands`].
#[must_use = "pass the previous flag to restore_startup_commands"]
pub(super) struct StartupCommands(Option<bool>);

/// Retain authored Begin/On broadcasts while keeping the surrounding screen
/// transition messages outside the captured song timeline. Pair every call
/// with [`restore_startup_commands`], on error paths too.
pub(super) fn capture_startup_commands(store: &mut Store) -> StartupCommands {
    StartupCommands(
        store
            .song_broadcasts
            .as_mut()
            .map(|capture| std::mem::replace(&mut capture.startup_commands, true)),
    )
}
pub(super) fn restore_startup_commands(store: &mut Store, previous: StartupCommands) {
    if let (Some(previous), Some(capture)) = (previous.0, store.song_broadcasts.as_mut()) {
        capture.startup_commands = previous;
    }
}

// Command probes have their own Lua broadcast history. Their speculative
// delivery must not enter the chronological playback stream, whose native
// recorder starts after these probes. Live delivery keeps its existing sink.
// Pair every call with resume_preparation_broadcasts, on error paths too.
pub(super) fn pause_preparation_broadcasts(store: &mut Store) -> PausedBroadcasts {
    PausedBroadcasts(
        if store.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            None
        } else {
            store.song_broadcasts.take()
        },
    )
}
pub(super) fn resume_preparation_broadcasts(store: &mut Store, paused: PausedBroadcasts) {
    if let Some(previous) = paused.0 {
        store.song_broadcasts = Some(previous);
    }
}
#[derive(Clone)]
enum Phase {
    Before,
    After(InitialUpdate),
    QueuedBefore,
    QueuedAfter(InitialUpdate),
    Text(Vec<TableId>),
    Tweens(HashMap<TableId, SongLuaOverlayState>),
    Wrappers,
    ScreenLayers,
}
#[derive(Default)]
struct Output {
    before: InitialUpdate,
    updates: HashMap<TableId, SongStartupState>,
    wrappers: HashMap<TableId, SongLuaOverlayState>,
}
impl Actors {
    pub(super) fn before_song_discovery(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
    ) -> Result<InitialUpdate, Error> {
        Ok(self.capture_song(vm, scene, Phase::QueuedBefore)?.before)
    }
    pub(super) fn after_song_discovery(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        before: InitialUpdate,
    ) -> Result<HashMap<TableId, SongStartupState>, Error> {
        Ok(self
            .capture_song(vm, scene, Phase::QueuedAfter(before))?
            .updates)
    }
    pub(crate) fn capture_song_startup_text(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        wrappers: Vec<TableId>,
    ) -> Result<(), Error> {
        self.capture_song(vm, scene, Phase::Text(wrappers))?;
        Ok(())
    }
    pub(crate) fn begin_song_capture(&self, vm: &mut Vm) {
        vm.store().song_broadcasts = Some(Broadcasts::default());
    }
    pub(crate) fn song_zero_update(&self, vm: &mut Vm, active: bool) {
        if let Some(capture) = &mut vm.store().song_broadcasts {
            capture.zero_update = active;
        }
    }
    pub(crate) fn take_song_broadcasts(&self, vm: &mut Vm) -> Vec<(f32, String, bool)> {
        vm.store()
            .song_broadcasts
            .as_mut()
            .map(|capture| std::mem::take(&mut capture.events))
            .unwrap_or_default()
    }
    fn capture_song(&self, vm: &mut Vm, scene: ActorScene, phase: Phase) -> Result<Output, Error> {
        let output = Arc::new(Mutex::new(Output::default()));
        let function = vm.new_host_function_with_data(
            capture,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            (scene.id, phase, output.clone()),
        )?;
        let first = vm.store().next;
        let result = vm.call(function, &[]).map(|_| ());
        song_activation::adopt_created(vm.store(), scene.id, first);
        result?;
        Ok(std::mem::take(&mut *output.lock().unwrap()))
    }
    pub(crate) fn before_song_update(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
    ) -> Result<InitialUpdate, Error> {
        Ok(self.capture_song(vm, scene, Phase::Before)?.before)
    }
    pub(crate) fn after_song_update(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        before: InitialUpdate,
    ) -> Result<HashMap<TableId, SongStartupState>, Error> {
        Ok(self.capture_song(vm, scene, Phase::After(before))?.updates)
    }
    pub(crate) fn screen_startup_states(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
    ) -> Result<HashMap<TableId, SongStartupState>, Error> {
        Ok(self.capture_song(vm, scene, Phase::ScreenLayers)?.updates)
    }
    pub(crate) fn song_startup_tweens(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        wrappers: HashMap<TableId, SongLuaOverlayState>,
    ) -> Result<HashMap<TableId, SongStartupState>, Error> {
        Ok(self
            .capture_song(vm, scene, Phase::Tweens(wrappers))?
            .updates)
    }
    pub(crate) fn song_wrapper_initials(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
    ) -> Result<HashMap<TableId, SongLuaOverlayState>, Error> {
        Ok(self.capture_song(vm, scene, Phase::Wrappers)?.wrappers)
    }
}
fn capture(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (scene, phase, output) = call
        .function_data::<(u64, Phase, Arc<Mutex<Output>>)>()
        .unwrap()
        .clone();
    let root = call.store().scenes[&scene]
        .song_session
        .as_ref()
        .unwrap()
        .roots;
    call.with_roots(&[Value::Table(root)], |call| {
        let mut result = Output::default();
        match phase {
            Phase::Before => {
                let retained = retain_table(call, root)?;
                collect_initial(call, root, retained, &mut result.before.states)?;
                for &actor in result.before.states.keys() {
                    result
                        .before
                        .prefixes
                        .insert(actor, command_capture(call.store(), actor).blocks);
                }
            }
            Phase::After(before) => result.updates = capture_states(call, before)?,
            Phase::QueuedBefore => {
                let retained = retain_table(call, root)?;
                collect_queued_initial(call, root, retained, &mut result.before.states)?;
            }
            Phase::QueuedAfter(before) => {
                for (actor, initial) in before.states {
                    result.updates.insert(
                        actor,
                        SongStartupState {
                            initial,
                            blocks: command_capture(call.store(), actor).blocks,
                            steps: Vec::new(),
                            commands: Vec::new(),
                            block_steps: Vec::new(),
                            clock_path: clock_path(call, actor)?,
                        },
                    );
                }
            }
            Phase::Text(wrappers) => {
                let mut actors: Vec<_> = call.store().scenes[&scene]
                    .initial_states
                    .as_ref()
                    .unwrap()
                    .actors
                    .keys()
                    .copied()
                    .collect();
                actors.extend(wrappers);
                for actor in actors {
                    let kind = sprites::field(call, actor, "__songlua_actor_type")?;
                    if initial_state::value_text(call, kind)?.is_some_and(|kind| {
                        kind.eq_ignore_ascii_case("BitmapText")
                            || kind.eq_ignore_ascii_case("RollingNumbers")
                    }) {
                        let text = get_static(call, actor, b"Text")?;
                        raw_set(call, actor, "__songlua_startup_text", text)?;
                    }
                }
            }
            Phase::Tweens(wrappers) => {
                let mut states = call.store().scenes[&scene]
                    .initial_states
                    .as_ref()
                    .unwrap()
                    .actors
                    .clone();
                for (actor, state) in wrappers {
                    states.entry(actor).or_insert(state);
                }
                result.updates = capture_states(
                    call,
                    InitialUpdate {
                        states,
                        prefixes: HashMap::new(),
                    },
                )?;
            }
            Phase::ScreenLayers => {
                let states = call.store().scenes[&scene]
                    .initial_states
                    .as_ref()
                    .unwrap()
                    .screen_layers
                    .clone();
                result.updates = capture_states(
                    call,
                    InitialUpdate {
                        states,
                        prefixes: HashMap::new(),
                    },
                )?;
            }
            Phase::Wrappers => result.wrappers = wrapper_initials(call, root)?,
        }
        *output.lock().unwrap() = result;
        Ok(())
    })
}
fn command_capture(store: &mut Store, actor: TableId) -> ActorCommandCapture {
    let guard = &mut *store;
    guard
        .actors
        .get_mut(&actor)
        .and_then(|actor| actor.capture.as_mut())
        .map(capture::Capture::snapshot)
        .unwrap_or_else(|| ActorCommandCapture {
            blocks: Vec::new(),
            block_steps: Vec::new(),
            steps: Vec::new(),
            queued_commands: Vec::new(),
        })
}
fn retain_table(call: &mut HostCall<'_>, root: TableId) -> Result<TableId, Error> {
    let retained = call.try_new_table()?;
    call.with_roots(&[Value::Table(retained)], |call| {
        if let Some(previous) = call.table_host_state(root)? {
            call.raw_table_set(retained, Value::Number(1.), Value::Table(previous))?;
        }
        call.set_table_host_state(root, Some(retained))?;
        Ok(retained)
    })
}
fn collect_initial(
    call: &mut HostCall<'_>,
    actor: TableId,
    retained: TableId,
    states: &mut HashMap<TableId, SongLuaOverlayState>,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let kind = sprites::field(call, actor, "__songlua_actor_type")?;
        if initial_state::value_text(call, kind)?.is_some() {
            let state = initial_state::read(call, actor)?;
            call.raw_table_set(retained, Value::Table(actor), Value::Boolean(true))?;
            states.insert(actor, state);
        }
        raw_children(call, actor, |call, child| {
            collect_initial(call, child, retained, states)
        })
    })
}

fn collect_queued_initial(
    call: &mut HostCall<'_>,
    actor: TableId,
    retained: TableId,
    states: &mut HashMap<TableId, SongLuaOverlayState>,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let kind = sprites::field(call, actor, "__songlua_actor_type")?;
        if let Some(kind) = initial_state::value_text(call, kind)? {
            states.insert(actor, initial_state::read(call, actor)?);
            call.raw_table_set(retained, Value::Table(actor), Value::Boolean(true))?;
            if let Some(capture) = call
                .store()
                .actors
                .get_mut(&actor)
                .and_then(|actor| actor.capture.as_mut())
            {
                capture.begin_discovery();
            }
            if kind.eq_ignore_ascii_case("Sprite") {
                capture_sprite_binding(call, actor)?;
            }
        }
        let wrappers = sprites::field(call, actor, "__songlua_wrappers")?;
        if wrappers != Value::Nil {
            call.with_roots(&[wrappers], |call| {
                let wrappers = table(call, wrappers)?;
                for i in 1.. {
                    let value = call.raw_table_get(wrappers, Value::Number(i as f64))?;
                    if value == Value::Nil {
                        break;
                    }
                    collect_queued_initial(call, table(call, value)?, retained, states)?;
                }
                Ok(())
            })?;
        } else {
            let wrappers = call
                .store()
                .actors
                .get(&actor)
                .map(|actor| actor.wrappers.clone())
                .unwrap_or_default();
            for wrapper in wrappers {
                collect_queued_initial(call, wrapper, retained, states)?;
            }
        }
        raw_children(call, actor, |call, child| {
            collect_queued_initial(call, child, retained, states)
        })
    })
}

fn capture_sprite_binding(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let binding = call.try_new_table()?;
    call.with_roots(&[Value::Table(binding)], |call| {
        let entries = call.call(helper(call, 7)?, &[Value::Table(actor)])?[0];
        call.with_roots(&[entries], |call| {
            let entries = table(call, entries)?;
            for i in 1..=call.raw_table_len(entries)? {
                let row = table(call, call.raw_table_get(entries, Value::Number(i as f64))?)?;
                let key = call.raw_table_get(row, Value::Number(1.))?;
                let Value::String(id) = key else {
                    continue;
                };
                let name = std::str::from_utf8(call.string_bytes(id).unwrap())
                    .map_err(|e| call.error(e.to_string().as_bytes()))?;
                if matches!(
                    name,
                    "Texture"
                        | "Frames"
                        | "__songlua_state_sprite_default_states"
                        | "__songlua_state_sprite_texture_epoch"
                        | "__songlua_state_sprite_frame_sheet"
                ) || name.starts_with("Frame")
                    || name.starts_with("Delay")
                {
                    let value = call.raw_table_get(row, Value::Number(2.))?;
                    let value = call.call(helper(call, 15)?, &[value])?[0];
                    call.raw_table_set(binding, key, value)?;
                }
            }
            raw_set(
                call,
                actor,
                "__songlua_startup_sprite",
                Value::Table(binding),
            )
        })
    })
}
fn raw_set(call: &mut HostCall<'_>, actor: TableId, name: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_set(actor, key, value)
}
fn raw_children(
    call: &mut HostCall<'_>,
    actor: TableId,
    mut visit: impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut i = 1;
    loop {
        let value = call.raw_table_get(actor, Value::Number(i as f64))?;
        if value == Value::Nil {
            break;
        }
        if let Value::Table(child) = value {
            call.with_roots(&[value], |call| visit(call, child))?;
        }
        i += 1;
    }
    Ok(())
}
pub(super) fn wrapper_initials(
    call: &mut HostCall<'_>,
    root: TableId,
) -> Result<HashMap<TableId, SongLuaOverlayState>, Error> {
    let retained = retain_table(call, root)?;
    let mut states = HashMap::new();
    collect_wrappers(call, root, retained, &mut states)?;
    Ok(states)
}
fn collect_wrappers(
    call: &mut HostCall<'_>,
    actor: TableId,
    retained: TableId,
    states: &mut HashMap<TableId, SongLuaOverlayState>,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let wrappers = sprites::field(call, actor, "__songlua_wrappers")?;
        let mut visit = |call: &mut HostCall<'_>, wrapper| {
            call.raw_table_set(retained, Value::Table(wrapper), Value::Boolean(true))?;
            states.entry(wrapper).or_default();
            collect_wrappers(call, wrapper, retained, states)
        };
        if wrappers != Value::Nil {
            call.with_roots(&[wrappers], |call| {
                raw_children(call, table(call, wrappers)?, &mut visit)
            })?;
        } else {
            let native = call
                .store()
                .actors
                .get(&actor)
                .map(|a| a.wrappers.clone())
                .unwrap_or_default();
            for wrapper in native {
                visit(call, wrapper)?;
            }
        }
        raw_children(call, actor, |call, child| {
            collect_wrappers(call, child, retained, states)
        })
    })
}
fn clock_path(call: &mut HostCall<'_>, actor: TableId) -> Result<Vec<[f32; 2]>, Error> {
    let retained = call.try_new_table()?;
    call.with_roots(&[Value::Table(retained), Value::Table(actor)], |call| {
        let mut path = Vec::new();
        let mut next = Some(actor);
        let mut wrapper_child = false;
        let mut seen = HashSet::new();
        while let Some(actor) = next {
            if !seen.insert(actor) {
                return Err(call.error(b"cycle in actor update ancestry"));
            }
            call.raw_table_set(retained, Value::Table(actor), Value::Boolean(true))?;
            let hibernate = get_static(call, actor, b"__songlua_hibernate_seconds")?;
            let theme = get_static(call, actor, b"__songlua_theme_hibernating")?;
            let hibernate = if !matches!(theme, Value::Nil | Value::Boolean(false)) {
                f32::INFINITY
            } else if hibernate == Value::Nil {
                0.
            } else {
                sprites::number(call, hibernate)? as f32
            };
            let rate = if wrapper_child {
                1.
            } else {
                let value = get_static(call, actor, b"__songlua_update_rate")?;
                if value == Value::Nil {
                    call.store()
                        .actors
                        .get(&actor)
                        .map_or(1., |actor| actor.update_rate)
                } else {
                    sprites::number(call, value)? as f32
                }
            };
            path.push([hibernate, rate]);
            let kind = sprites::field(call, actor, "__songlua_actor_type")?;
            wrapper_child = initial_state::value_text(call, kind)?
                .is_some_and(|kind| kind.eq_ignore_ascii_case("WrapperState"));
            let parent = get_static(call, actor, b"__songlua_parent")?;
            next = if parent == Value::Nil {
                call.store()
                    .actors
                    .get(&actor)
                    .and_then(|actor| actor.parent)
            } else {
                Some(table(call, parent)?)
            };
        }
        path.reverse();
        Ok(path)
    })
}
pub(super) fn broadcast(
    call: &mut HostCall<'_>,
    message: &[u8],
    has_params: bool,
) -> Result<(), Error> {
    let active = {
        let guard = call.store();
        guard.song_broadcasts.as_ref().is_some_and(|capture| {
            // Begin/On broadcasts occur while startup queues are still owned;
            // they are native events, unlike the screen transition dispatch
            // between those command passes.
            guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
                || (capture.overlay_started
                    && (capture.startup_commands
                        || capture.zero_update
                        || guard.startup_queues.is_none()))
        })
    };
    if call.store().song_broadcasts.is_none() {
        return Ok(());
    }
    let beat = effects::values(call, false).map_or(0., |values| values[0]);
    if !active {
        return Ok(());
    }
    let message =
        String::from_utf8(message.to_vec()).map_err(|e| call.error(e.to_string().as_bytes()))?;
    if let Some(capture) = &mut call.store().song_broadcasts {
        capture.events.push((beat, message, has_params));
    }
    Ok(())
}

// Theme construction delivers real callbacks while temporarily excluding its
// events from the live song stream, matching the two native construction scopes.
pub(super) fn without_live_broadcasts<T>(
    call: &mut HostCall<'_>,
    action: impl FnOnce(&mut HostCall<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let sources = call.store().live_sources.take();
    let previous = {
        let guard = call.store();
        if guard.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            guard.song_broadcasts.take()
        } else {
            None
        }
    };
    let result = action(call);
    if let Some(sources) = sources {
        call.store().live_sources = Some(sources);
    }
    if let Some(previous) = previous {
        call.store().song_broadcasts = Some(previous);
    }
    result
}

fn capture_states(
    call: &mut HostCall<'_>,
    before: InitialUpdate,
) -> Result<HashMap<TableId, SongStartupState>, Error> {
    let mut updates = HashMap::new();
    for (actor, initial) in before.states {
        let captured = command_capture(call.store(), actor);
        let mut commands = Vec::new();
        let mut pending = captured.queued_commands.iter();
        for step in &captured.steps {
            let command = if step.is_none() {
                pending
                    .next()
                    .map(|(name, _)| {
                        String::from_utf8(name.clone())
                            .map_err(|e| call.error(e.to_string().as_bytes()))
                    })
                    .transpose()?
            } else {
                None
            };
            commands.push(command);
        }
        let prefix = before
            .prefixes
            .get(&actor)
            .filter(|prefix| captured.blocks.starts_with(prefix))
            .map_or(0, Vec::len);
        let state = SongStartupState {
            initial,
            blocks: captured.blocks.into_iter().skip(prefix).collect(),
            block_steps: captured
                .block_steps
                .into_iter()
                .skip(prefix)
                .map(|step| (step != 0).then_some(step))
                .collect(),
            steps: captured
                .steps
                .into_iter()
                .map(|step| step.unwrap_or(0.))
                .collect(),
            commands,
            clock_path: clock_path(call, actor)?,
        };
        updates.insert(actor, state);
    }
    Ok(updates)
}
