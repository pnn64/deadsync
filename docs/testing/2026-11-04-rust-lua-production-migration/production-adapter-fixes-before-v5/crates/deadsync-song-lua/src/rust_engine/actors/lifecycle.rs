//! Existing actor queue and frame lifecycle, owned by the song host.
use super::*;
mod snapshot;
pub(super) use snapshot::{cached_for_root, callback_jobs, plan_roots};

#[derive(Clone)]
pub(super) struct TweenStep {
    index: Option<usize>,
    duration: f32,
    left: f32,
    from: Option<TweenPose>,
    pub(super) to: TweenPose,
    easing: String,
    opt1: Option<f32>,
    command: Option<Vec<u8>>,
}

impl TweenStep {
    pub(super) fn startup(
        index: usize,
        duration: f32,
        to: TweenPose,
        easing: Option<String>,
        opt1: Option<f32>,
        command: Option<Vec<u8>>,
    ) -> Self {
        Self {
            index: Some(index),
            duration,
            left: duration,
            from: None,
            to,
            easing: easing.unwrap_or_default(),
            opt1,
            command,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Lifecycle {
    Ease(&'static str),
    Tween,
    Sleep,
    Hibernate,
    Stop,
    Finish,
    Hurry,
    TimeLeft,
    Queue,
    QueueMessage,
    Update,
    Rate,
    GetRate,
}

pub(super) const ACTOR_METHODS: &[(&str, Lifecycle)] = &[
    ("linear", Lifecycle::Ease("linear")),
    ("accelerate", Lifecycle::Ease("inQuad")),
    ("decelerate", Lifecycle::Ease("outQuad")),
    ("smooth", Lifecycle::Ease("smooth")),
    ("spring", Lifecycle::Ease("spring")),
    ("bouncebegin", Lifecycle::Ease("bouncebegin")),
    ("bounceend", Lifecycle::Ease("bounceend")),
    ("tween", Lifecycle::Tween),
    ("sleep", Lifecycle::Sleep),
    ("hibernate", Lifecycle::Hibernate),
    ("stoptweening", Lifecycle::Stop),
    ("finishtweening", Lifecycle::Finish),
    ("hurrytweening", Lifecycle::Hurry),
    ("GetTweenTimeLeft", Lifecycle::TimeLeft),
    ("queuecommand", Lifecycle::Queue),
    ("queuemessage", Lifecycle::QueueMessage),
];
pub(super) const FRAME_METHODS: &[(&str, Lifecycle)] = &[
    ("SetUpdateFunction", Lifecycle::Update),
    ("SetUpdateRate", Lifecycle::Rate),
    ("GetUpdateRate", Lifecycle::GetRate),
];

pub(super) fn finish(call: &mut HostCall<'_>, store: &Shared, id: TableId) -> Result<(), Error> {
    action_capture::prepare(call, store, id)?;
    {
        let mut store = store.lock().unwrap();
        if let Some(clock) = &mut store.replay_clock {
            clock.reset(id);
        }
        let actor = store.actors.get_mut(&id).unwrap();
        if !capture::cancel(actor, true) {
            if let Some(last) = actor.queue.back() {
                actor.current = last.to;
            }
            actor.queue.clear();
            actor.sync_rotation_getters();
        }
    }
    song_immediate::cancel(call, store, id, true)?;
    song_messages::reset_timing(call, store, id)?;
    song_replay::sync_getters(call, store, id)?;
    // Finish also visits native named children. Hurry, TimeLeft and ordinary
    // frame updates retain the production traversal of authored children only.
    for child in children(call, store, id)? {
        finish(call, store, child)?;
    }
    Ok(())
}

fn hurry(store: &mut Store, id: TableId, factor: f32) {
    if let Some(clock) = &mut store.replay_clock {
        clock.hurry(id, factor);
    }
    let actor = store.actors.get_mut(&id).unwrap();
    for step in &mut actor.queue {
        step.duration *= factor;
        step.left *= factor;
    }
    if let Some(capture) = &mut actor.capture {
        capture.hurry(factor);
    }
    for child in actor.children.clone() {
        hurry(store, child, factor);
    }
}

pub(super) fn time_left(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
) -> Result<f32, Error> {
    call.with_roots(&[Value::Table(id)], |call| {
        if let Some(state) = transitions::state(call, id)? {
            return if state == 1 {
                match transitions::child(call, id)? {
                    Some(child) => time_left(call, store, child),
                    None => Ok(0.),
                }
            } else {
                Ok(0.)
            };
        }
        let theme_hibernating = !matches!(
            get(call, id, b"__songlua_theme_hibernating")?,
            Value::Nil | Value::Boolean(false)
        );
        let native = {
            let store = store.lock().unwrap();
            store.actors.get(&id).map(|actor| {
                let hibernate = if theme_hibernating {
                    f32::INFINITY
                } else {
                    actor.hibernate
                };
                (
                    hibernate,
                    actor.capture.as_ref().map_or_else(
                        || {
                            actor
                                .queue
                                .iter()
                                .fold(hibernate, |left, step| left + step.left)
                        },
                        |capture| hibernate + capture.time_left.max(0.),
                    ),
                )
            })
        };
        let (hibernate, mut left) = if let Some(native) = native {
            native
        } else {
            let value = get(call, id, b"__songlua_hibernate_seconds")?;
            let hibernate = if theme_hibernating {
                f32::INFINITY
            } else if value == Value::Nil {
                0.
            } else {
                sprites::number(call, value)? as f32
            };
            let value = sprites::field(call, id, "__songlua_capture_tween_time_left")?;
            let left = if value == Value::Nil {
                0.
            } else {
                (sprites::number(call, value)? as f32).max(0.)
            };
            (hibernate, hibernate + left)
        };
        let mut index = 1;
        loop {
            let value = call.raw_table_get(id, Value::Number(index as f64))?;
            if value == Value::Nil {
                break;
            }
            if let Value::Table(child) = value {
                let child = hibernate + time_left(call, store, child)?;
                if left < child {
                    left = child;
                }
            }
            index += 1;
        }
        Ok(left)
    })
}

fn queue(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    exact_duration: f64,
    easing: &str,
    command: Option<Vec<u8>>,
) -> Result<(), Error> {
    let duration = exact_duration as f32;
    let shared = store;
    action_capture::prepare(call, store, id)?;
    capture::enter_replay_queue(store, id);
    // BeginTweening finishes the actor subtree before adding its 52nd state.
    let count = {
        let guard = store.lock().unwrap();
        let actor = &guard.actors[&id];
        guard
            .replay_clock
            .as_ref()
            .and_then(|clock| clock.queue_len(id))
            .unwrap_or_else(|| {
                actor
                    .capture
                    .as_ref()
                    .map_or(actor.queue.len(), capture::Capture::pending_steps)
            })
    };
    if count > 50 {
        finish(call, store, id)?;
    }
    let mut store = store.lock().unwrap();
    if let Some(clock) = &mut store.replay_clock {
        clock.tween(
            id,
            if command.is_some() {
                None
            } else {
                Some(exact_duration)
            },
        );
    }
    let queued = store.capture_scope.is_some();
    let replaying = store.replay_frame.is_some();
    let actor = store.actors.get_mut(&id).unwrap();
    if replaying {
        actor.replay.get_or_insert_with(song_replay::State::default);
    }
    let index = actor.replay.as_mut().map(|replay| {
        let index = replay.progress.len();
        replay.progress.push(Vec::new());
        replay.capture_open = command.is_none();
        index
    });
    if let Some(capture) = &mut actor.capture {
        capture.begin_tween(duration, easing, queued);
        if !actor.active_commands.is_empty() {
            capture.exact_step(exact_duration);
        }
    }
    let to = *actor.destination();
    let command_step = command.is_some();
    actor.queue.push_back(TweenStep {
        index,
        duration,
        left: duration,
        from: None,
        to,
        easing: easing.to_owned(),
        opt1: None,
        command,
    });
    drop(store);
    if replaying {
        song_messages::timing(
            call,
            id,
            if command_step { 0. } else { duration },
            (!command_step).then_some(easing),
        )?;
    }
    if !command_step {
        // Native BeginTweening owns an empty destination capture block too.
        super::ease_probe::actor(call, shared, id)?;
    }
    Ok(())
}

fn duration(call: &HostCall<'_>, value: Value, sleep: bool) -> f64 {
    // The production numeric helpers parse string durations as Rust floats,
    // but reject nonfinite numeric values (Sleep retains positive infinity).
    let value = match value {
        Value::Number(value) if value.is_finite() || (sleep && value == f64::INFINITY) => {
            Some(value)
        }
        Value::String(id) => call
            .string_bytes(id)
            .and_then(|text| std::str::from_utf8(text).ok())
            .and_then(|text| text.trim().parse::<f64>().ok()),
        _ => None,
    };
    value.unwrap_or(0.0).max(0.0)
}

fn native_tween(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let duration = call.number(1)? as f32;
    if duration < 0.0 {
        return Ok(());
    }
    if duration.is_nan() {
        return Err(call.error(b"invalid tween duration"));
    }
    let names = [
        "TweenType_Linear",
        "TweenType_Accelerate",
        "TweenType_Decelerate",
        "TweenType_Spring",
        "TweenType_Bezier",
    ];
    let kind = match arg(call, 2) {
        Value::Number(index) if (0.0..5.0).contains(&index) && index.fract() == 0.0 => {
            Some(index as usize)
        }
        Value::String(id) => call
            .string_bytes(id)
            .and_then(|name| std::str::from_utf8(name).ok())
            .and_then(|name| {
                names.iter().position(|full| {
                    name == *full || name.eq_ignore_ascii_case(full.split_once('_').unwrap().1)
                })
            }),
        _ => None,
    }
    .ok_or_else(|| call.error(b"Expected TweenType"))?;
    let easing = if kind == 4 {
        let controls = table(call, arg(call, 3))?;
        let count = call.raw_table_len(controls)?;
        if count != 4 && count != 8 {
            return Ok(());
        }
        let mut points = Vec::with_capacity(count);
        for index in 1..=count {
            let value = call.raw_table_get(controls, Value::Number(index as f64))?;
            let value = call.call(helper(call, 6)?, &[value])?[0];
            let value = if let Value::Number(value) = value {
                value as f32
            } else {
                0.0
            };
            points.push(value.to_string());
        }
        format!("bezier({})", points.join(","))
    } else {
        ["linear", "inQuad", "outQuad", "spring"][kind].to_owned()
    };
    queue(call, store, actor, f64::from(duration), &easing, None)
}

pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    op: Lifecycle,
) -> Result<(), Error> {
    if matches!(op, Lifecycle::Update | Lifecycle::Rate | Lifecycle::GetRate)
        && !is_frame(store, actor)
    {
        return Err(call.error(b"ActorFrame expected"));
    }
    match op {
        Lifecycle::Ease(easing) => {
            let duration = duration(call, arg(call, 1), false);
            queue(call, store, actor, duration, easing, None)?;
        }
        Lifecycle::Tween => native_tween(call, store, actor)?,
        Lifecycle::Sleep => {
            let duration = duration(call, arg(call, 1), true);
            queue(call, store, actor, duration, "linear", None)?;
            queue(call, store, actor, 0.0, "linear", None)?;
        }
        Lifecycle::Hibernate => {
            let key = call.try_intern(b"__songlua_theme_hibernating")?;
            call.raw_table_set(actor, key, Value::Boolean(false))?;
            let duration = float_scalar(call, arg(call, 1), true).unwrap_or(0.0);
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .hibernate = duration;
            let key = call.try_intern(b"__songlua_hibernate_seconds")?;
            call.raw_table_set(actor, key, Value::Number(duration as f64))?;
            action_capture::prepare_write(call, store, actor)?;
            capture::write(
                call,
                store,
                actor,
                crate::SongLuaOverlayStateDelta {
                    hibernating: Some(duration > 0.),
                    ..Default::default()
                },
                true,
            );
        }
        Lifecycle::Stop => {
            action_capture::prepare(call, store, actor)?;
            {
                let mut store = store.lock().unwrap();
                let fallback = song_pose::mode(&store, actor);
                let prior = song_pose::prior_position(&store, actor);
                if let Some(clock) = &mut store.replay_clock {
                    clock.reset(actor);
                }
                let actor = store.actors.get_mut(&actor).unwrap();
                if fallback && let Some(prior) = prior {
                    let current = actor.current;
                    actor.preparation_getters.get_or_insert([
                        current.x,
                        current.y,
                        current.z,
                        current.aux,
                    ])[..3]
                        .copy_from_slice(&prior);
                }
                if !capture::cancel(actor, false) {
                    actor.queue.clear();
                    actor.sync_rotation_getters();
                }
                // Fallback Stop drops the destination position. Other sampled
                // destination fields remain independent of its physical queue.
                if fallback
                    && let Some(getters) = actor
                        .replay
                        .as_mut()
                        .and_then(|replay| replay.getters.as_mut())
                {
                    [getters.x, getters.y, getters.z] =
                        [actor.current.x, actor.current.y, actor.current.z];
                }
            }
            song_immediate::cancel(call, store, actor, false)?;
            song_messages::reset_timing(call, store, actor)?;
            song_replay::sync_getters(call, store, actor)?;
        }
        Lifecycle::Finish => finish(call, store, actor)?,
        Lifecycle::Hurry => {
            let factor = call.number(1)? as f32;
            if factor < 0.0 {
                return Err(call.error(b"hurrytweening factor must not be negative"));
            }
            action_capture::prepare(call, store, actor)?;
            hurry(&mut store.lock().unwrap(), actor, factor);
            song_messages::hurry_timing(call, store, actor, factor)?;
        }
        Lifecycle::TimeLeft => {
            let left = time_left(call, store, actor)?;
            return call.push(Value::Number(left as f64));
        }
        Lifecycle::Queue | Lifecycle::QueueMessage => {
            if let Some(mut name) = command_name(call, arg(call, 1)) {
                if matches!(op, Lifecycle::QueueMessage) {
                    name.insert(0, b'!');
                }
                if !capture::queue(call, store, actor, name.clone())? {
                    queue(call, store, actor, 0.0, "linear", Some(name))?;
                }
            }
        }
        Lifecycle::Update => {
            let function = arg(call, 1);
            if !matches!(function, Value::Nil | Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"SetUpdateFunction requires a function or nil"));
            }
            let key = call.try_intern(b"__songlua_update_function")?;
            call.table_set(Value::Table(actor), key, function)?;
            invalidate_plans(call, store)?;
        }
        Lifecycle::Rate => {
            let rate = call.number(1)? as f32;
            if rate <= 0.0 {
                return Err(call.error(b"Update rate must be greater than 0"));
            }
            if rate > 0.0 {
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .update_rate = rate;
                invalidate_plans(call, store)?;
            }
        }
        Lifecycle::GetRate => {
            return call.push(Value::Number(
                store.lock().unwrap().actors[&actor].update_rate as f64,
            ));
        }
    }
    call.push(Value::Table(actor))
}

pub(super) fn start(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    if store.lock().unwrap().actors[&actor].started {
        return Ok(());
    }
    play(call, store, actor, b"Begin", None, true)?;
    startup(call, store, actor)
}

pub(super) fn end(call: &mut HostCall<'_>, store: &Shared, root: TableId) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = call.try_intern(b"__songlua_top_screen")?;
    call.raw_table_set(globals, key, Value::Nil)?;
    // The production live scene owns a synthetic root. End walks its ordinary
    // descendants in their current order, excluding that root and wrappers.
    // Commands may append tweens/queues, but End never advances them.
    for child in children(call, store, root)? {
        play(call, store, child, b"End", None, true)?;
    }
    Ok(())
}

pub(super) fn startup(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return native_children::group_lifecycle(call, store, actor, true);
    }
    if store.lock().unwrap().actors[&actor].started {
        return Ok(());
    }
    run_named(call, store, actor, b"On", None)?;
    for child in declared_children(store, actor) {
        native_children::adopt_declared(call, store, child, actor)?;
        startup(call, store, child)?;
    }
    if let Some(stream) = song_meters::stream(call, store, actor)? {
        startup(call, store, stream)?;
    }
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .started = true;
    courses::publish_raw_flag(call, store, actor, "__songlua_startup_commands_ran")?;
    Ok(())
}

