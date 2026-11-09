//! Existing compiler audit records. This entire module is test-support only.
use super::*;
use crate::{SongLuaBoolWrite, SongLuaSkinWrite, SongLuaSpeedWrite, SongLuaStateCall};

#[derive(Clone, Default)]
pub(super) struct Observations(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    active: bool,
    skin_active: bool,
    output: Output,
}
#[derive(Default)]
pub(super) struct Output {
    booleans: Vec<SongLuaBoolWrite>,
    speeds: Vec<SongLuaSpeedWrite>,
    skins: Vec<SongLuaSkinWrite>,
    calls: Vec<SongLuaStateCall>,
}
impl Output {
    pub(super) fn apply<A, L>(self, output: &mut crate::CompiledSongLuaData<A, L>) {
        output.boolean_writes.extend(self.booleans);
        output.speed_writes.extend(self.speeds);
        output.noteskin_writes.extend(self.skins);
        output.state_calls.extend(self.calls);
    }
}
pub(super) struct Boolean<'a> {
    pub key: &'a str,
    pub previous: bool,
    pub current: bool,
    pub chained: bool,
}
pub(super) struct SpeedBefore {
    player: usize,
    fields: [[f32; 2]; 4],
}
pub(super) struct Speed<'a> {
    pub key: &'a str,
    pub before: Option<SpeedBefore>,
    pub failed: bool,
    pub chained: bool,
}
impl Observations {
    pub(super) fn start(&self) {
        *self.0.lock().unwrap() = State {
            active: true,
            skin_active: true,
            output: Output::default(),
        };
    }
    pub(super) fn stop(&self) {
        self.0.lock().unwrap().active = false;
    }
    pub(super) fn skin_active(&self, active: bool) {
        self.0.lock().unwrap().skin_active = active;
    }
    pub(super) fn enabled(&self) -> bool {
        self.0.lock().unwrap().active
    }
    pub(super) fn take(&self) -> Output {
        let mut state = self.0.lock().unwrap();
        state.active = false;
        std::mem::take(&mut state.output)
    }
    pub(super) fn boolean(
        &self,
        call: &mut HostCall<'_>,
        globals: Value,
        owner: TableId,
        value: Boolean<'_>,
    ) -> Result<(), Error> {
        if !self.enabled() {
            return Ok(());
        }
        let Some(player) = player(call, globals, owner, false)? else {
            return Ok(());
        };
        let [beat, second] = clock(call, globals)?;
        let write = SongLuaBoolWrite {
            player,
            key: value.key.to_owned(),
            beat,
            second,
            previous: value.previous,
            current: value.current,
            chained: value.chained,
        };
        let mut state = self.0.lock().unwrap();
        if state.output.booleans.len() >= 2_000_000 {
            drop(state);
            return Err(call.error(b"boolean option audit exceeded two million writes"));
        }
        state.output.booleans.push(write);
        Ok(())
    }
    pub(super) fn speed_before(
        &self,
        call: &mut HostCall<'_>,
        globals: Value,
        owner: TableId,
    ) -> Result<Option<SpeedBefore>, Error> {
        if !self.enabled() {
            return Ok(None);
        }
        let Some(player) = player(call, globals, owner, true)? else {
            return Ok(None);
        };
        Ok(Some(SpeedBefore {
            player,
            fields: speed_fields(call, owner)?,
        }))
    }
    pub(super) fn speed(
        &self,
        call: &mut HostCall<'_>,
        globals: Value,
        owner: TableId,
        value: Speed<'_>,
    ) -> Result<(), Error> {
        let Some(before) = value.before else {
            return Ok(());
        };
        let [beat, second] = clock(call, globals)?;
        let write = SongLuaSpeedWrite {
            player: before.player,
            key: value.key.to_owned(),
            beat,
            second,
            previous: before.fields,
            current: speed_fields(call, owner)?,
            failed: value.failed,
            chained: value.chained,
        };
        let mut state = self.0.lock().unwrap();
        if state.output.speeds.len() >= 2_000_000 {
            drop(state);
            return Err(call.error(b"speed option audit exceeded two million calls"));
        }
        state.output.speeds.push(write);
        Ok(())
    }
    pub(super) fn skin(
        &self,
        call: &mut HostCall<'_>,
        globals: Value,
        owner: TableId,
        key: &str,
        previous: String,
    ) -> Result<(), Error> {
        let active = {
            let state = self.0.lock().unwrap();
            state.active && state.skin_active
        };
        if !active {
            return Ok(());
        }
        let Some(player) = player(call, globals, owner, false)? else {
            return Ok(());
        };
        let [beat, second] = clock(call, globals)?;
        let name = raw(call, owner, "__songlua_noteskin_name")?;
        let current = if name == Value::Nil {
            String::new()
        } else {
            text(call, name)?
        };
        let current = if current.is_empty() {
            crate::SONG_LUA_DEFAULT_NOTESKIN_NAME.into()
        } else {
            current
        };
        let write = SongLuaSkinWrite {
            player,
            key: key.to_owned(),
            previous,
            current,
            beat,
            second,
        };
        let mut state = self.0.lock().unwrap();
        if state.output.skins.len() >= 2_000_000 {
            drop(state);
            return Err(call.error(b"noteskin audit exceeded two million writes"));
        }
        state.output.skins.push(write);
        Ok(())
    }
    pub(super) fn state_call(
        &self,
        call: &mut HostCall<'_>,
        globals: Value,
        operation: &'static str,
        args: Vec<serde_json::Value>,
    ) -> Result<(), Error> {
        if !self.enabled() {
            return Ok(());
        }
        let [beat, second] = clock(call, globals)?;
        let write = SongLuaStateCall {
            operation,
            args,
            beat: beat as f32,
            second: second as f32,
        };
        let mut state = self.0.lock().unwrap();
        if state.output.calls.len() >= 2_000_000 {
            drop(state);
            return Err(call.error(b"game state audit exceeded two million calls"));
        }
        state.output.calls.push(write);
        Ok(())
    }
}

