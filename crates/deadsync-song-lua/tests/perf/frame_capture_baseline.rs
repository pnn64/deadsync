// Frozen from 8b3c45af9 (0.5.1626); visibility and formatting only.
use super::*;

pub(super) struct SongLuaOverlayUpdateCapture {
    pub(super) actor_indices: HashMap<usize, usize>,
    pub(super) active_broadcast: Option<String>,
    // Scoped with active_broadcast so nested dispatch restores both together.
    pub(super) active_broadcast_command: Option<mlua::LuaString>,
    pub(super) runtime_broadcasts: Vec<(f32, String, bool)>,
    pub(super) touched: Vec<usize>,
    pub(super) touched_flags: Vec<bool>,
    pub(super) values: Vec<Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>>,
    pub(super) final_values: Vec<Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>>,
    pub(super) scheduled: Vec<Vec<SongLuaScheduledOverlayUpdate>>,
    pub(super) stateful_messages:
        BTreeMap<String, BTreeMap<usize, BTreeSet<SongLuaOverlayUpdateTarget>>>,
    pub(super) stateful_writes: BTreeMap<String, Vec<SongLuaStatefulMessageWrite>>,
}

impl SongLuaOverlayUpdateCapture {
    pub(super) fn new(actor_indices: HashMap<usize, usize>) -> Self {
        let actor_count = actor_indices.len();
        Self {
            actor_indices,
            active_broadcast: None,
            active_broadcast_command: None,
            runtime_broadcasts: Vec::new(),
            touched: Vec::with_capacity(actor_count),
            touched_flags: vec![false; actor_count],
            values: (0..actor_count).map(|_| Vec::new()).collect(),
            final_values: (0..actor_count).map(|_| Vec::new()).collect(),
            scheduled: (0..actor_count).map(|_| Vec::new()).collect(),
            stateful_messages: BTreeMap::new(),
            stateful_writes: BTreeMap::new(),
        }
    }

    fn record_stateful_message(
        &mut self,
        actor: &Table,
        beat: f32,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
        delay_seconds: f32,
        duration_seconds: f32,
        easing: Option<String>,
        opt1: Option<f32>,
    ) -> Option<usize> {
        if self.active_broadcast.is_none() {
            return None;
        }
        let index = self.touch(actor)?;
        let message = self
            .active_broadcast
            .as_deref()
            .expect("broadcast is active");
        if let Some(targets) = self.stateful_messages.get_mut(message) {
            targets.entry(index).or_default().insert(target);
        } else {
            self.stateful_messages.insert(
                message.to_owned(),
                BTreeMap::from([(index, BTreeSet::from([target]))]),
            );
        }
        let write = SongLuaStatefulMessageWrite {
            overlay_index: index,
            beat,
            target,
            value,
            delay_seconds,
            duration_seconds,
            easing,
            opt1,
        };
        if let Some(writes) = self.stateful_writes.get_mut(message) {
            writes.push(write);
        } else {
            self.stateful_writes.insert(message.to_owned(), vec![write]);
        }
        Some(index)
    }

    pub(super) fn touch(&mut self, actor: &Table) -> Option<usize> {
        let &index = self.actor_indices.get(&(actor.to_pointer() as usize))?;
        if !self.touched_flags[index] {
            self.touched_flags[index] = true;
            self.touched.push(index);
        }
        Some(index)
    }

    pub(super) fn record(
        &mut self,
        actor: &Table,
        beat: f32,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        let index = if self.active_broadcast.is_some() {
            self.record_stateful_message(actor, beat, target, value.clone(), 0.0, 0.0, None, None)
        } else {
            self.touch(actor)
        };
        let Some(index) = index else {
            return false;
        };
        Self::replace_value(&mut self.final_values[index], target, value.clone());
        Self::replace_value(&mut self.values[index], target, value);
        true
    }

    fn replace_value(
        values: &mut Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) {
        if let Some((_, current)) = values.iter_mut().find(|(current, _)| *current == target) {
            *current = value;
        } else {
            values.push((target, value));
        }
    }

    pub(super) fn record_scheduled(
        &mut self,
        actor: &Table,
        beat: f32,
        delay_seconds: f32,
        duration_seconds: f32,
        easing: Option<String>,
        opt1: Option<f32>,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        let index = if self.active_broadcast.is_some() {
            self.record_stateful_message(
                actor,
                beat,
                target,
                value.clone(),
                delay_seconds,
                duration_seconds,
                easing.clone(),
                opt1,
            )
        } else {
            self.touch(actor)
        };
        let Some(index) = index else {
            return false;
        };
        Self::replace_value(&mut self.final_values[index], target, value.clone());
        self.scheduled[index].push(SongLuaScheduledOverlayUpdate {
            delay_seconds,
            duration_seconds,
            easing,
            opt1,
            target,
            value,
        });
        true
    }
}

pub(super) fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    let call = || match params {
        Some(params) => command.call::<()>((actor, params)),
        None => command.call::<()>((actor,)),
    };
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), call);
    }
    call()
}