pub(super) fn advance_queue(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f32,
    span: f32,
) -> Result<(), Error> {
    let mut remaining = delta;
    // A meter stream hidden by an already initialized duplicate-name group
    // has never subscribed. Its stored queue/getters remain observable, but
    // production does not advance that queue without native registration.
    if !store.lock().unwrap().actors[&actor].registered {
        return Ok(());
    }
    if song_pose::mode(&store.lock().unwrap(), actor) {
        return song_pose::advance_position(call, store, actor);
    }
    let mut dispatched = 0;
    // Commands can replace the queue. Release metadata before entering Lua and
    // consume the original frame's remaining native float time afterwards.
    while remaining > 0.0 {
        let command = {
            let mut store = store.lock().unwrap();
            let sync = !store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
            let time = store.replay_frame.map_or(0., |(_, time)| time as f32);
            let actor = store.actors.get_mut(&actor).unwrap();
            let Some(step) = actor.queue.front_mut() else {
                break;
            };
            let beginning = step.from.is_none();
            if beginning {
                step.from = Some(actor.current);
                if let (Some(replay), Some(index)) = (&mut actor.replay, step.index) {
                    let start = if step.duration == 0. {
                        time
                    } else {
                        time - span * (remaining / delta)
                    };
                    replay.progress[index].push([start, step.duration]);
                }
            }
            let command = if beginning { step.command.take() } else { None };
            let used = remaining.min(step.left);
            step.left -= used;
            remaining -= used;
            if let (Some(replay), Some(index)) = (&mut actor.replay, step.index) {
                replay.progress[index].push([time, step.left]);
            }
            if step.left == 0.0 {
                actor.current = step.to;
                actor.queue.pop_front();
            } else {
                let factor = crate::overlay_command_ease_factor(
                    Some(&step.easing),
                    1.0 - step.left / step.duration,
                    step.opt1,
                );
                actor.current = step.from.unwrap().lerp(step.to, factor);
            }
            if sync {
                actor.sync_rotation_getters();
            }
            command
        };
        song_replay::sync_getters(call, store, actor)?;
        if let Some(command) = command {
            dispatched += 1;
            if dispatched > 10000 {
                return Err(call.error(b"nonterminating zero-time actor command queue"));
            }
            song_messages::reset_timing(call, store, actor)?;
            let previous = {
                let mut guard = store.lock().unwrap();
                let time = guard
                    .replay_frame
                    .map(|(_, time)| song_immediate::Dispatch {
                        seconds: Some(time),
                        delay: 0.,
                    });
                std::mem::replace(&mut guard.replay_dispatch, time)
            };
            let previous_clock = store
                .lock()
                .unwrap()
                .replay_clock
                .as_mut()
                .and_then(|clock| clock.dispatched(actor, remaining));
            #[cfg(feature = "test-support")]
            {
                store.lock().unwrap().audit_queued_depth += 1;
            }
            let result = if let Some(message) = command.strip_prefix(b"!") {
                messages::queued(call, store, message)
            } else {
                play(call, store, actor, &command, None, true)
            };
            #[cfg(feature = "test-support")]
            {
                store.lock().unwrap().audit_queued_depth -= 1;
            }
            store.lock().unwrap().replay_dispatch = previous;
            if let Some(clock) = &mut store.lock().unwrap().replay_clock {
                clock.dispatch = previous_clock;
            }
            result?;
        }
    }
    Ok(())
}

