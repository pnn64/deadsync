//! Retained stage/option owners selected at the live scene boundary.
use super::*;
use crate::LUA_PLAYERS;

impl Actors {
    pub(in crate::rust_engine) fn live_frame_owners(
        &self,
        vm: &mut Vm,
        catalog: Value,
        status: Arc<Mutex<[crate::rust_engine::PlayerStateStatus; LUA_PLAYERS]>>,
    ) -> Result<Root, Error> {
        let globals = vm.with_native_stack(|stack| Ok(stack.value(-10002).unwrap()))?;
        let initialize = vm.new_host_function_with_data(
            initialize,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                catalog,
                globals,
            ],
            status,
        )?;
        let batch = vm.call(initialize, &[])?[0];
        vm.pin(batch)
    }
}

fn ordinary(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(owner, key)
}
fn raw_field(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(owner, key)
}
fn invoke(
    call: &mut HostCall<'_>,
    owner: Value,
    name: &str,
    argument: Option<Value>,
) -> Result<Value, Error> {
    call.with_roots(&[owner, argument.unwrap_or(Value::Nil)], |call| {
        let method = ordinary(call, owner, name)?;
        if !matches!(method, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"live statistics method expected"));
        }
        let args = [owner, argument.unwrap_or(Value::Nil)];
        Ok(call
            .call(method, &args[..if argument.is_some() { 2 } else { 1 }])?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let catalog = table(call, call.captures()[2])?;
    let globals = call.captures()[3];
    let batch = call.try_new_table()?;
    call.with_roots(&[Value::Table(batch)], |call| {
        let manager = ordinary(call, globals, "STATSMAN")?;
        table(call, manager)?;
        let stage = invoke(call, manager, "GetCurStageStats", None)?;
        table(call, stage)?;
        call.raw_table_set(batch, Value::Number(1.), stage)?;
        for i in 0..LUA_PLAYERS {
            let player = call.try_intern(crate::player_number_name(i).as_bytes())?;
            let stats = invoke(call, stage, "GetPlayerStageStats", Some(player))?;
            table(call, stats)?;
            call.raw_table_set(batch, Value::Number((i + 2) as f64), stats)?;
        }
        let selected = call.try_intern(b"live_statistics")?;
        call.raw_table_set(catalog, selected, Value::Table(batch))?;
        {
            let status = call
                .function_data::<Arc<Mutex<[crate::rust_engine::PlayerStateStatus; LUA_PLAYERS]>>>()
                .unwrap();
            for player in status.lock().unwrap().iter_mut() {
                player.health = None;
            }
        }
        let screen = raw_field(call, table(call, globals)?, "__songlua_top_screen")?;
        let screen = table(call, screen)?;
        let children = native_children::named_children(call, screen)?;
        let children = table(call, children)?;
        call.with_roots(&[Value::Table(screen), Value::Table(children)], |call| {
            for i in 0..LUA_PLAYERS {
                let meter = raw_field(call, children, &format!("LifeP{}", i + 1))?;
                if meter != Value::Nil {
                    table(call, meter)?;
                    let stats = call.raw_table_get(batch, Value::Number((i + 2) as f64))?;
                    let key = call.try_intern(b"__songlua_life_meter")?;
                    call.raw_table_set(table(call, stats)?, key, meter)?;
                }
            }
            Ok(())
        })?;
        let songs = raw_field(call, catalog, "song_options")?;
        let current = call.raw_table_get(table(call, songs)?, Value::Number(4.))?;
        table(call, current)?;
        call.raw_table_set(batch, Value::Number(4.), current)?;
        let players = raw_field(call, catalog, "player_options")?;
        let players = table(call, players)?;
        for i in 0..LUA_PLAYERS {
            let levels = call.raw_table_get(players, Value::Number((i + 1) as f64))?;
            let current = call.raw_table_get(table(call, levels)?, Value::Number(4.))?;
            table(call, current)?;
            call.raw_table_set(batch, Value::Number((i + 5) as f64), current)?;
        }
        call.push(Value::Table(batch))
    })
}
