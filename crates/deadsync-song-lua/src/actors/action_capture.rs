//! Lazy actor state around the existing compiler touched-function probes.
use super::super::{SongActorFunctionBlocks, random::RandomState};
use super::*;

const KEYS: [&str; 3] = [
    crate::SONG_LUA_CAPTURE_ACTORS_KEY,
    crate::SONG_LUA_CAPTURE_ACTOR_SET_KEY,
    crate::SONG_LUA_CAPTURE_SNAPSHOTS_KEY,
];
pub(super) struct State {
    retained: TableId,
    actors: TableId,
    actor_set: TableId,
    snapshots: TableId,
    order: Vec<TableId>,
    seen: HashSet<TableId>,
    saved: Vec<(TableId, TableId, Option<function_capture::Saved>)>,
}
#[derive(Default)]
struct Output {
    previous: Option<State>,
    current: Option<State>,
    activated: bool,
    deactivated: bool,
    restored: bool,
    raw: bool,
    blocks: SongActorFunctionBlocks,
}
#[derive(Clone)]
struct Run {
    store: Shared,
    selected: Vec<(usize, TableId)>,
    output: Arc<Mutex<Output>>,
    action: Option<function_action::Scope>,
}
impl Actors {
    pub(crate) fn capture_touched_actor_function(
        &self,
        vm: &mut Vm,
        actors: Vec<(usize, TableId)>,
        function: Value,
        arg: Option<f32>,
        random: &RandomState,
    ) -> (Result<SongActorFunctionBlocks, Error>, bool, Option<Root>) {
        self.capture_scoped_function(vm, actors, function, arg, random, None)
    }
    pub(super) fn capture_scoped_function(
        &self,
        vm: &mut Vm,
        actors: Vec<(usize, TableId)>,
        function: Value,
        arg: Option<f32>,
        random: &RandomState,
        action: Option<function_action::Scope>,
    ) -> (Result<SongActorFunctionBlocks, Error>, bool, Option<Root>) {
        let _broadcasts = song_capture::pause_preparation_broadcasts(&self.store);
        let output = Arc::new(Mutex::new(Output::default()));
        let mut returned = None;
        let result = (|| {
            let retained = vm.try_new_table()?;
            let pin = vm.pin(Value::Table(retained))?;
            let result = (|| {
                retained::attach_values(
                    vm,
                    retained,
                    actors.iter().map(|(_, actor)| Value::Table(*actor)),
                )?;
                let mut captures = [
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(retained),
                ]
                .to_vec();
                if let Some(action) = &action {
                    captures.push(Value::Table(action.retained));
                }
                let data = Run {
                    store: self.store.clone(),
                    selected: actors,
                    output: output.clone(),
                    action,
                };
                // Allocate cleanup callbacks before activating the probe. Their
                // allocation must not run GC between freezing touched actors and
                // restoring the outer scope's command locals/globals.
                let start = vm.new_host_function_with_data(begin, &captures, data.clone())?;
                vm.raw_table_set(retained, Value::Number(11.), start)?;
                let inactive =
                    vm.new_host_function_with_data(deactivate, &captures, data.clone())?;
                vm.raw_table_set(retained, Value::Number(12.), inactive)?;
                let finish = vm.new_host_function_with_data(finish, &captures, data)?;
                vm.raw_table_set(retained, Value::Number(13.), finish)?;
                vm.call(start, &[])?;
                let previous_locals = self.begin_command_locals();
                let random = random.preserve();
                let args = arg
                    .map(|n| Value::Number(f64::from(n)))
                    .into_iter()
                    .collect::<Vec<_>>();
                let result = vm
                    .call(function, &args)
                    .map(|v| v.first().copied().unwrap_or(Value::Nil));
                let result = result.and_then(|value| vm.pin(value).map(|pin| returned = Some(pin)));
                {
                    // Native reads touched actor identities and snapshots before
                    // restoring command locals. Later cleanup activity belongs
                    // outside that frozen set of probe results.
                    let mut guard = self.store.lock().unwrap();
                    let state = guard.action_capture.as_mut().unwrap();
                    let frozen = State {
                        retained: state.retained,
                        actors: state.actors,
                        actor_set: state.actor_set,
                        snapshots: state.snapshots,
                        order: state.order.clone(),
                        seen: state.seen.clone(),
                        saved: std::mem::take(&mut state.saved),
                    };
                    let mut out = output.lock().unwrap();
                    out.current = Some(frozen);
                    out.raw = result
                        .as_ref()
                        .err()
                        .is_some_and(|e| e.kind() != ErrorKind::Runtime);
                }
                let locals = self.restore_command_locals(vm, previous_locals);
                let inactive = vm.call(inactive, &[]).map(|_| ());
                drop(random);
                locals?;
                inactive?;
                vm.call(finish, &[])?;
                result?;
                Ok(std::mem::take(&mut output.lock().unwrap().blocks))
            })();
            // VM allocation/entry failure must never leave a live Rust scope
            // whose retained values are about to lose their temporary root.
            {
                let mut out = output.lock().unwrap();
                if out.activated && !out.deactivated {
                    self.store.lock().unwrap().action_capture = out.previous.take();
                    out.deactivated = true;
                }
            }
            vm.unpin(pin)?;
            result
        })();
        let mut out = output.lock().unwrap();
        let restored = out.restored;
        out.current = None;
        if result.is_err() {
            out.blocks.clear();
        }
        (result, restored, returned)
    }
}
fn begin(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let globals = helper(call, 5)?;
    let retained = table(call, call.captures()[2])?;
    let mut tables = Vec::new();
    for (i, key) in KEYS.iter().enumerate() {
        let key = call.try_intern(key.as_bytes())?;
        call.raw_table_set(retained, Value::Number((i + 7) as f64), key)?;
        let old = call.table_get(globals, key)?;
        call.raw_table_set(retained, Value::Number((i + 1) as f64), old)?;
        let next = call.try_new_table()?;
        call.raw_table_set(retained, Value::Number((i + 4) as f64), Value::Table(next))?;
        tables.push((key, next));
    }
    for &(key, next) in &tables {
        call.table_set(globals, key, Value::Table(next))?;
    }
    let state = State {
        retained,
        actors: tables[0].1,
        actor_set: tables[1].1,
        snapshots: tables[2].1,
        order: Vec::new(),
        seen: HashSet::new(),
        saved: Vec::new(),
    };
    let previous = data.store.lock().unwrap().action_capture.replace(state);
    let mut output = data.output.lock().unwrap();
    output.previous = previous;
    output.activated = true;
    Ok(())
}
/// Setter preflight. Structural and queue/tween operations use prepare directly.
pub(super) fn prepare_write(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    super::ease_probe::actor(call, store, actor)?;
    prepare(call, store, actor)
}
pub(super) fn prepare(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    let tables = {
        let mut guard = store.lock().unwrap();
        let Some(state) = guard.action_capture.as_mut() else {
            return Ok(());
        };
        if !state.seen.insert(actor) {
            return Ok(());
        }
        (
            state.actors,
            state.actor_set,
            state.snapshots,
            state.retained,
        )
    };
    call.raw_table_set(tables.1, Value::Table(actor), Value::Boolean(true))?;
    let count = call.raw_table_len(tables.0)? + 1;
    call.raw_table_set(tables.0, Value::Number(count as f64), Value::Table(actor))?;
    store
        .lock()
        .unwrap()
        .action_capture
        .as_mut()
        .unwrap()
        .order
        .push(actor);
    let raw = state_snapshot::raw_copy(call, actor)?;
    call.with_roots(&[Value::Table(raw)], |call| {
        let row = call.try_new_table()?;
        call.with_roots(&[Value::Table(row)], |call| {
            call.raw_table_set(row, Value::Number(1.), Value::Table(actor))?;
            call.raw_table_set(row, Value::Number(2.), Value::Table(raw))?;
            let count = call.raw_table_len(tables.2)? + 1;
            call.raw_table_set(tables.3, Value::Table(row), Value::Boolean(true))?;
            call.raw_table_set(tables.2, Value::Number(count as f64), Value::Table(row))
        })
    })?;
    let saved = function_capture::Saved::read(store, actor, raw);
    store
        .lock()
        .unwrap()
        .action_capture
        .as_mut()
        .unwrap()
        .saved
        .push((actor, raw, saved));
    function_capture::reset(call, store, actor)
}
fn deactivate(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    {
        let mut output = data.output.lock().unwrap();
        let mut store = data.store.lock().unwrap();
        store.action_capture = output.previous.take();
        output.deactivated = true;
    }
    let retained = table(call, call.captures()[2])?;
    let globals = helper(call, 5)?;
    for (i, key) in KEYS.iter().enumerate() {
        let old = call.raw_table_get(retained, Value::Number((i + 1) as f64))?;
        let key = call.try_intern(key.as_bytes())?;
        call.table_set(globals, key, old)?;
    }
    Ok(())
}
fn finish(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let (state, restore_raw) = {
        let mut out = data.output.lock().unwrap();
        (out.current.take().unwrap(), out.raw)
    };
    let mut blocks = Vec::new();
    for (index, actor) in &data.selected {
        if state.seen.contains(actor)
            && let Some(capture) = data
                .store
                .lock()
                .unwrap()
                .actors
                .get_mut(actor)
                .and_then(|a| a.capture.as_mut())
        {
            let captured = capture.snapshot().blocks;
            if !captured.is_empty() {
                blocks.push((*index, captured));
            }
        }
    }
    if let Some(action) = &data.action {
        action.before_actor_restore(call)?;
    }
    for actor in state.order {
        function_capture::reset(call, &data.store, actor)?;
    }
    for (actor, raw, saved) in state.saved {
        state_snapshot::restore_raw(call, actor, raw, restore_raw)?;
        if let Some(saved) = saved {
            saved.restore(&data.store);
        }
    }
    lifecycle::invalidate_plans(call, &data.store)?;
    if let Some(action) = &data.action {
        action.after_actor_restore(call)?;
    }
    let mut output = data.output.lock().unwrap();
    output.blocks = blocks;
    output.restored = true;
    Ok(())
}
