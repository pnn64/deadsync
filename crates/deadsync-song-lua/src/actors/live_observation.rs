//! Read-only replay observation at the existing overlay callback boundary.
use super::*;
use crate::statistics;

const OBSERVER: &[u8] = b"__songlua_overlay_stage_observer";
const OBSERVE_GETTERS: &[u8] = b"__songlua_observe_stage_getters";
type Output = Arc<Mutex<Option<crate::SongLuaOverlayStageObservation>>>;
type StatisticsOutput = Arc<Mutex<Option<crate::live::SongLuaStageStatisticsObservation>>>;

impl crate::SongRuntime {
    /// Only invoke Lua getters when the native probe sampled this update.
    /// Authored overrides may have side effects, so intervening frames retain
    /// the cheap host snapshot without introducing additional Lua queries.
    pub fn set_stage_getter_observation(&mut self, enabled: bool) -> Result<(), Error> {
        let key = self.vm.try_intern(OBSERVE_GETTERS)?;
        self.vm.raw_table_set(self.actors.helpers, key, Value::Boolean(enabled))
    }

    /// Observe the public Lua getters without advancing gameplay or publishing
    /// replacement statistics. Lifecycle probes use this same reader as the
    /// pre-foreground overlay probe below.
    pub fn observe_stage_statistics(
        &mut self,
    ) -> Result<crate::live::SongLuaStageStatisticsObservation, Error> {
        let output: StatisticsOutput = Default::default();
        let globals = self.vm.get_global(b"_G")?;
        let callback = self.vm.new_host_function_with_data(
            |call| {
                let output = call.function_data::<StatisticsOutput>().unwrap().clone();
                let globals = table(call, call.captures()[0])?;
                *output.lock().unwrap() = Some(read_statistics(call, globals)?);
                Ok(())
            },
            &[globals],
            output.clone(),
        )?;
        self.vm.call(callback, &[])?;
        let observed = output.lock().unwrap().take().expect("stage observer ran");
        Ok(observed)
    }
}

fn query(
    call: &mut HostCall<'_>,
    owner: Value,
    name: &[u8],
    args: &[Value],
) -> Result<Value, Error> {
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(owner);
    values.extend_from_slice(args);
    call.with_roots(&values, |call| {
        let key = call.try_intern(name)?;
        let function = call.table_get(owner, key)?;
        Ok(call.call(function, &values)?.first().copied().unwrap_or(Value::Nil))
    })
}

fn numeric_query(call: &mut HostCall<'_>, owner: Value, name: &[u8]) -> Result<f64, Error> {
    let value = query(call, owner, name, &[])?;
    sprites::number(call, value)
}

fn read_statistics(
    call: &mut HostCall<'_>,
    globals: TableId,
) -> Result<crate::live::SongLuaStageStatisticsObservation, Error> {
    let game = get_static(call, globals, b"GAMESTATE")?;
    let manager = get_static(call, globals, b"STATSMAN")?;
    let stage = query(call, manager, b"GetCurStageStats", &[])?;
    call.with_roots(&[game, stage], |call| {
        let gameplay_seconds = numeric_query(call, stage, b"GetGameplaySeconds")?;
        let steps_seconds = numeric_query(call, stage, b"GetStepsSeconds")?;
        let enabled = query(call, game, b"GetEnabledPlayers", &[])?;
        call.with_roots(&[enabled], |call| {
            let enabled = table(call, enabled)?;
            let mut players = Vec::new();
            for index in 1..=crate::LUA_PLAYERS + 1 {
                let player = call.raw_table_get(enabled, Value::Number(index as f64))?;
                if player == Value::Nil { break; }
                if index > crate::LUA_PLAYERS {
                    return Err(call.error(b"stage observer found too many enabled players"));
                }
                let stringify = get_static(call, globals, b"tostring")?;
                let identity = call.call(stringify, &[player])?.first().copied().unwrap_or(Value::Nil);
                let identity = assets::text(call, identity)?;
                let stats = query(call, stage, b"GetPlayerStageStats", &[player])?;
                let observed = call.with_roots(&[stats], |call| {
                    let life = query(call, stats, b"GetLifeRecord", &[
                        Value::Number(gameplay_seconds), Value::Number(2.0),
                    ])?;
                    let [life_begin, life_latest] = call.with_roots(&[life], |call| {
                        let life = table(call, life)?;
                        let first = call.raw_table_get(life, Value::Number(1.0))?;
                        let last = call.raw_table_get(life, Value::Number(2.0))?;
                        Ok([sprites::number(call, first)?, sprites::number(call, last)?])
                    })?;
                    let state = query(call, game, b"GetPlayerState", &[player])?;
                    let autoplay = call.with_roots(&[state], |call| {
                        let current = call.try_intern(b"ModsLevel_Current")?;
                        let options = query(call, state, b"GetPlayerOptions", &[current])?;
                        numeric_query(call, options, b"PlayerAutoPlay")
                    })?;
                    let score = numeric_query(call, stats, b"GetScore")?;
                    let current_max_score = numeric_query(call, stats, b"GetCurMaxScore")?;
                    let alive_seconds = numeric_query(call, stats, b"GetAliveSeconds")?;
                    Ok(crate::live::SongLuaStagePlayerObservation {
                        player: identity, score, current_max_score, alive_seconds,
                        life_begin, life_latest, autoplay,
                    })
                })?;
                players.push(observed);
            }
            Ok(crate::live::SongLuaStageStatisticsObservation { gameplay_seconds, steps_seconds, players })
        })
    })
}

#[derive(Clone)]
struct Observer {
    actor: Option<TableId>,
    statistics: Arc<Mutex<statistics::State>>,
    step_count: usize,
    output: Output,
}

impl Actors {
    pub(crate) fn take_message_coverage(&self, vm: &mut Vm) -> Vec<String> {
        std::mem::take(&mut vm.store().message_coverage)
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
        vm.store().message_paths = order;
        Ok(())
    }
}

fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let screen = get_static(call, globals, b"__songlua_top_screen")?;
    let screen = table(call, screen)?;
    let children = native_children::named_children(call, screen)?;
    let children = table(call, children)?;
    let overlay = get_static(call, children, b"Overlay")?;
    let overlay = table(call, overlay)?;
    let children = native_children::named_children(call, overlay)?;
    let children = table(call, children)?;
    let probe = get_static(call, children, b"ITGmaniaHarnessTraceProbe")?;
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
        &[call.captures()[2], Value::Table(globals), call.captures()[1]],
        data,
    )?;
    let helpers = table(call, call.captures()[1])?;
    let capture_key = call.try_intern(OBSERVE_GETTERS)?;
    call.raw_table_set(helpers, capture_key, Value::Boolean(false))?;
    let key = call.try_intern(OBSERVER)?;
    call.raw_table_set(helpers, key, callback)
}

pub(super) fn observe(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let helpers = table(call, call.captures()[1])?;
    let observer = get_static(call, helpers, OBSERVER)?;
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
    let key = call.intern_static(b"scene_step_indices")?;
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
    let helpers = table(call, call.captures()[2])?;
    let statistics = if get_static(call, helpers, OBSERVE_GETTERS)? == Value::Boolean(true) {
        Some(read_statistics(call, globals)?)
    } else {
        None
    };
    *data.output.lock().unwrap() = Some(crate::SongLuaOverlayStageObservation {
        beat,
        music_seconds,
        players,
        player_step_indices,
        statistics,
    });
    Ok(())
}