pub(super) struct FrameNode {
    actor: TableId,
    owned: bool,
    transition: bool,
    recurring: bool,
    texture: bool,
    wrappers: Vec<FrameNode>,
    children: Vec<FrameNode>,
    columns: Vec<FrameNode>,
    order: usize,
    callback_order: usize,
}
pub(super) struct Plan {
    roots: TableId,
    node: FrameNode,
    orders: Arc<HashMap<TableId, usize>>,
}
pub(super) fn invalidate_plans(call: &mut HostCall<'_>, store: &Shared) -> Result<(), Error> {
    let plans = std::mem::take(&mut store.lock().unwrap().update_plans);
    let registry = table(call, call.captures()[0])?;
    let multitap = store.lock().unwrap().multitap_plan.take();
    if let Some(plan) = multitap {
        call.raw_table_set(registry, Value::Table(plan.roots), Value::Nil)?;
    }
    for plan in plans.into_values() {
        call.raw_table_set(registry, Value::Table(plan.roots), Value::Nil)?;
    }
    Ok(())
}
fn plan(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    roots: TableId,
    synthetic_root: bool,
) -> Result<FrameNode, Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let owned = store.lock().unwrap().actors.contains_key(&actor);
        let (wrappers, children) = if owned {
            let store = store.lock().unwrap();
            let node = &store.actors[&actor];
            (node.wrappers.clone(), node.children.clone())
        } else {
            // Generated duplicate-name groups have no raw native pose or queue.
            // The cached traversal still owns their table and visits their array.
            call.raw_table_set(roots, Value::Table(actor), Value::Boolean(true))?;
            let children = if synthetic_root {
                // The loader owns a plain container outside the actor registry.
                // Native traversal visits its raw sequence and skips non-tables.
                let mut children = Vec::new();
                for index in 1usize.. {
                    match call.raw_table_get(actor, Value::Number(index as f64))? {
                        Value::Nil => break,
                        Value::Table(child) => children.push(child),
                        _ => {}
                    }
                }
                children
            } else {
                native_children::group_members(call, actor)?
            };
            (Vec::new(), children)
        };
        let wrappers = wrappers
            .into_iter()
            .map(|id| plan(call, store, id, roots, false))
            .collect::<Result<_, _>>()?;
        let recurring = match sprites::field(call, actor, "__songlua_recurring_update_command")? {
            Value::Nil => false,
            value => {
                let value = bytes(call, value)?;
                std::str::from_utf8(&value).map_err(|e| call.error(e.to_string().as_bytes()))?;
                true
            }
        };
        let transition = transitions::state(call, actor)?.is_some();
        let mut nodes = Vec::new();
        for child in children {
            native_children::adopt_declared(call, store, child, actor)?;
            nodes.push(plan(call, store, child, roots, false)?);
        }
        if owned && let Some(stream) = song_meters::stream(call, store, actor)? {
            nodes.push(plan(call, store, stream, roots, false)?);
        }
        let texture = store
            .lock()
            .unwrap()
            .actors
            .get(&actor)
            .is_some_and(|actor| actor.model_materials.iter().any(Option::is_some));
        let mut columns = Vec::new();
        for column in notefields::update_columns(call, actor)? {
            columns.push(plan(call, store, column, roots, false)?);
        }
        Ok(FrameNode {
            actor,
            owned,
            transition,
            recurring,
            texture,
            wrappers,
            children: nodes,
            columns,
            order: 0,
            callback_order: 0,
        })
    })
}

