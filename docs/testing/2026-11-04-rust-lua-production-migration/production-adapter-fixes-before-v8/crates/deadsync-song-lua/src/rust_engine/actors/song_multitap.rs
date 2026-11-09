//! Compiler observations for the existing multitap preparation branch.
use super::*;

#[derive(Clone, Copy)]
enum Job {
    Enter(TableId, Option<TableId>, DeltaSource),
    Advance(TableId, usize, bool),
    Recurring(TableId, usize),
    Callback(TableId, usize),
    Texture(TableId),
    PlayerFeedback(TableId),
    Transition(TableId),
}
#[derive(Clone, Copy)]
enum DeltaSource {
    Children,
    Wrappers,
    Columns,
}
pub(super) struct Plan {
    pub(super) roots: TableId,
    jobs: Vec<Job>,
    orders: Arc<HashMap<TableId, usize>>,
    counter: usize,
    slots: Vec<(usize, Option<usize>)>,
    actor_count: usize,
    scratch: Mutex<Vec<Deltas>>,
    live: bool,
}

#[derive(Clone, Copy, Default)]
struct Deltas {
    input: (Option<f64>, Option<f32>),
    wrappers: (Option<f64>, Option<f32>),
    children: (Option<f64>, Option<f32>),
}

impl Job {
    fn actor(self) -> TableId {
        match self {
            Self::Enter(id, ..)
            | Self::Advance(id, ..)
            | Self::Recurring(id, ..)
            | Self::Callback(id, ..)
            | Self::Texture(id)
            | Self::PlayerFeedback(id)
            | Self::Transition(id) => id,
        }
    }
}

#[derive(Clone)]
struct Advance {
    store: Shared,
    delta: f64,
    statistics: Option<live_feedback::Statistics>,
}

impl Actors {
    pub(in crate::rust_engine) fn advance_compiler_replay(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        root: Value,
        frame: usize,
        delta: f64,
    ) -> Result<bool, Error> {
        let Some(seconds) = self.set_song_replay_frame(screen, frame) else {
            return Ok(false);
        };
        let first = {
            let mut store = self.store.lock().unwrap();
            store.replay_frame = Some((screen.id, seconds));
            store.next
        };
        let previous = std::mem::replace(&mut self.store.lock().unwrap().replay_advancing, true);
        let result = (|| {
            let function = vm.new_host_function_with_data(
                advance,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    root,
                ],
                Advance {
                    store: self.store.clone(),
                    delta,
                    statistics: None,
                },
            )?;
            vm.call(function, &[]).map(|_| true)
        })();
        self.store.lock().unwrap().replay_advancing = previous;
        song_activation::adopt_created(&self.store, screen.id, first);
        result
    }

    pub(in crate::rust_engine) fn advance_live(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        delta: f64,
        before_enter: Option<Value>,
        statistics: live_feedback::Statistics,
        catalog: Root,
    ) -> Result<bool, Error> {
        let (root, first) = {
            let store = self.store.lock().unwrap();
            if !store.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
                drop(store);
                return vm.with_native_stack(|stack| {
                    Err(stack.error(b"live actor updates require a live song runtime"))
                });
            }
            let Some(session) = store
                .scenes
                .get(&screen.id)
                .and_then(|s| s.song_session.as_ref())
                .filter(|s| s.activated)
            else {
                drop(store);
                return vm.with_native_stack(|stack| {
                    Err(stack.error(b"live actor updates require activated song layers"))
                });
            };
            (session.roots, store.next)
        };
        let entry = before_enter.unwrap_or(Value::Nil);
        if !matches!(entry, Value::Nil | Value::Function(_) | Value::Native(_)) {
            return vm.with_native_stack(|stack| {
                Err(stack.error(b"actor entry callback must be a function"))
            });
        }
        let result = (|| {
            let catalog = vm.rooted(catalog)?;
            let callback = vm.new_host_function_with_data(
                advance,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(root),
                    entry,
                    catalog,
                ],
                Advance {
                    store: self.store.clone(),
                    delta,
                    statistics: Some(statistics),
                },
            )?;
            vm.call(callback, &[]).map(|_| true)
        })();
        song_activation::adopt_created(&self.store, screen.id, first);
        result
    }
}

