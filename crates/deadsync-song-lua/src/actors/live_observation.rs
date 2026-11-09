//! Read-only replay observation at the existing overlay callback boundary.
use super::*;
use crate::statistics;

const OBSERVER: &[u8] = b"__songlua_overlay_stage_observer";
type Output = Arc<Mutex<Option<crate::SongLuaOverlayStageObservation>>>;

#[derive(Clone)]
struct Observer {
    actor: Option<TableId>,
    statistics: Arc<Mutex<statistics::State>>,
    step_count: usize,
    output: Output,
}

impl Actors {
    pub(crate) fn take_message_coverage(&self) -> Vec<String> {
        std::mem::take(&mut self.store.lock().unwrap().message_coverage)
    }

    pub(crate) fn enable_overlay_observer(
        &self,
        vm: &mut Vm,
        catalog: Value,
        statistics: Arc<Mutex<statistics::State>>,
        step_count: usize,
        output: Output,
    ) -> Result<(), Error> {
        let setup = vm.new_host_function_with_data(
            install,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                catalog,
            ],
            Observer {
                actor: None,
                statistics,
                step_count,
                output,
            },
        )?;
        vm.call(setup, &[]).map(|_| ())
    }

    pub(crate) fn install_message_paths(
        &self,
        vm: &mut Vm,
        paths: &[(String, usize)],
    ) -> Result<(), Error> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut order = HashMap::new();
        let mut identities = HashSet::new();
        for (path, rank) in paths {
            if *rank == 0
                || order.insert(path.clone(), *rank).is_some()
                || !identities.insert(*rank)
            {
                return vm.with_native_stack(|s| {
                    Err(s.error(b"invalid captured subscriber identities"))
                });
            }
        }
        self.store.lock().unwrap().message_paths = order;
        Ok(())
    }
}

fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let screen = get(call, globals, b"__songlua_top_screen")?;
    let screen = table(call, screen)?;
    let children = native_children::named_children(call, screen)?;
    let children = table(call, children)?;
    let overlay = get(call, children, b"Overlay")?;
    let overlay = table(call, overlay)?;
    let children = native_children::named_children(call, overlay)?;
    let children = table(call, children)?;
    let probe = get(call, children, b"ITGmaniaHarnessTraceProbe")?;
    let actor = if probe == Value::Nil {
        overlay
    } else {
        table(call, probe)?
    };
    let mut data = call.function_data::<Observer>().unwrap().clone();
    data.actor = Some(actor);
    *data.output.lock().unwrap() = None;
    // Both Lua owners are traced captures. The actor identity itself is only
    // compared, like the native probe pointer, and does not retain a dead scene.
    let callback = call.new_host_function_with_data(
        record,
        &[call.captures()[2], Value::Table(globals)],
        data,
    )?;
    let helpers = table(call, call.captures()[1])?;
    let key = call.try_intern(OBSERVER)?;
    call.raw_table_set(helpers, key, callback)
}

pub(super) fn observe(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let helpers = table(call, call.captures()[1])?;
    let key = call.try_intern(OBSERVER)?;
    let observer = call.raw_table_get(helpers, key)?;
    if observer != Value::Nil {
        call.call(observer, &[Value::Table(actor)])?;
    }
    Ok(())
}

fn record(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Observer>().unwrap().clone();
    if call.arguments().first().copied() != data.actor.map(Value::Table) {
        return Ok(());
    }
    let Some(players) = data.statistics.lock().unwrap().players else {
        return Ok(());
    };
    let catalog = table(call, call.captures()[0])?;
    let key = call.try_intern(b"scene_step_indices")?;
    let selected = call.raw_table_get(catalog, key)?;
    let values = call.call(selected, &[])?.to_vec();
    let player_step_indices = std::array::from_fn(|i| match values[i] {
        Value::Number(index) if (index as usize) < data.step_count => Some(index as usize),
        _ => None,
    });
    let globals = table(call, call.captures()[1])?;
    let key = call.try_intern(crate::SONG_LUA_RUNTIME_KEY.as_bytes())?;
    let runtime = call.table_get(Value::Table(globals), key)?;
    let runtime = table(call, runtime)?;
    let key = call.try_intern(crate::BEAT.as_bytes())?;
    let beat = call.table_get(Value::Table(runtime), key)?;
    let beat = sprites::number(call, beat)? as f32;
    let key = call.try_intern(crate::SECONDS.as_bytes())?;
    let seconds = call.table_get(Value::Table(runtime), key)?;
    let music_seconds = sprites::number(call, seconds)? as f32;
    *data.output.lock().unwrap() = Some(crate::SongLuaOverlayStageObservation {
        beat,
        music_seconds,
        players,
        player_step_indices,
    });
    Ok(())
}
