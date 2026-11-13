//! Existing indexed actor-function capture used by compiler easing readers.
use super::super::SongActorFunctionBlocks;
use super::*;
use crate::tween_pose::TweenPose;

pub(super) struct Saved {
    pub(super) actor: TableId,
    pub(super) raw: TableId,
    state: ActorSnapshot,
    base_transform: crate::base_transform::BaseTransform,
    pose_exported: bool,
    current: TweenPose,
    queue: VecDeque<TweenStep>,
    replay: Option<song_replay::State>,
    capture: Option<capture::Capture>,
    getters: Option<[f32; 4]>,
    hibernate: f32,
    rate: f32,
    effects: effects::State,
    wrappers: Vec<TableId>,
}
impl Saved {
    pub(super) fn read(store: &Store, actor: TableId, raw: TableId) -> Option<Self> {
        let a = store.actors.get(&actor)?;
        Some(Self {
            actor,
            raw,
            state: a.state.clone(),
            base_transform: a.base_transform,
            pose_exported: a.pose_exported,
            current: a.current,
            queue: a.queue.clone(),
            replay: a.replay.clone(),
            capture: a.capture.clone(),
            getters: a.preparation_getters,
            hibernate: a.hibernate,
            rate: a.update_rate,
            effects: a.effects,
            wrappers: a.wrappers.clone(),
        })
    }
    pub(super) fn restore(self, store: &mut Store) {
        self.restore_inner(store, false);
    }
    pub(super) fn restore_mutable(self, store: &mut Store) {
        self.restore_inner(store, true);
    }
    fn restore_inner(self, store: &mut Store, retain_capture: bool) {
        let Some(a) = store.actors.get_mut(&self.actor) else {
            return;
        };
        // Child attachment and parent changes are structural native mutations;
        // wrapper membership is explicitly part of semantic actor state.
        let children = std::mem::take(&mut a.state.children);
        let parent = a.state.parent;
        a.state = self.state;
        a.base_transform = self.base_transform;
        a.pose_exported = self.pose_exported;
        a.state.children = children;
        a.state.parent = parent;
        a.current = self.current;
        a.queue = self.queue;
        a.replay = self.replay;
        a.preparation_getters = self.getters;
        a.hibernate = self.hibernate;
        a.update_rate = self.rate;
        a.effects = self.effects;
        a.wrappers = self.wrappers;
        a.capture = self.capture;
        if !retain_capture {
            a.capture
                .get_or_insert_with(capture::Capture::default)
                .reset_blocks();
        }
    }
}
#[derive(Default)]
struct Output {
    saved: Vec<Saved>,
    blocks: SongActorFunctionBlocks,
    restored: bool,
}
#[derive(Clone)]
struct Run {
    actors: Vec<(usize, TableId)>,
    raw: bool,
    output: Arc<Mutex<Output>>,
}

/// Preparation flags replaced by [`Actors::deferred_message_capture`].
#[must_use = "pass the saved flags to end_deferred_message_capture"]
pub(crate) struct DeferredCapture {
    preparing: bool,
    playback: bool,
}
impl Actors {
    // The compiler retries deferred commands after chronological capture has
    // ended, using the shared locals reached by real updates. This is a bounded
    // preparation probe, not another frame or live callback.
    pub(crate) fn deferred_message_capture(&self, vm: &mut Vm) -> Result<DeferredCapture, String> {
        let store = vm.store();
        if store.clock.live.load(std::sync::atomic::Ordering::Relaxed)
            || store.replay_frame.is_some()
        {
            return Err("deferred message capture requires inactive preparation replay".into());
        }
        let scope = DeferredCapture {
            preparing: store.preparing,
            playback: store.playback,
        };
        store.preparing = true;
        store.playback = false;
        Ok(scope)
    }

    /// Restore the flags saved by [`Self::deferred_message_capture`].
    pub(crate) fn end_deferred_message_capture(&self, vm: &mut Vm, scope: DeferredCapture) {
        let store = vm.store();
        store.preparing = scope.preparing;
        store.playback = scope.playback;
    }