pub(super) fn advance(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    delta: f32,
) -> Result<(), Error> {
    // Production retains its traversal until an invalidating operation such as
    // SetUpdateFunction/SetUpdateRate. SortByDrawOrder alone does not invalidate
    // it. The current frame keeps its original plan even across invalidation;
    // callback values themselves are still resolved at the time of invocation.
    let plan = retained_plan(call, store, actor)?;
    // An authored callback may invalidate the cache and collect while this
    // frame still uses the original plan's proxy groups.
    let prior_phase = {
        let mut guard = store.lock().unwrap();
        guard.replay_clock.as_mut().and_then(|clock| {
            clock.orders = plan.orders.clone();
            clock.phase.take()
        })
    };
    let result = call.with_roots(&[Value::Table(plan.roots)], |call| {
        advance_node(call, store, &plan.node, delta, delta)
    });
    if let Some(clock) = &mut store.lock().unwrap().replay_clock {
        clock.phase = prior_phase;
    }
    result
}

fn assign_orders(node: &mut FrameNode, counter: &mut usize, orders: &mut HashMap<TableId, usize>) {
    for wrapper in &mut node.wrappers {
        assign_orders(wrapper, counter, orders);
    }
    *counter += 1;
    node.order = *counter;
    orders.insert(node.actor, *counter);
    for child in &mut node.children {
        assign_orders(child, counter, orders);
    }
    node.callback_order = *counter;
    for column in &mut node.columns {
        assign_orders(column, counter, orders);
    }
}