fn advance(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Advance {
        store,
        delta,
        statistics,
    } = call.function_data::<Advance>().unwrap().clone();
    let root = table(call, call.captures()[2])?;
    let plan = build(call, &store, root, statistics.is_some())?;
    let mut deltas = std::mem::take(&mut *plan.scratch.lock().unwrap());
    deltas.resize(plan.actor_count, Deltas::default());
    deltas.fill(Deltas::default());
    let prior = {
        let mut guard = store.lock().unwrap();
        guard.replay_clock.as_mut().and_then(|clock| {
            clock.orders = plan.orders.clone();
            clock.phase.take()
        })
    };
    let result = call.with_roots(&[Value::Table(plan.roots)], |call| {
        for (&job, &(slot, parent)) in plan.jobs.iter().zip(&plan.slots) {
            let actor = job.actor();
            match job {
                Job::Enter(_, _, source) => {
                    if statistics.is_some() && call.captures()[3] != Value::Nil {
                        call.call(call.captures()[3], &[Value::Table(actor)])?;
                    }
                    let (mut incoming, mut span) =
                        parent.map_or((Some(delta), Some(delta as f32)), |p| match source {
                            DeltaSource::Children => deltas[p].children,
                            DeltaSource::Wrappers => deltas[p].wrappers,
                            DeltaSource::Columns => deltas[p].input,
                        });
                    if truthy(raw(call, actor, "__songlua_live_inactive")?) {
                        incoming = None;
                        span = None;
                    } else {
                        let own = raw(call, actor, "__songlua_live_delta")?;
                        if own != Value::Nil {
                            let own = sprites::number(call, own)?;
                            incoming = Some(own);
                            span = Some(own as f32);
                        }
                    }
                    let input = (incoming, span);
                    let awake = incoming
                        .map(|d| hibernate(call, &store, actor, d))
                        .transpose()?
                        .flatten();
                    let span = match (incoming, awake, span) {
                        (Some(i), Some(a), Some(s)) if i > 0. => Some(s * (a as f32 / i as f32)),
                        (_, Some(_), Some(_)) => Some(0.),
                        _ => None,
                    };
                    deltas[slot] = Deltas {
                        input,
                        wrappers: (awake, span),
                        children: (None, None),
                    };
                }
                Job::Texture(_) => {
                    if let Some(d) = deltas[slot].input.0 {
                        models::advance_materials(&store, actor, d as f32);
                    }
                }
                Job::PlayerFeedback(_) => {
                    let catalog = table(call, call.captures()[4])?;
                    live_feedback::update(
                        call,
                        &store,
                        statistics.as_ref().unwrap(),
                        catalog,
                        actor,
                    )?;
                }
                Job::Transition(_) => {
                    if deltas[slot].children.0.is_some() {
                        let state = transitions::state(call, actor)?
                            .ok_or_else(|| call.error(b"transition state expected"))?;
                        if state != 1 {
                            deltas[slot].children.0 = None;
                        } else if let Some(child) = transitions::child(call, actor)?
                            && lifecycle::time_left(call, &store, child)? == 0.
                        {
                            let key = call.try_intern(b"__songlua_transition_state")?;
                            call.raw_table_set(actor, key, Value::Number(2.))?;
                        }
                    }
                }
                Job::Advance(_, order, recurring) => {
                    let rate = raw(call, actor, "__songlua_update_rate")?;
                    let rate = if rate == Value::Nil {
                        store
                            .lock()
                            .unwrap()
                            .actors
                            .get(&actor)
                            .map_or(1., |a| a.update_rate)
                    } else {
                        sprites::number(call, rate)? as f32
                    };
                    deltas[slot].children = (
                        deltas[slot].wrappers.0.map(|d| f64::from(d as f32 * rate)),
                        deltas[slot].wrappers.1,
                    );
                    if let Some(d) = deltas[slot].children.0 {
                        phase(&store, order, d as f32);
                        if store.lock().unwrap().actors.contains_key(&actor) {
                            effects::advance(call, &store, actor, d as f32)?;
                            lifecycle::advance_queue(
                                call,
                                &store,
                                actor,
                                d as f32,
                                deltas[slot].children.1.unwrap_or(d as f32),
                            )?;
                            capture::advance_ready(call, &store, actor)?;
                            if !recurring {
                                recurring_update(call, &store, actor, d as f32)?;
                            }
                        }
                    }
                }
                Job::Recurring(_, order) => {
                    if let Some(d) = deltas[slot].children.0 {
                        phase(&store, order, d as f32);
                        recurring_update(call, &store, actor, d as f32)?;
                    }
                }
                Job::Callback(_, order) => {
                    if let Some(d) = deltas[slot].children.0 {
                        phase(&store, order, d as f32);
                        callback(call, &store, actor, d)?;
                        #[cfg(feature = "test-support")]
                        live_observation::observe(call, actor)?;
                    }
                }
            }
        }
        Ok(())
    });
    if let Some(clock) = &mut store.lock().unwrap().replay_clock {
        clock.phase = prior;
    }
    *plan.scratch.lock().unwrap() = deltas;
    result
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn phase(store: &Shared, order: usize, delta: f32) {
    if let Some(clock) = &mut store.lock().unwrap().replay_clock {
        clock.enter(order, delta);
    }
}
fn hibernate(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f64,
) -> Result<Option<f64>, Error> {
    if truthy(raw(call, actor, "__songlua_theme_hibernating")?) {
        return Ok(None);
    }
    let left = raw(call, actor, "__songlua_hibernate_seconds")?;
    let left = if left == Value::Nil {
        store
            .lock()
            .unwrap()
            .actors
            .get(&actor)
            .map_or(0., |a| a.hibernate)
    } else {
        sprites::number(call, left)? as f32
    };
    if left <= 0. || left.is_nan() {
        return Ok(Some(f64::from(delta as f32)));
    }
    let left = left - delta as f32;
    let key = call.try_intern(b"__songlua_hibernate_seconds")?;
    call.raw_table_set(actor, key, Value::Number(f64::from(left.max(0.))))?;
    if let Some(actor) = store.lock().unwrap().actors.get_mut(&actor) {
        actor.hibernate = left.max(0.);
    }
    if left <= 0. {
        song_immediate::record(
            call,
            store,
            actor,
            crate::SongLuaOverlayUpdateTarget::Hibernating,
            crate::SongLuaOverlayUpdateValue::Bool(false),
        );
    }
    Ok((left <= 0.).then(|| f64::from(-left)))
}
fn recurring_update(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f32,
) -> Result<(), Error> {
    let consumed = store
        .lock()
        .unwrap()
        .replay_clock
        .as_ref()
        .is_some_and(|c| c.consumed_recurring(actor));
    if !consumed {
        recurring::advance(call, store, actor, delta)?;
    }
    Ok(())
}
fn callback(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f64,
) -> Result<(), Error> {
    let pending = truthy(raw(call, actor, "__songlua_update_pending")?);
    let function = raw(
        call,
        actor,
        if pending {
            "__songlua_update_previous"
        } else {
            "__songlua_update_function"
        },
    )?;
    if function == Value::Nil {
        return Ok(());
    }
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"actor update function must be a function"));
    }
    let result = call.with_roots(&[function], |call| {
        run_callback(
            call,
            store,
            actor,
            function,
            Some(Value::Number(delta)),
            true,
        )
    });
    let drain = capture::drain_commands(call, store, actor);
    if let Err(error) = result {
        diagnostics::report(call, store, actor, diagnostics::Callback::Update, error)?;
    }
    if let Err(error) = drain {
        diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error)?;
    }
    Ok(())
}