fn table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    if let Value::Table(table) = value {
        Ok(table)
    } else {
        Err(call.error(b"audit table expected"))
    }
}
fn get(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(owner, key)
}
fn raw(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(owner, key)
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let string = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"audit string expected"))?;
    String::from_utf8(call.string_bytes(string).unwrap().to_vec())
        .map_err(|e| call.error(e.to_string().as_bytes()))
}
fn number(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    if let Value::Number(value) = value {
        return Ok(value);
    }
    let read = call.new_host_function(
        |call| {
            let value = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(value)?;
            let number = stack.number(-1)?;
            stack.push(Value::Number(number))?;
            stack.finish(1)
        },
        &[value],
    )?;
    let Value::Number(value) = call.call(read, &[])?[0] else {
        unreachable!()
    };
    Ok(value)
}
fn clock(call: &mut HostCall<'_>, globals: Value) -> Result<[f64; 2], Error> {
    let runtime = get(call, globals, crate::SONG_LUA_RUNTIME_KEY)?;
    table(call, runtime)?;
    call.with_roots(&[runtime], |call| {
        let beat = get(call, runtime, crate::SONG_LUA_RUNTIME_BEAT_KEY)?;
        let beat = number(call, beat)?;
        let second = get(call, runtime, crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
        Ok([beat, number(call, second)?])
    })
}
fn player(
    call: &mut HostCall<'_>,
    globals: Value,
    owner: TableId,
    optional: bool,
) -> Result<Option<usize>, Error> {
    for (index, key) in crate::SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        let current = get(call, globals, key)?;
        if current == Value::Nil && optional {
            continue;
        }
        if table(call, current)? == owner {
            return Ok(Some(index));
        }
    }
    Ok(None)
}
fn speed_fields(call: &mut HostCall<'_>, owner: TableId) -> Result<[[f32; 2]; 4], Error> {
    let key = call.try_intern(b"__songlua_player_option_speeds")?;
    let speeds = call.raw_table_get(owner, key)?;
    let speeds = if speeds == Value::Nil {
        let speeds = call.try_new_table()?;
        call.raw_table_set(owner, key, Value::Table(speeds))?;
        speeds
    } else {
        table(call, speeds)?
    };
    let mut fields = [[0.; 2]; 4];
    for (index, (field, key, default)) in [
        ("__songlua_time_spacing", "_spacing", 0.),
        ("__songlua_speedmod_xmod", "xmod", 1.),
        ("__songlua_speedmod_cmod", "cmod", 200.),
        ("__songlua_speedmod_mmod", "mmod", 0.),
    ]
    .into_iter()
    .enumerate()
    {
        let amount = raw(call, owner, field)?;
        let amount = if amount == Value::Nil {
            default
        } else {
            number(call, amount)? as f32
        };
        let speed = raw(call, speeds, key)?;
        fields[index] = [
            amount,
            if speed == Value::Nil {
                1.
            } else {
                number(call, speed)? as f32
            },
        ];
    }
    Ok(fields)
}