fn enter_phase(store: &Shared, order: usize, delta: f32) {
    if let Some(clock) = &mut store.lock().unwrap().replay_clock {
        clock.enter(order, delta);
    }
}

fn advance_node(
    call: &mut HostCall<'_>,
    store: &Shared,
    node: &FrameNode,
    delta: f32,
    span: f32,
) -> Result<(), Error> {
    let actor = node.actor;
    let material_delta = delta;
    let material_span = span;
    let delta = if !node.owned {
        delta
    } else {
        let mut guard = store.lock().unwrap();
        let actor = guard.actors.get_mut(&actor).unwrap();
        if actor.hibernate > 0.0 {
            let left = actor.hibernate - delta;
            actor.hibernate = left.max(0.0);
            drop(guard);
            let key = call.try_intern(b"__songlua_hibernate_seconds")?;
            call.raw_table_set(node.actor, key, Value::Number(left.max(0.0) as f64))?;
            if left <= 0.0 {
                song_immediate::record(
                    call,
                    store,
                    node.actor,
                    crate::SongLuaOverlayUpdateTarget::Hibernating,
                    crate::SongLuaOverlayUpdateValue::Bool(false),
                );
                -left
            } else {
                models::advance_materials(store, node.actor, material_delta);
                for column in &node.columns {
                    advance_node(call, store, column, material_delta, material_span)?;
                }
                return Ok(());
            }
        } else {
            delta
        }
    };
    let span = if material_delta > 0. {
        span * (delta / material_delta)
    } else {
        0.
    };
    // Wrappers receive the owner's post-hibernation delta before its update
    // rate. Their callbacks can change that rate for this same frame.
    for wrapper in &node.wrappers {
        advance_node(call, store, wrapper, delta, span)?;
    }
    let delta = if node.owned {
        let delta = delta * store.lock().unwrap().actors[&actor].update_rate;
        enter_phase(store, node.order, delta);
        effects::advance(call, store, actor, delta)?;
        advance_queue(call, store, actor, delta, span)?;
        capture::advance_ready(call, store, actor)?;
        enter_phase(store, node.order, delta);
        let consumed = store
            .lock()
            .unwrap()
            .replay_clock
            .as_ref()
            .is_some_and(|clock| clock.consumed_recurring(actor));
        if !consumed {
            recurring::advance(call, store, actor, delta)?;
        }
        delta
    } else {
        delta
    };
    // Transition::UpdateInternal follows the owner's base queue, gates its
    // children and update callback, and tests completion before the final child
    // update. A cached plan retains whether this actor has a transition job.
    let children_active = if node.transition {
        let state = transitions::state(call, actor)?
            .ok_or_else(|| call.error(b"transition state expected"))?;
        if state != 1 {
            false
        } else {
            if let Some(child) = transitions::child(call, actor)?
                && time_left(call, store, child)? == 0.
            {
                sprites::set(call, actor, "__songlua_transition_state", Value::Number(2.))?;
            }
            true
        }
    } else {
        true
    };
    if children_active {
        for child in &node.children {
            advance_node(call, store, child, delta, span)?;
        }
    }
    models::advance_materials(store, actor, material_delta);
    enter_phase(store, node.callback_order, delta);
    let function = get(call, actor, b"__songlua_update_function")?;
    if children_active && function != Value::Nil {
        if !matches!(function, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"actor update function must be a function"));
        }
        let result = call.with_roots(&[function], |call| {
            run_actor_function(
                call,
                store,
                actor,
                function,
                Some(Value::Number(delta as f64)),
            )
        });
        let drain = capture::drain_commands(call, store, actor);
        if let Err(error) = result {
            diagnostics::report(call, store, actor, diagnostics::Callback::Update, error)?;
        }
        if let Err(error) = drain {
            diagnostics::report(call, store, actor, diagnostics::Callback::Queue, error)?;
        }
    }
    // NoteField advances its column vector after its frame callback, using the
    // incoming delta outside the frame's hibernation and update-rate gates.
    for column in &node.columns {
        advance_node(call, store, column, material_delta, material_span)?;
    }
    Ok(())
}