impl Actors {
    pub(in crate::rust_engine) fn install_compiler_materials(
        &self,
        vm: &mut Vm,
        materials: Vec<(TableId, Vec<Option<crate::model_texture::Clock>>)>,
    ) -> Result<bool, String> {
        let active = !materials.is_empty();
        {
            let mut store = self.store.lock().unwrap();
            if materials
                .iter()
                .any(|(id, _)| !store.actors.contains_key(id))
            {
                return Err(
                    "model material owner has been released or belongs to another runtime".into(),
                );
            }
            for actor in store.actors.values_mut() {
                actor.model_materials.clear();
            }
            for (id, clocks) in materials {
                store.actors.get_mut(&id).unwrap().model_materials = clocks;
            }
        }
        let function = vm
            .new_host_function_with_data(
                |call| {
                    let store = call.function_data::<Shared>().unwrap().clone();
                    lifecycle::invalidate_plans(call, &store)
                },
                &[Value::Table(self.registry), Value::Table(self.helpers)],
                self.store.clone(),
            )
            .map_err(|e| e.to_string())?;
        vm.call(function, &[]).map_err(|e| e.to_string())?;
        Ok(active)
    }

    pub(in crate::rust_engine) fn multitap_update_only(
        &self,
        vm: &mut Vm,
        root: Value,
    ) -> Result<bool, Error> {
        let function = vm.new_host_function_with_data(
            update_only,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                root,
            ],
            self.store.clone(),
        )?;
        Ok(vm.call(function, &[])?[0] == Value::Boolean(true))
    }

    pub(in crate::rust_engine) fn multitap_dummy(&self, vm: &mut Vm) -> Result<Root, Error> {
        let function = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let actor = offline::dummy(call, &store, "Model")?;
                call.push(Value::Table(actor))
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        let value = vm.call(function, &[])?[0];
        vm.pin(value)
    }
}

