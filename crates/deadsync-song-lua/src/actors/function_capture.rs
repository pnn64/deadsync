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
    authored_transform: bool,
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
            authored_transform: a.authored_transform,
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
        // Native deletion is structural: restoring a pose cannot resurrect a
        // destroyed wrapper or make its retained Lua handle usable again.
        let wrappers = self
            .wrappers
            .into_iter()
            .filter(|wrapper| store.actors.contains_key(wrapper))
            .collect();
        let Some(a) = store.actors.get_mut(&self.actor) else {
            return;
        };
        // Child attachment and parent changes are structural native mutations;
        // surviving wrapper membership is part of semantic actor state.
        let children = std::mem::take(&mut a.state.children);
        let parent = a.state.parent;
        a.state = self.state;
        a.base_transform = self.base_transform;
        a.pose_exported = self.pose_exported;
        a.authored_transform = self.authored_transform;
        a.state.children = children;
        a.state.parent = parent;
        a.current = self.current;
        a.queue = self.queue;
        a.replay = self.replay;
        a.preparation_getters = self.getters;
        a.hibernate = self.hibernate;
        a.update_rate = self.rate;
        a.effects = self.effects;
        a.wrappers = wrappers;
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
    reset_native_frame(call.store(), actor);
    reset_fields(call, actor)
}

pub(super) struct BakedFrameReset {
    keys: [Value; RESET_FIELDS.len()],
    blocks_key: Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResetLayout {
    Unchecked,
    Complete,
    MissingSlots,
}

#[derive(Clone, Copy)]
pub(super) struct ResetVersion {
    fields: u32,
    layout: ResetLayout,
}

impl BakedFrameReset {
    pub(super) fn new() -> Self {
        Self {
            keys: [Value::Nil; RESET_FIELDS.len()],
            blocks_key: Value::Nil,
        }
    }

    pub(super) fn reset(&mut self, vm: &mut Vm, actor: TableId) -> Result<(), Error> {
        let owned = vm.store().actors.get_mut(&actor).map(|entry| {
            if let Some(capture) = &mut entry.capture {
                capture.reset_blocks();
            }
            entry.baked_capture_reset.take()
        });
        let unchanged = match owned.flatten() {
            Some(prior) if prior.fields == vm.raw_fields_version(actor)? => Some(prior.layout),
            _ => None,
        };
        let mut layout = unchanged.unwrap_or(ResetLayout::Unchecked);
        if layout != ResetLayout::Complete {
            if self.keys[0] == Value::Nil {
                for ((name, _), key) in RESET_FIELDS.into_iter().zip(&mut self.keys) {
                    *key = vm.intern_static(name.as_bytes())?;
                }
            }
            for ((_, value), key) in RESET_FIELDS.into_iter().zip(self.keys) {
                vm.raw_table_set(actor, key, value)?;
            }
        }
        if self.blocks_key == Value::Nil {
            self.blocks_key = vm.intern_static(BLOCKS_KEY.as_bytes())?;
        }
        let blocks = vm.try_new_table()?;
        vm.raw_table_set(actor, self.blocks_key, Value::Table(blocks))?;
        if owned.is_some() {
            if unchanged == Some(ResetLayout::Unchecked) {
                // Inspect the final layout. The first insertion of the
                // blocks key might have replaced a nil slot from that reset.
                layout = if self.has_reset_slots(vm, actor)? {
                    ResetLayout::Complete
                } else {
                    ResetLayout::MissingSlots
                };
            }
            let fields = vm.raw_fields_version(actor)?;
            vm.store()
                .actors
                .get_mut(&actor)
                .unwrap()
                .baked_capture_reset = Some(ResetVersion { fields, layout });
        }
        Ok(())
    }