/// Even an actor paused by an ancestor contributes a hold sample for the
/// compiler's currently running tween. Unstarted queues have no progress yet.
pub(super) fn sample_replay(actor: &mut Actor, time: f32) {
    if let (Some(replay), Some(step)) = (&mut actor.replay, actor.queue.front())
        && let Some(index) = step.index
        && step.from.is_some()
        && replay.progress[index]
            .last()
            .is_none_or(|sample| sample[0] != time)
    {
        replay.progress[index].push([time, step.left]);
    }
}

/// Message writes keep the delay of the real existing queue, including sleep
/// and hibernation; it cannot be reconstructed from the last command alone.
pub(super) fn message_delay(actor: &Actor, duration: f32) -> f32 {
    if duration > 0. {
        actor
            .queue
            .iter()
            .take(actor.queue.len().saturating_sub(1))
            .fold(actor.hibernate, |left, step| left + step.left)
    } else {
        0.
    }
}

pub(super) fn retained_plan(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<Arc<Plan>, Error> {
    let cached = store.lock().unwrap().update_plans.get(&actor).cloned();
    let plan = if let Some(plan) = cached {
        plan
    } else {
        let roots = call.try_new_table()?;
        let plan = call.with_roots(&[Value::Table(roots)], |call| {
            let mut node = plan(call, store, actor, roots, true)?;
            let mut orders = HashMap::new();
            assign_orders(&mut node, &mut 0, &mut orders);
            let registry = table(call, call.captures()[0])?;
            call.raw_table_set(registry, Value::Table(roots), Value::Boolean(true))?;
            Ok(Arc::new(Plan {
                roots,
                node,
                orders: Arc::new(orders),
            }))
        })?;
        store
            .lock()
            .unwrap()
            .update_plans
            .insert(actor, plan.clone());
        plan
    };
    Ok(plan)
}
