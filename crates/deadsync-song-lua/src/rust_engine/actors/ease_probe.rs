//! Function-ease setter instrumentation and temporary observation data.
use super::*;
use crate::SongLuaEaseTarget as Target;

const KEYS: [&str; 3] = [
    "__songlua_probe_methods",
    "__songlua_probe_actors",
    "__songlua_probe_actor_set",
];
pub(in crate::rust_engine) const ACTORS: usize = 7;
pub(in crate::rust_engine) struct Session {
    pub retained: TableId,
    pub pin: Root,
}
pub(in crate::rust_engine) struct Read {
    pub methods: Vec<String>,
    pub target: Result<Option<Target>, Error>,
}
fn global(call: &mut HostCall<'_>, key: &str) -> Result<Value, Error> {
    super::super::field(call, helper(call, 5)?, key)
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
pub(super) fn actor(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    if !store.lock().unwrap().ease_probe_active {
        return Ok(());
    }
    let actors = global(call, KEYS[1])?;
    let Some(actors_id) = optional_table(call, actors)? else {
        return Ok(());
    };
    call.with_roots(&[actors], |call| {
        let seen = global(call, KEYS[2])?;
        let Some(seen_id) = optional_table(call, seen)? else {
            return Ok(());
        };
        call.with_roots(&[seen], |call| {
            if call
                .raw_table_get(seen_id, Value::Table(actor))?
                .is_truthy()
            {
                return Ok(());
            }
            call.raw_table_set(seen_id, Value::Table(actor), Value::Boolean(true))?;
            let index = call.raw_table_len(actors_id)? + 1;
            call.raw_table_set(actors_id, Value::Number(index as f64), Value::Table(actor))
        })
    })
}
fn kind(call: &mut HostCall<'_>, actor: TableId) -> Result<&'static str, Error> {
    let player = sprites::field(call, actor, "__songlua_player_index")?;
    if song_columns::optional_integer(call, player)?.is_none() {
        return Ok("overlay");
    }
    let child = sprites::field(call, actor, "__songlua_player_child_name")?;
    Ok(match initial_state::value_text(call, child)?.as_deref() {
        None => "player",
        Some(v) if v.eq_ignore_ascii_case("NoteField") => "notefield",
        Some(v) if v.eq_ignore_ascii_case("Judgment") => "judgment",
        Some(v) if v.eq_ignore_ascii_case("Combo") => "combo",
        _ => "player-child",
    })
}
pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    id: TableId,
    name: &str,
) -> Result<(), Error> {
    if !matches!(
        name,
        "x" | "y"
            | "z"
            | "rotationx"
            | "rotationy"
            | "rotationz"
            | "zoom"
            | "zoomx"
            | "zoomy"
            | "zoomz"
            | "baserotationx"
            | "baserotationy"
            | "baserotationz"
            | "skewx"
            | "skewy"
    ) || !store.lock().unwrap().ease_probe_active
    {
        return Ok(());
    }
    actor(call, store, id)?;
    let calls = global(call, KEYS[0])?;
    let Some(calls_id) = optional_table(call, calls)? else {
        return Ok(());
    };
    call.with_roots(&[calls], |call| {
        let name = format!("{}.{name}", kind(call, id)?);
        let value = call.try_intern(name.as_bytes())?;
        let index = call.raw_table_len(calls_id)? + 1;
        call.raw_table_set(calls_id, Value::Number(index as f64), value)
    })
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    initial_state::value_text(call, value)?
        .ok_or_else(|| call.error(b"probe method string expected"))
}
fn classify(call: &mut HostCall<'_>, calls: TableId) -> Result<Option<Target>, Error> {
    let targets = [
        Target::PlayerX,
        Target::PlayerY,
        Target::PlayerZ,
        Target::PlayerRotationX,
        Target::PlayerRotationY,
        Target::PlayerRotationZ,
        Target::PlayerSkewX,
        Target::PlayerSkewY,
        Target::PlayerZoom,
        Target::PlayerZoomX,
        Target::PlayerZoomY,
        Target::PlayerZoomZ,
    ];
    let mut flags = 0u16;
    for index in 1usize.. {
        let value = call.raw_table_get(calls, Value::Number(index as f64))?;
        if value == Value::Nil {
            break;
        }
        let value = text(call, value)?;
        let (kind, name) = value.split_once('.').unwrap_or(("player", &value));
        if kind != "player" {
            return Ok(None);
        }
        let flag = match name {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            "rotationx" => 3,
            "rotationy" => 4,
            "rotationz" => 5,
            "skewx" => 6,
            "skewy" => 7,
            "zoom" => 8,
            "zoomx" => 9,
            "zoomy" => 10,
            "zoomz" => 11,
            _ => return Ok(None),
        };
        flags |= 1 << flag;
    }
    Ok((flags.count_ones() == 1).then(|| targets[flags.trailing_zeros() as usize].clone()))
}
impl Actors {
    pub(in crate::rust_engine) fn set_ease_probe_active(&self, active: bool) {
        self.store.lock().unwrap().ease_probe_active = active;
    }
    pub(in crate::rust_engine) fn begin_ease_probe(&self, vm: &mut Vm) -> Result<Session, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let result = (|| {
            let setup = vm.new_host_function(
                |call| {
                    let retained = table(call, call.captures()[2])?;
                    for (i, key) in KEYS.iter().enumerate() {
                        let value = global(call, key)?;
                        call.raw_table_set(retained, Value::Number((i + 1) as f64), value)?;
                    }
                    for slot in [4, 5, ACTORS] {
                        let table = call.try_new_table()?;
                        call.raw_table_set(
                            retained,
                            Value::Number(slot as f64),
                            Value::Table(table),
                        )?;
                    }
                    for (i, key) in KEYS.iter().enumerate() {
                        let value = if i == 2 {
                            Value::Table(call.try_new_table()?)
                        } else {
                            call.raw_table_get(retained, Value::Number((i + 4) as f64))?
                        };
                        super::super::set(call, helper(call, 5)?, key, value)?;
                    }
                    Ok(())
                },
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(retained),
                ],
            )?;
            vm.call(setup, &[])?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(Session { retained, pin })
    }
    pub(in crate::rust_engine) fn finish_ease_probe(
        &self,
        vm: &mut Vm,
        session: &Session,
    ) -> Result<Read, Error> {
        let output = Arc::new(Mutex::new(None));
        let callback = vm.new_host_function_with_data(
            |call| {
                let retained = table(call, call.captures()[2])?;
                let calls = table(call, call.raw_table_get(retained, Value::Number(4.))?)?;
                let actors = table(call, call.raw_table_get(retained, Value::Number(5.))?)?;
                let keep = table(
                    call,
                    call.raw_table_get(retained, Value::Number(ACTORS as f64))?,
                )?;
                let mut methods = Vec::new();
                for i in 1usize.. {
                    let v = call.raw_table_get(calls, Value::Number(i as f64))?;
                    if v == Value::Nil {
                        break;
                    }
                    methods.push(text(call, v)?);
                }
                for i in 1usize.. {
                    let v = call.raw_table_get(actors, Value::Number(i as f64))?;
                    if v == Value::Nil {
                        break;
                    }
                    table(call, v)?;
                    call.raw_table_set(keep, Value::Number(i as f64), v)?;
                }
                let target = classify(call, calls);
                for (i, key) in KEYS.iter().enumerate() {
                    let old = call.raw_table_get(retained, Value::Number((i + 1) as f64))?;
                    super::super::set(call, helper(call, 5)?, key, old)?;
                }
                *call
                    .function_data::<Arc<Mutex<Option<Read>>>>()
                    .unwrap()
                    .lock()
                    .unwrap() = Some(Read { methods, target });
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(session.retained),
            ],
            output.clone(),
        )?;
        vm.call(callback, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
}