    fn has_reset_slots(&self, vm: &mut Vm, actor: TableId) -> Result<bool, Error> {
        // Raw nil can mean a missing key. In native Lua order a write to a
        // missing key can change traversal, so require valid next cursors.
        // Keyed writes invalidate the proof. Replacing the existing blocks
        // value leaves these slots in place, and still publishes a fresh table.
        vm.with_native_stack(|stack| {
            if !stack.check_stack(3) {
                return Ok(false);
            }
            let top = stack.top();
            stack.push(Value::Table(actor))?;
            let index = stack.top_offset();
            let mut complete = true;
            for ((_, value), key) in RESET_FIELDS.into_iter().zip(self.keys) {
                if value != Value::Nil {
                    continue;
                }
                stack.push(key)?;
                match stack.next_table(index) {
                    Ok(_) => stack.set_top(top + 1)?,
                    Err(error) if error.message() == "invalid key to 'next'" => {
                        complete = false;
                        break;
                    }
                    Err(error) => return Err(error),
                }
            }
            stack.set_top(top)?;
            Ok(complete)
        })
    }
}

fn reset_native_frame(store: &mut Store, actor: TableId) {
    if let Some(capture) = store
        .actors
        .get_mut(&actor)
        .and_then(|a| a.capture.as_mut())
    {
        capture.reset_blocks();
    }
}

const RESET_FIELDS: [(&str, Value); 9] = [
    ("__songlua_capture_cursor", Value::Number(0.)),
    ("__songlua_capture_duration", Value::Number(0.)),
    ("__songlua_capture_tween_time_left", Value::Number(0.)),
    ("__songlua_startup_queue_seconds", Value::Nil),
    ("__songlua_capture_easing", Value::Nil),
    ("__songlua_capture_opt1", Value::Nil),
    ("__songlua_capture_opt2", Value::Nil),
    ("__songlua_capture_block", Value::Nil),
    ("__songlua_capture_immediate_block", Value::Nil),
];
const BLOCKS_KEY: &str = "__songlua_capture_blocks";

#[cfg(test)]
mod reset_tests {
    use super::*;

    fn raw_keys(vm: &mut Vm, actor: TableId) -> Vec<Value> {
        vm.with_native_stack(|stack| {
            let base = stack.top();
            stack.push(Value::Table(actor))?;
            let index = stack.top_offset();
            stack.push(Value::Nil)?;
            let mut keys = Vec::new();
            while stack.next_table(index)? {
                keys.push(stack.value(-2).unwrap());
                stack.set_top(stack.top() - 1)?;
            }
            stack.set_top(base)?;
            Ok(keys)
        })
        .unwrap()
    }

    #[test]
    fn repeated_baked_reset_checks_slot_layout_and_keeps_a_fresh_blocks_table() {
        let mut runtime =
            crate::SongRuntime::new(&crate::SongLuaCompileContext::new(".", "reset")).unwrap();
        let vm = &mut runtime.vm;
        let actor = vm
            .with_native_stack(|stack| {
                let base = stack.top();
                // The block and blocks keys have the same main slot at this
                // capacity. That nil slot is lost on the first blocks write.
                stack.create_table(0, 64)?;
                let Some(Value::Table(actor)) = stack.value(-1) else {
                    unreachable!()
                };
                stack.set_top(base)?;
                Ok(actor)
            })
            .unwrap();
        let root = vm.pin(Value::Table(actor)).unwrap();
        vm.store()
            .actors
            .insert(actor, Actor::new(0, "Actor", None, None));
        let mut reset = BakedFrameReset::new();
        let blocks = vm.intern_static(BLOCKS_KEY.as_bytes()).unwrap();
        let duration = vm.intern_static(b"__songlua_capture_duration").unwrap();

        reset.reset(vm, actor).unwrap();
        assert!(!reset.has_reset_slots(vm, actor).unwrap());
        reset.reset(vm, actor).unwrap();
        let first = vm.raw_table_get(actor, blocks).unwrap();
        let keys = raw_keys(vm, actor);
        let version = vm.raw_fields_version(actor).unwrap();
        reset.reset(vm, actor).unwrap();
        assert_eq!(
            vm.raw_fields_version(actor).unwrap(),
            version.wrapping_add(1)
        );
        assert_eq!(keys, raw_keys(vm, actor));
        assert_ne!(first, vm.raw_table_get(actor, blocks).unwrap());

        vm.raw_table_set(actor, duration, Value::Number(7.))
            .unwrap();
        reset.reset(vm, actor).unwrap();
        assert_eq!(
            vm.raw_table_get(actor, duration).unwrap(),
            Value::Number(0.)
        );
        assert_eq!(keys, raw_keys(vm, actor));
        vm.unpin(root).unwrap();
    }
}

fn reset_fields(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    for (key, value) in RESET_FIELDS {
        song_replay::raw_set(call, actor, key, value)?;
    }
    if call
        .store()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        let value = get_static(call, actor, BLOCKS_KEY.as_bytes())?;
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
    song_replay::raw_set(call, actor, BLOCKS_KEY, Value::Table(blocks))
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