    pub(crate) fn actor_function_capture_ready(&self, vm: &mut Vm) -> bool {
        let guard = vm.store();
        guard.preparing
            && !guard.playback
            && !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn capture_actor_function(
        &self,
        vm: &mut Vm,
        actors: Vec<(usize, TableId)>,
        function: Value,
        arg: Option<f32>,
    ) -> (Result<SongActorFunctionBlocks, Error>, bool, Option<Root>) {
        let broadcasts = song_capture::pause_preparation_broadcasts(vm.store());
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
                let captures = [
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    function,
                    Value::Table(retained),
                ];
                let mut data = Run {
                    actors,
                    raw: false,
                    output: output.clone(),
                };
                let setup = vm.new_host_function_with_data(setup, &captures, data.clone())?;
                vm.call(setup, &[])?;
                // A failed Lua entry must finish before restoration begins. A
                // fatal instruction abort applies to its entire nested call tree.
                let args = arg
                    .map(|n| Value::Number(f64::from(n)))
                    .into_iter()
                    .collect::<Vec<_>>();
                let result = vm
                    .call(function, &args)
                    .map(|values| values.first().copied().unwrap_or(Value::Nil));
                // Native function.call::<Value> retains its first result
                // throughout actor restoration and the outer clock rewind.
                let result = result.and_then(|value| vm.pin(value).map(|pin| returned = Some(pin)));
                data.raw = result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.kind() != ErrorKind::Runtime);
                let finish = vm.new_host_function_with_data(finish, &captures, data)?;
                vm.call(finish, &[])?;
                result?;
                Ok(std::mem::take(&mut output.lock().unwrap().blocks))
            })();
            vm.unpin(pin)?;
            result
        })();
        let mut output = output.lock().unwrap();
        let restored = output.restored;
        output.saved.clear();
        if result.is_err() {
            output.blocks.clear();
        }
        song_capture::resume_preparation_broadcasts(vm.store(), broadcasts);
        (result, restored, returned)
    }
}

pub(super) fn reset(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if let Some(a) = call.store().actors.get_mut(&actor) {
        a.capture
            .get_or_insert_with(capture::Capture::default)
            .reset_blocks();
    }
    reset_fields(call, actor)
}
pub(super) fn reset_frame(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if let Some(capture) = call
        .store()
        .actors
        .get_mut(&actor)
        .and_then(|a| a.capture.as_mut())
    {
        capture.reset_blocks();
    }
    reset_fields(call, actor)
}
fn reset_fields(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    for key in [
        "__songlua_capture_cursor",
        "__songlua_capture_duration",
        "__songlua_capture_tween_time_left",
    ] {
        song_replay::raw_set(call, actor, key, Value::Number(0.))?;
    }
    for key in [
        "__songlua_startup_queue_seconds",
        "__songlua_capture_easing",
        "__songlua_capture_opt1",
        "__songlua_capture_opt2",
        "__songlua_capture_block",
        "__songlua_capture_immediate_block",
    ] {
        song_replay::raw_set(call, actor, key, Value::Nil)?;
    }
    if call
        .store()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        let value = get(call, actor, b"__songlua_capture_blocks")?;
        if value != Value::Nil {
            let blocks = table(call, value)?;
            let mut stack = call.native_stack()?;
            let base = stack.top();
            if !stack.check_stack(5) {
                return Err(stack.error(b"capture reset: native stack overflow"));
            }
            stack.push(Value::Table(blocks))?;
            let index = stack.top_offset();
            stack.push(Value::Nil)?;
            while stack.next_table(index)? {
                let key = stack.value(-2).unwrap();
                stack.push(key)?;
                stack.push(Value::Nil)?;
                stack.set_table(index, true)?;
                stack.set_top(stack.top() - 1)?;
            }
            stack.set_top(base)?;
            return Ok(());
        }
    }
    let blocks = call.try_new_table()?;
    song_replay::raw_set(
        call,
        actor,
        "__songlua_capture_blocks",
        Value::Table(blocks),
    )
}
fn setup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let retained = table(call, call.captures()[3])?;
    let mut saved = Vec::new();
    for (i, (_, actor)) in data.actors.iter().enumerate() {
        let raw = state_snapshot::raw_copy(call, *actor)?;
        call.raw_table_set(retained, Value::Number((i + 1) as f64), Value::Table(raw))?;
        saved.push(
            Saved::read(call.store_ref(), *actor, raw)
                .ok_or_else(|| call.error(b"capture actor is not owned by this runtime"))?,
        );
    }
    for (_, actor) in &data.actors {
        reset(call, *actor)?;
    }
    data.output.lock().unwrap().saved = saved;
    Ok(())
}
fn finish(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let mut blocks = Vec::new();
    for (index, id) in &data.actors {
        if let Some(capture) = call
            .store()
            .actors
            .get_mut(id)
            .and_then(|a| a.capture.as_mut())
        {
            let captured = capture.snapshot().blocks;
            if !captured.is_empty() {
                blocks.push((*index, captured));
            }
        }
    }
    for (_, actor) in &data.actors {
        reset(call, *actor)?;
    }
    let saved = std::mem::take(&mut data.output.lock().unwrap().saved);
    for state in saved {
        state_snapshot::restore_raw(call, state.actor, state.raw, data.raw)?;
        state.restore(call.store());
    }
    lifecycle::invalidate_plans(call)?;
    let mut output = data.output.lock().unwrap();
    output.blocks = blocks;
    output.restored = true;
    Ok(())
}