fn raw(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(actor, key)
}
fn function(call: &HostCall<'_>, value: Value) -> Result<bool, Error> {
    match value {
        Value::Nil => Ok(false),
        Value::Function(_) | Value::Native(_) => Ok(true),
        _ => Err(call.error(b"function expected")),
    }
}
fn retain(call: &mut HostCall<'_>, plan: &Plan, actor: TableId) -> Result<(), Error> {
    call.raw_table_set(plan.roots, Value::Table(actor), Value::Boolean(true))
}
fn sequence(
    call: &mut HostCall<'_>,
    value: Value,
    mut visit: impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let array = table(call, value)?;
        for i in 1usize.. {
            let value = call.raw_table_get(array, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            let actor = table(call, value)?;
            call.with_roots(&[value], |call| visit(call, actor))?;
        }
        Ok(())
    })
}
fn collect_self(
    call: &mut HostCall<'_>,
    store: &Shared,
    plan: &mut Plan,
    actor: TableId,
    parent: Option<TableId>,
    source: DeltaSource,
) -> Result<(), Error> {
    retain(call, plan, actor)?;
    plan.jobs.push(Job::Enter(actor, parent, source));
    let wrappers = sprites::field(call, actor, "__songlua_wrappers")?;
    if wrappers != Value::Nil {
        sequence(call, wrappers, |call, wrapper| {
            collect(
                call,
                store,
                plan,
                wrapper,
                Some(actor),
                DeltaSource::Wrappers,
            )
        })?;
    } else {
        // Owned wrappers need no exported Lua array unless the author replaces it.
        let wrappers = store
            .lock()
            .unwrap()
            .actors
            .get(&actor)
            .map(|a| a.wrappers.clone())
            .unwrap_or_default();
        for wrapper in wrappers {
            collect(
                call,
                store,
                plan,
                wrapper,
                Some(actor),
                DeltaSource::Wrappers,
            )?;
        }
    }
    let recurring = sprites::field(call, actor, "__songlua_recurring_update_command")?;
    let recurring = if recurring == Value::Nil {
        capture::has_recurring(store, actor)
    } else {
        initial_state::value_text(call, recurring)?.is_some()
    };
    plan.counter += 1;
    Arc::make_mut(&mut plan.orders).insert(actor, plan.counter);
    plan.jobs.push(Job::Advance(actor, plan.counter, recurring));
    if recurring {
        plan.jobs.push(Job::Recurring(actor, plan.counter));
    }
    let transition = raw(call, actor, "__songlua_transition_state")?;
    if transition != Value::Nil {
        let read = call.new_host_function(
            |call| {
                let value = call.captures()[0];
                let mut stack = call.native_stack()?;
                stack.push(value)?;
                let integer = stack
                    .to_integer(-1)
                    .ok_or_else(|| stack.error(b"integer expected"))?;
                if u8::try_from(integer).is_err() {
                    return Err(stack.error(b"transition state out of range"));
                }
                stack.finish(0)
            },
            &[transition],
        )?;
        call.call(read, &[])?;
        plan.jobs.push(Job::Transition(actor));
    }
    Ok(())
}
fn collect(
    call: &mut HostCall<'_>,
    store: &Shared,
    plan: &mut Plan,
    actor: TableId,
    parent: Option<TableId>,
    source: DeltaSource,
) -> Result<(), Error> {
    collect_self(call, store, plan, actor, parent, source)?;
    for i in 1usize.. {
        let child = call.raw_table_get(actor, Value::Number(i as f64))?;
        if child == Value::Nil {
            break;
        }
        if let Value::Table(child) = child {
            call.with_roots(&[Value::Table(child)], |call| {
                native_children::adopt_declared(call, store, child, actor)?;
                collect(call, store, plan, child, Some(actor), DeltaSource::Children)
            })?;
        }
    }
    if store.lock().unwrap().actors.contains_key(&actor)
        && let Some(stream) = song_meters::stream(call, store, actor)?
    {
        collect(
            call,
            store,
            plan,
            stream,
            Some(actor),
            DeltaSource::Children,
        )?;
    }
    if store
        .lock()
        .unwrap()
        .actors
        .get(&actor)
        .is_some_and(|a| a.model_materials.iter().any(Option::is_some))
    {
        plan.jobs.push(Job::Texture(actor));
    }
    plan.jobs.push(Job::Callback(actor, plan.counter));
    let columns = raw(call, actor, "__songlua_note_columns")?;
    if columns != Value::Nil {
        sequence(call, columns, |call, child| {
            collect(call, store, plan, child, Some(actor), DeltaSource::Columns)
        })?;
    }
    Ok(())
}
fn build(
    call: &mut HostCall<'_>,
    store: &Shared,
    root: TableId,
    live: bool,
) -> Result<Arc<Plan>, Error> {
    let cached = store.lock().unwrap().multitap_plan.clone();
    if let Some(plan) = cached.as_ref().filter(|p| p.live == live) {
        return Ok(plan.clone());
    }
    if cached.is_some() {
        lifecycle::invalidate_plans(call, store)?;
    }
    let roots = call.try_new_table()?;
    call.with_roots(&[Value::Table(roots)], |call| {
        let mut plan = Plan {
            roots,
            jobs: Vec::new(),
            orders: Arc::new(HashMap::new()),
            counter: 0,
            slots: Vec::new(),
            actor_count: 0,
            scratch: Mutex::new(Vec::new()),
            live,
        };
        let globals = table(call, helper(call, 5)?)?;
        let top = raw(call, globals, "__songlua_top_screen")?;
        let mut after = Vec::new();
        if top != Value::Nil {
            let screen = table(call, top)?;
            collect_self(call, store, &mut plan, screen, None, DeltaSource::Children)?;
            let children = native_children::named_children(call, screen)?;
            call.with_roots(&[children], |call| {
                let children = table(call, children)?;
                for name in ["Underlay", "Overlay", "Debug"] {
                    let actor = raw(call, children, name)?;
                    if actor != Value::Nil {
                        let actor = table(call, actor)?;
                        retain(call, &plan, actor)?;
                        collect(call, store, &mut plan, actor, None, DeltaSource::Children)?;
                    }
                }
                for name in ["In", "Out", "Cancel"] {
                    let actor = raw(call, children, name)?;
                    if actor != Value::Nil {
                        let actor = table(call, actor)?;
                        retain(call, &plan, actor)?;
                        after.push(actor);
                    }
                }
                Ok(())
            })?;
            sequence(call, top, |call, actor| {
                let role = raw(call, actor, "__songlua_screen_actor_role")?;
                if initial_state::value_text(call, role)?.is_some() {
                    collect(call, store, &mut plan, actor, None, DeltaSource::Children)?;
                }
                Ok(())
            })?;
        }
        for key in [
            "__songlua_top_screen_player_1",
            "__songlua_top_screen_player_2",
        ] {
            let player = raw(call, globals, key)?;
            if player != Value::Nil {
                let player = table(call, player)?;
                retain(call, &plan, player)?;
                collect(call, store, &mut plan, player, None, DeltaSource::Children)?;
                if live {
                    plan.jobs.push(Job::PlayerFeedback(player));
                }
            }
        }
        collect(call, store, &mut plan, root, None, DeltaSource::Children)?;
        for actor in after {
            collect(call, store, &mut plan, actor, None, DeltaSource::Children)?;
        }
        if top != Value::Nil {
            plan.jobs
                .push(Job::Callback(table(call, top)?, plan.counter));
        }
        let mut actors = HashMap::new();
        for &job in &plan.jobs {
            let next = actors.len();
            let slot = *actors.entry(job.actor()).or_insert(next);
            let parent = if let Job::Enter(_, Some(parent), _) = job {
                Some(actors[&parent])
            } else {
                None
            };
            plan.slots.push((slot, parent));
        }
        plan.actor_count = actors.len();
        let registry = table(call, call.captures()[0])?;
        call.raw_table_set(registry, Value::Table(roots), Value::Boolean(true))?;
        let plan = Arc::new(plan);
        store.lock().unwrap().multitap_plan = Some(plan.clone());
        Ok(plan)
    })
}
fn update_only(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let Value::Table(root) = call.captures()[2] else {
        return call.push(Value::Boolean(false));
    };
    let plan = build(call, &store, root, false)?;
    let eligible = call.with_roots(&[Value::Table(plan.roots)], |call| {
        // Preserve the native two passes, including errors after extra callbacks.
        for job in &plan.jobs {
            if let Job::Advance(actor, _, _) = *job {
                let sleep = raw(call, actor, "__songlua_hibernate_seconds")?;
                if sleep != Value::Nil && sprites::number(call, sleep)? as f32 > 0. {
                    return Ok(false);
                }
                let deferred = raw(call, actor, "__songlua_command_queue_deferred")?;
                if if deferred == Value::Nil {
                    capture::deferred(&store, actor)
                } else {
                    !matches!(deferred, Value::Boolean(false))
                } {
                    return Ok(false);
                }
            }
        }
        let mut active = Vec::new();
        for job in &plan.jobs {
            match *job {
                Job::Advance(..)
                | Job::Enter(..)
                | Job::Texture(_)
                | Job::Transition(_)
                | Job::PlayerFeedback(_) => (),
                Job::Recurring(..) => active.push(None),
                Job::Callback(actor, _) => {
                    let current = sprites::field(call, actor, "__songlua_update_function")?;
                    if function(call, current)? {
                        active.push(Some(actor));
                    } else {
                        let previous = sprites::field(call, actor, "__songlua_update_previous")?;
                        if function(call, previous)? {
                            active.push(Some(actor));
                        }
                    }
                }
            }
        }
        match active.as_slice() {
            [] => Ok(true),
            [Some(actor)] => {
                let name = sprites::field(call, *actor, "Name")?;
                Ok(initial_state::value_text(call, name)?.as_deref() == Some("Update"))
            }
            _ => Ok(false),
        }
    })?;
    call.push(Value::Boolean(eligible))
}
