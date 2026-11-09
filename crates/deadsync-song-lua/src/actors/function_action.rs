//! Outer state and output around the compiler's existing function-action probe.
use super::*;
use crate::function_snapshot::Snapshot;
use std::path::PathBuf;

const BROADCASTS: usize = 2;
const SOUNDS: usize = 4;
const ROOTS: usize = 6;
const GLOBALS: [&str; 3] = [
    crate::SONG_LUA_BROADCASTS_KEY,
    crate::SONG_LUA_SOUND_CALLS_KEY,
    crate::SONG_LUA_SUPPRESS_BROADCAST_KEY,
];
type Broadcasts = Vec<(String, bool)>;
#[derive(Default)]
struct State {
    columns: Vec<ColumnSnapshot>,
    before: i64,
    side_effects: i64,
    broadcasts: Option<Result<Broadcasts, Error>>,
    sounds: Option<Result<Vec<PathBuf>, Error>>,
}
struct ColumnSnapshot {
    player: TableId,
    field: Option<TableId>,
    columns: Option<TableId>,
    handlers: Vec<HandlerSnapshot>,
}
struct HandlerSnapshot {
    handler: TableId,
    spline: TableId,
    values: Vec<(&'static str, Value)>,
    spline_values: Vec<(&'static str, Value)>,
}
#[derive(Clone)]
pub(super) struct Scope {
    pub(super) retained: TableId,
    data: Arc<Mutex<State>>,
}
pub(crate) struct Session {
    scope: Scope,
    pin: Root,
}
impl Session {
    pub(crate) fn output(&self) -> Result<(Broadcasts, Vec<PathBuf>, i64), Error> {
        let mut state = self.scope.data.lock().unwrap();
        let broadcasts = state.broadcasts.take().unwrap()?;
        let sounds = state.sounds.take().unwrap()?;
        Ok((broadcasts, sounds, state.side_effects))
    }
    pub(crate) fn release(self, vm: &mut Vm) -> Result<(), Error> {
        vm.unpin(self.pin)?;
        Ok(())
    }
}
impl Actors {
    pub(crate) fn begin_column_capture(&self, vm: &mut Vm) -> Result<Session, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let scope = Scope {
            retained,
            data: Arc::new(Mutex::new(State::default())),
        };
        let result = (|| {
            let roots = vm.try_new_table()?;
            vm.raw_table_set(retained, Value::Number(ROOTS as f64), Value::Table(roots))?;
            let setup = vm.new_host_function_with_data(
                |call| {
                    let scope = call.function_data::<Scope>().unwrap().clone();
                    let columns = snapshot_columns(call, &scope)?;
                    scope.data.lock().unwrap().columns = columns;
                    Ok(())
                },
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(retained),
                ],
                scope.clone(),
            )?;
            vm.call(setup, &[])?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(Session { scope, pin })
    }
    pub(crate) fn restore_column_capture(
        &self,
        vm: &mut Vm,
        session: &Session,
    ) -> Result<(), Error> {
        let finish = vm.new_host_function_with_data(
            |call| {
                let scope = call.function_data::<Scope>().unwrap().clone();
                let columns = std::mem::take(&mut scope.data.lock().unwrap().columns);
                restore_columns(call, columns)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(session.scope.retained),
            ],
            session.scope.clone(),
        )?;
        vm.call(finish, &[])?;
        Ok(())
    }
    pub(crate) fn snapshot_function_action(
        &self,
        vm: &mut Vm,
        function: Value,
    ) -> Result<Snapshot, Error> {
        Snapshot::capture_functions(vm, self.registry, &[function])
    }
    pub(crate) fn begin_function_action(&self, vm: &mut Vm) -> Result<Session, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let scope = Scope {
            retained,
            data: Arc::new(Mutex::new(State::default())),
        };
        let result = (|| {
            let roots = vm.try_new_table()?;
            vm.raw_table_set(retained, Value::Number(ROOTS as f64), Value::Table(roots))?;
            let setup = vm.new_host_function_with_data(
                setup,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(retained),
                ],
                scope.clone(),
            )?;
            vm.call(setup, &[])?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(Session { scope, pin })
    }
    pub(crate) fn capture_function_action(
        &self,
        vm: &mut Vm,
        actors: Vec<(usize, TableId)>,
        function: Value,
        random: &super::super::random::RandomState,
        session: &Session,
    ) -> (
        Result<super::super::SongActorFunctionBlocks, Error>,
        bool,
        Option<Root>,
    ) {
        self.capture_scoped_function(
            vm,
            actors,
            function,
            None,
            random,
            Some(session.scope.clone()),
        )
    }
}

pub(super) fn install(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    let code = compile_named(
        br#"
        local type,next,rawset=type,next,rawset
        local function clone(value)
            if type(value)~='table' then return value end
            local result={}
            for key,item in next,value do rawset(result,clone(key),clone(item)) end
            return result
        end
        return clone
    "#,
        b"=function action column values",
    )?;
    let function = vm.execute(&code)?[0];
    super::super::put(vm, helpers, "function_action_clone", function)
}
fn global(call: &mut HostCall<'_>, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(helper(call, 5)?, key)
}
fn field(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<Value, Error> {
    sprites::field(call, actor, name)
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
impl Scope {
    fn keep(&self, call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
        if !matches!(value, Value::Nil | Value::Boolean(_) | Value::Number(_)) {
            let roots = table(
                call,
                call.raw_table_get(self.retained, Value::Number(ROOTS as f64))?,
            )?;
            call.raw_table_set(roots, value, Value::Boolean(true))?;
        }
        Ok(value)
    }
    fn copy(&self, call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
        call.with_roots(&[value], |call| {
            let clone = native_children::helper_function(call, b"function_action_clone")?;
            let result = call.call(clone, &[value])?[0];
            self.keep(call, result)
        })
    }
    pub(super) fn before_actor_restore(&self, call: &mut HostCall<'_>) -> Result<(), Error> {
        // Output errors are reported only after all ordinary cleanup, matching
        // native. Column restoration precedes both side-effect measurement and
        // actor reset, so authored restoration callbacks retain that ordering.
        let broadcasts = table(
            call,
            call.raw_table_get(self.retained, Value::Number(BROADCASTS as f64))?,
        )?;
        let sounds = table(
            call,
            call.raw_table_get(self.retained, Value::Number(SOUNDS as f64))?,
        )?;
        let broadcasts = read_broadcasts(call, broadcasts);
        let sounds = read_paths(call, sounds);
        let columns = std::mem::take(&mut self.data.lock().unwrap().columns);
        restore_columns(call, columns)?;
        let count = side_effect_count(call)?;
        let mut state = self.data.lock().unwrap();
        state.broadcasts = Some(broadcasts);
        state.sounds = Some(sounds);
        state.side_effects = count.saturating_sub(state.before);
        Ok(())
    }
    pub(super) fn after_actor_restore(&self, call: &mut HostCall<'_>) -> Result<(), Error> {
        let globals = helper(call, 5)?;
        for (i, name) in GLOBALS.iter().enumerate() {
            let old = call.raw_table_get(self.retained, Value::Number((i * 2 + 1) as f64))?;
            let key = call.try_intern(name.as_bytes())?;
            call.table_set(globals, key, old)?;
        }
        Ok(())
    }
}
fn setup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let scope = call.function_data::<Scope>().unwrap().clone();
    let before = side_effect_count(call)?;
    let columns = snapshot_columns(call, &scope)?;
    {
        let mut data = scope.data.lock().unwrap();
        data.before = before;
        data.columns = columns;
    }
    for (i, name) in GLOBALS.iter().enumerate() {
        let old = global(call, name)?;
        call.raw_table_set(scope.retained, Value::Number((i * 2 + 1) as f64), old)?;
        let next = if i == 2 {
            Value::Boolean(true)
        } else {
            Value::Table(call.try_new_table()?)
        };
        if i < 2 {
            call.raw_table_set(scope.retained, Value::Number((i * 2 + 2) as f64), next)?;
        }
        let key = call.try_intern(name.as_bytes())?;
        call.table_set(helper(call, 5)?, key, next)?;
    }
    Ok(())
}
fn snapshot_columns(call: &mut HostCall<'_>, scope: &Scope) -> Result<Vec<ColumnSnapshot>, Error> {
    let mut result = Vec::new();
    for name in [
        "__songlua_top_screen_player_1",
        "__songlua_top_screen_player_2",
    ] {
        let value = global(call, name)?;
        let value = scope.keep(call, value)?;
        let Some(player) = optional_table(call, value)? else {
            continue;
        };
        let key = call.try_intern(b"__songlua_children")?;
        let children = call.raw_table_get(player, key)?;
        let children = if call.raw_table_len(player)? == 0 {
            optional_table(call, children)?
        } else {
            None
        };
        let value = if let Some(children) = children {
            let key = call.try_intern(b"NoteField")?;
            call.raw_table_get(children, key)?
        } else {
            let named = native_children::named_children(call, player)?;
            call.with_roots(&[named], |call| {
                let named = table(call, named)?;
                field(call, named, "NoteField")
            })?
        };
        let value = scope.keep(call, value)?;
        let note_field = optional_table(call, value)?;
        let columns = if let Some(note_field) = note_field {
            let value = field(call, note_field, "__songlua_note_columns")?;
            let value = scope.keep(call, value)?;
            optional_table(call, value)?
        } else {
            None
        };
        let mut handlers = Vec::new();
        if let Some(columns) = columns {
            for i in 1usize.. {
                let value = call.raw_table_get(columns, Value::Number(i as f64))?;
                if value == Value::Nil {
                    break;
                }
                let value = scope.keep(call, value)?;
                let column = table(call, value)?;
                for key in [
                    "__songlua_pos_handler",
                    "__songlua_rot_handler",
                    "__songlua_zoom_handler",
                ] {
                    let value = field(call, column, key)?;
                    let value = scope.keep(call, value)?;
                    let Some(handler) = optional_table(call, value)? else {
                        continue;
                    };
                    let value = field(call, handler, "__songlua_spline")?;
                    let value = scope.keep(call, value)?;
                    let Some(spline) = optional_table(call, value)? else {
                        continue;
                    };
                    let mut values = Vec::new();
                    let mut spline_values = Vec::new();
                    for key in [
                        "__songlua_spline_mode",
                        "__songlua_subtract_song_beat",
                        "__songlua_receptor_t",
                        "__songlua_beats_per_t",
                    ] {
                        let value = field(call, handler, key)?;
                        values.push((key, scope.copy(call, value)?));
                    }
                    for key in [
                        "__songlua_spline_size",
                        "__songlua_spline_loop",
                        "__songlua_spline_points",
                    ] {
                        let value = field(call, spline, key)?;
                        let value = if key == "__songlua_spline_loop" {
                            scope.keep(call, value)?
                        } else {
                            scope.copy(call, value)?
                        };
                        spline_values.push((key, value));
                    }
                    handlers.push(HandlerSnapshot {
                        handler,
                        spline,
                        values,
                        spline_values,
                    });
                }
            }
        }
        result.push(ColumnSnapshot {
            player,
            field: note_field,
            columns,
            handlers,
        });
    }
    Ok(result)
}
fn restore_columns(call: &mut HostCall<'_>, columns: Vec<ColumnSnapshot>) -> Result<(), Error> {
    for saved in columns {
        let registry = native_children::helper_function(call, b"native_registry")?;
        let children = call.call(registry, &[Value::Table(saved.player)])?[0];
        call.with_roots(&[children], |call| {
            let key = call.try_intern(b"NoteField")?;
            call.table_set(children, key, saved.field.map_or(Value::Nil, Value::Table))?;
            if let Some(field) = saved.field {
                sprites::set(
                    call,
                    field,
                    "__songlua_note_columns",
                    saved.columns.map_or(Value::Nil, Value::Table),
                )?;
            }
            Ok(())
        })?;
        for handler in saved.handlers {
            for (key, value) in handler.values {
                sprites::set(call, handler.handler, key, value)?;
            }
            for (key, value) in handler.spline_values {
                sprites::set(call, handler.spline, key, value)?;
            }
        }
    }
    Ok(())
}
fn side_effect_count(call: &mut HostCall<'_>) -> Result<i64, Error> {
    let value = global(call, crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY)?;
    if value == Value::Nil {
        return Ok(0);
    }
    let result = Arc::new(Mutex::new(0_i64));
    let read = call.new_host_function_with_data(
        |call| {
            let result = call.function_data::<Arc<Mutex<i64>>>().unwrap().clone();
            let value = call.captures()[0];
            let count = {
                let mut stack = call.native_stack()?;
                stack.push(value)?;
                stack
                    .to_integer(-1)
                    .ok_or_else(|| stack.error(b"side-effect integer expected"))?
                    as i64
            };
            *result.lock().unwrap() = count;
            Ok(())
        },
        &[value],
        result.clone(),
    )?;
    call.call(read, &[])?;
    Ok(*result.lock().unwrap())
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).unwrap())
        .map(str::to_owned)
        .map_err(|e| call.error(e.to_string().as_bytes()))
}
fn read_broadcasts(call: &mut HostCall<'_>, values: TableId) -> Result<Broadcasts, Error> {
    let mut result = Vec::new();
    for i in 1usize.. {
        let value = call.raw_table_get(values, Value::Number(i as f64))?;
        if value == Value::Nil {
            break;
        }
        call.with_roots(&[value], |call| {
            let entry = table(call, value)?;
            let message = field(call, entry, "message")?;
            if message == Value::Nil {
                return Ok(());
            }
            let message = text(call, message)?;
            let has_params = field(call, entry, "has_params")?.is_truthy();
            result.push((message, has_params));
            Ok(())
        })?;
    }
    Ok(result)
}
pub(super) fn read_paths(call: &mut HostCall<'_>, values: TableId) -> Result<Vec<PathBuf>, Error> {
    let mut result = Vec::new();
    for i in 1usize.. {
        let value = call.raw_table_get(values, Value::Number(i as f64))?;
        if value == Value::Nil {
            break;
        }
        result.push(PathBuf::from(text(call, value)?));
    }
    Ok(result)
}
