//! Existing source-message probes and per-load deferred message identities.
use super::*;
#[path = "capture_state.rs"]
mod state;
use std::path::PathBuf;

pub(in crate::rust_engine) struct Deferred {
    pub actor: TableId,
    pub command: String,
    pub detail: String,
    pub observed: bool,
}
#[derive(Clone)]
enum Failure {
    Source(String),
    Replay(String),
    Changed,
}
#[derive(Default)]
struct Output {
    state: Option<state::Snapshot>,
    blocks: Vec<crate::SongLuaOverlayCommandBlock>,
}
#[derive(Clone)]
struct Run {
    store: Shared,
    name: String,
    output: Arc<Mutex<Output>>,
}
pub(in crate::rust_engine) struct CommandSession {
    retained: TableId,
    pin: Root,
    data: Run,
}
impl CommandSession {
    pub(in crate::rust_engine) fn runner(&self, vm: &Vm) -> Result<Value, Error> {
        vm.raw_table_get(self.retained, Value::Number(3.))
    }
    pub(in crate::rust_engine) fn restore(&self, vm: &mut Vm, raw: bool) -> Result<(), Error> {
        let finish = vm.raw_table_get(self.retained, Value::Number(4.))?;
        vm.call(finish, &[Value::Boolean(raw)])?;
        Ok(())
    }
    pub(in crate::rust_engine) fn blocks(&self) -> Vec<crate::SongLuaOverlayCommandBlock> {
        std::mem::take(&mut self.data.output.lock().unwrap().blocks)
    }
    pub(in crate::rust_engine) fn release(self, vm: &mut Vm) -> Result<(), Error> {
        vm.unpin(self.pin)?;
        Ok(())
    }
}
#[derive(Default)]
struct SoundOutput {
    paths: Option<Result<Vec<PathBuf>, Error>>,
}
pub(in crate::rust_engine) struct SoundSession {
    retained: TableId,
    pin: Root,
    output: Arc<Mutex<SoundOutput>>,
}
impl SoundSession {
    pub(in crate::rust_engine) fn collect_restore(
        &self,
        vm: &mut Vm,
    ) -> Result<Result<Vec<PathBuf>, Error>, Error> {
        let finish = vm.raw_table_get(self.retained, Value::Number(3.))?;
        vm.call(finish, &[])?;
        Ok(self.output.lock().unwrap().paths.take().unwrap())
    }
    pub(in crate::rust_engine) fn release(self, vm: &mut Vm) -> Result<(), Error> {
        vm.unpin(self.pin)?;
        Ok(())
    }
}
fn lookup(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<Option<Value>, Error> {
    let key = call.try_intern(name.as_bytes())?;
    let value = call.table_get(Value::Table(actor), key)?;
    match value {
        Value::Nil => Ok(None),
        Value::Function(_) | Value::Native(_) => Ok(Some(value)),
        _ => Err(call.error(b"actor command must be a function")),
    }
}
impl Actors {
    pub(in crate::rust_engine) fn lookup_capture_command(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
    ) -> Result<Option<Root>, Error> {
        let read = vm.new_host_function_with_data(
            |call| {
                let name = call.function_data::<String>().unwrap().clone();
                let actor = table(call, call.captures()[2])?;
                let value = lookup(call, actor, &name)?.unwrap_or(Value::Nil);
                call.push(value)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            name.to_owned(),
        )?;
        let value = vm.call(read, &[])?[0];
        if value == Value::Nil {
            Ok(None)
        } else {
            vm.pin(value).map(Some)
        }
    }
    pub(in crate::rust_engine) fn prepare_command_capture(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
    ) -> Result<CommandSession, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let data = Run {
            store: self.store.clone(),
            name: name.to_owned(),
            output: Arc::new(Mutex::new(Output::default())),
        };
        let result = (|| {
            let captures = [
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
                Value::Table(retained),
            ];
            let setup = vm.new_host_function_with_data(
                |call| {
                    let data = call.function_data::<Run>().unwrap().clone();
                    let actor = table(call, call.captures()[2])?;
                    let retained = table(call, call.captures()[3])?;
                    let saved = state::Snapshot::capture(call, &data.store, actor, retained)?;
                    data.output.lock().unwrap().state = Some(saved);
                    Ok(())
                },
                &captures,
                data.clone(),
            )?;
            vm.call(setup, &[])?;
            let runner = vm.new_host_function_with_data(run, &captures, data.clone())?;
            vm.raw_table_set(retained, Value::Number(3.), runner)?;
            let finish = vm.new_host_function_with_data(
                |call| {
                    let data = call.function_data::<Run>().unwrap().clone();
                    let saved = data.output.lock().unwrap().state.take().unwrap();
                    saved.restore(call, &data.store, arg(call, 0) == Value::Boolean(true))
                },
                &captures,
                data.clone(),
            )?;
            vm.raw_table_set(retained, Value::Number(4.), finish)?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(CommandSession {
            retained,
            pin,
            data,
        })
    }
    pub(in crate::rust_engine) fn message_command_names(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Vec<String>, Error> {
        let output = Arc::new(Mutex::new(None));
        let scan = vm.new_host_function_with_data(
            |call| {
                let out = call
                    .function_data::<Arc<Mutex<Option<Vec<String>>>>>()
                    .unwrap()
                    .clone();
                let actor = call.captures()[0];
                let names = {
                    let mut stack = call.native_stack()?;
                    stack.push(actor)?;
                    let actor = stack.top_offset();
                    stack.push(Value::Nil)?;
                    let mut names = Vec::new();
                    while stack.next_table(actor)? {
                        if matches!(stack.value(-2), Some(Value::String(_)))
                            && matches!(
                                stack.value(-1),
                                Some(Value::Function(_) | Value::Native(_))
                            )
                        {
                            let bytes = stack.to_bytes(-2)?.unwrap();
                            if let Ok(name) = std::str::from_utf8(bytes)
                                && name.ends_with("MessageCommand")
                            {
                                names.push(name.to_owned());
                            }
                        }
                        stack.set_top(stack.top() - 1)?;
                    }
                    names.sort_unstable();
                    names
                };
                *out.lock().unwrap() = Some(names);
                Ok(())
            },
            &[actor],
            output.clone(),
        )?;
        vm.call(scan, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
    pub(in crate::rust_engine) fn begin_message_sounds(
        &self,
        vm: &mut Vm,
    ) -> Result<SoundSession, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let output = Arc::new(Mutex::new(SoundOutput::default()));
        let result = (|| {
            let captures = [
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(retained),
            ];
            let setup = vm.new_host_function(
                |call| {
                    let retained = table(call, call.captures()[2])?;
                    let key = call.try_intern(crate::SONG_LUA_SOUND_CALLS_KEY.as_bytes())?;
                    let globals = helper(call, 5)?;
                    let old = call.table_get(globals, key)?;
                    call.raw_table_set(retained, Value::Number(1.), old)?;
                    let next = call.try_new_table()?;
                    call.raw_table_set(retained, Value::Number(2.), Value::Table(next))?;
                    call.table_set(globals, key, Value::Table(next))
                },
                &captures,
            )?;
            vm.call(setup, &[])?;
            let finish = vm.new_host_function_with_data(
                |call| {
                    let output = call
                        .function_data::<Arc<Mutex<SoundOutput>>>()
                        .unwrap()
                        .clone();
                    let retained = table(call, call.captures()[2])?;
                    let paths = table(call, call.raw_table_get(retained, Value::Number(2.))?)?;
                    let result = function_action::read_paths(call, paths);
                    output.lock().unwrap().paths = Some(result);
                    let old = call.raw_table_get(retained, Value::Number(1.))?;
                    let key = call.try_intern(crate::SONG_LUA_SOUND_CALLS_KEY.as_bytes())?;
                    call.table_set(helper(call, 5)?, key, old)
                },
                &captures,
                output.clone(),
            )?;
            vm.raw_table_set(retained, Value::Number(3.), finish)?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(SoundSession {
            retained,
            pin,
            output,
        })
    }
    pub(in crate::rust_engine) fn defer_actor_message(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
        error: &str,
    ) -> Result<String, Error> {
        self.defer_message(vm, actor, name, Failure::Source(error.to_owned()))
    }
    pub(in crate::rust_engine) fn defer_dynamic_actor_message(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
        error: Option<&str>,
    ) -> Result<String, Error> {
        self.defer_message(
            vm,
            actor,
            name,
            error.map_or(Failure::Changed, |e| Failure::Replay(e.to_owned())),
        )
    }
    fn defer_message(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
        failure: Failure,
    ) -> Result<String, Error> {
        let output = Arc::new(Mutex::new(None));
        let record = vm.new_host_function_with_data(
            |call| {
                let (store, name, failure, output) = call
                    .function_data::<(Shared, String, Failure, Arc<Mutex<Option<String>>>)>()
                    .unwrap()
                    .clone();
                let actor = table(call, call.captures()[2])?;
                let prior = store
                    .lock()
                    .unwrap()
                    .deferred_messages
                    .iter()
                    .find(|m| m.actor == actor && m.command == name)
                    .map(|m| m.detail.clone());
                let detail = if let Some(prior) = prior {
                    prior
                } else {
                    let mut label = "Actor".to_owned();
                    let mut display = None;
                    for key in ["__songlua_actor_type", "Name"] {
                        let value: Result<Option<String>, Error> = (|| {
                            let key = call.try_intern(key.as_bytes())?;
                            let v = call.table_get(Value::Table(actor), key)?;
                            let Some(id) = call.coerce_string(v)? else {
                                return Ok(None);
                            };
                            Ok(call
                                .string_bytes(id)
                                .and_then(|b| std::str::from_utf8(b).ok())
                                .map(str::to_owned))
                        })();
                        if let Ok(Some(value)) = value {
                            if key == "Name" {
                                display = Some(value)
                            } else {
                                label = value
                            }
                        }
                    }
                    if let Some(display) = display.filter(|n| !n.trim().is_empty()) {
                        label = format!("{label} '{display}'");
                    }
                    let detail = match failure {
                        Failure::Source(error) => format!("{label}.{name}: {error}"),
                        Failure::Replay(error) => format!("{label}.{name} cannot be replayed for stable cross-actor capture: {error}"),
                        Failure::Changed => format!("{label}.{name} changes cross-actor targets or effects between runs"),
                    };
                    let helpers = table(call, call.captures()[1])?;
                    let key = call.try_intern(b"deferred_message_actors")?;
                    let roots = match call.raw_table_get(helpers, key)? {
                        Value::Nil => {
                            let roots = call.try_new_table()?;
                            call.raw_table_set(helpers, key, Value::Table(roots))?;
                            roots
                        }
                        value => table(call, value)?,
                    };
                    call.raw_table_set(roots, Value::Table(actor), Value::Boolean(true))?;
                    store.lock().unwrap().deferred_messages.push(Deferred {
                        actor,
                        command: name,
                        detail: detail.clone(),
                        observed: false,
                    });
                    detail
                };
                *output.lock().unwrap() = Some(detail);
                Ok(())
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            (
                self.store.clone(),
                name.to_owned(),
                failure,
                output.clone(),
            ),
        )?;
        vm.call(record, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
    #[cfg(feature = "test-support")]
    pub(in crate::rust_engine) fn deferred_message_states(
        &self,
    ) -> Vec<(TableId, String, String, bool)> {
        self.store
            .lock()
            .unwrap()
            .deferred_messages
            .iter()
            .map(|m| (m.actor, m.command.clone(), m.detail.clone(), m.observed))
            .collect()
    }
    #[cfg(feature = "test-support")]
    pub(in crate::rust_engine) fn seed_message_sound_block(
        &self,
        actor: Value,
        played: bool,
    ) -> bool {
        let Value::Table(actor) = actor else {
            return false;
        };
        let mut store = self.store.lock().unwrap();
        let Some(actor) = store.actors.get_mut(&actor) else {
            return false;
        };
        actor
            .capture
            .get_or_insert_with(super::super::capture::Capture::default)
            .write(
                crate::SongLuaOverlayStateDelta {
                    sound_play: Some(played),
                    ..Default::default()
                },
                false,
                None,
            );
        true
    }
    #[cfg(feature = "test-support")]
    pub(in crate::rust_engine) fn clear_deferred_message_states(
        &self,
        vm: &mut Vm,
    ) -> Result<(), Error> {
        self.store.lock().unwrap().deferred_messages.clear();
        let key = vm.try_intern(b"deferred_message_actors")?;
        vm.raw_table_set(self.helpers, key, Value::Nil)
    }
}
fn run(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let Some(command) = lookup(call, actor, &data.name)? else {
        return Ok(());
    };
    call.with_roots(&[command], |call| {
        function_capture::reset(call, &data.store, actor)?;
        let params = super::probe::defaults(call, &data.name)?;
        call.with_roots(&[params.unwrap_or(Value::Nil)], |call| {
            run_guarded_command(
                call,
                &data.store,
                actor,
                data.name.as_bytes().to_vec(),
                command,
                params,
                true,
            )
        })?;
        let blocks = data
            .store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .and_then(|a| a.capture.as_mut())
            .map(|c| c.snapshot().blocks)
            .unwrap_or_default();
        data.output.lock().unwrap().blocks = blocks;
        Ok(())
    })
}
