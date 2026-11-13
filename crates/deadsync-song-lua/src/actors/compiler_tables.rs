//! Ordinary debug lookups and rooted discovery of callback-owned schedule tables.
use super::*;
use deadlib_lua::NativeDebugFrame;

#[derive(Clone)]
pub(crate) enum Query {
    Updates {
        names: Vec<String>,
        nested: bool,
    },
    Global {
        namespace: String,
        functions: Vec<String>,
        names: Vec<String>,
    },
    ReadsGlobal(String),
}
#[derive(Clone)]
struct Binding {
    query: Query,
}
impl Actors {
    pub(crate) fn read_compiler_tables(
        &self,
        vm: &mut Vm,
        root: Value,
        query: Query,
    ) -> Result<(Root, bool), Error> {
        let input = vm.pin(root)?;
        let result = self.read_compiler_tables_rooted(vm, root, query);
        vm.unpin(input)?;
        result
    }
    fn read_compiler_tables_rooted(
        &self,
        vm: &mut Vm,
        root: Value,
        query: Query,
    ) -> Result<(Root, bool), Error> {
        let output = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(output))?;
        let result = (|| {
            let read = vm.new_host_function_with_data(
                read,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    root,
                    Value::Table(output),
                ],
                Binding { query },
            )?;
            Ok(vm.call(read, &[])?[0] == Value::Boolean(true))
        })();
        match result {
            Ok(found) => Ok((pin, found)),
            Err(error) => {
                vm.unpin(pin)?;
                Err(error)
            }
        }
    }
}
fn optional_function(call: &HostCall<'_>, value: Value) -> Result<Option<Value>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Function(_) | Value::Native(_) => Ok(Some(value)),
        _ => Err(call.error(b"function expected")),
    }
}
fn field(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    sprites::field(call, actor, key)
}
fn getupvalue(call: &mut HostCall<'_>) -> Result<Option<Value>, Error> {
    let debug = super::super::field(call, helper(call, 5)?, "debug")?;
    if debug == Value::Nil {
        return Ok(None);
    }
    let id = table(call, debug)?;
    call.with_roots(&[debug], |call| {
        let value = field(call, id, "getupvalue")?;
        optional_function(call, value)
    })
}
fn upvalue_count(call: &mut HostCall<'_>, function: Value) -> Result<usize, Error> {
    let inspect = call.new_host_function(
        |call| {
            let function = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(function)?;
            let record = NativeDebugFrame::from_index(stack.thread_id(), 0);
            let n = stack.get_info(record, b">u")?.upvalues.unwrap();
            stack.push(Value::Number(n as f64))?;
            stack.finish(1)
        },
        &[function],
    )?;
    let Value::Number(n) = call.call(inspect, &[])?[0] else {
        unreachable!()
    };
    Ok(n as usize)
}
struct Reader {
    output: TableId,
    getupvalue: Value,
    names: Vec<String>,
    tables: HashSet<TableId>,
    functions: HashSet<usize>,
    count: usize,
    nested: bool,
}
impl Reader {
    fn function(&mut self, call: &mut HostCall<'_>, function: Value) -> Result<(), Error> {
        if self.nested
            && !self
                .functions
                .insert(call.object_pointer(function).unwrap())
        {
            return Ok(());
        }
        let count = upvalue_count(call, function)?;
        for i in 1..=count {
            let values = call.call(self.getupvalue, &[function, Value::Number(i as f64)])?;
            let values = [
                values.first().copied().unwrap_or(Value::Nil),
                values.get(1).copied().unwrap_or(Value::Nil),
            ];
            call.with_roots(&values, |call| {
                let matches = if let Value::String(name) = values[0] {
                    let name = std::str::from_utf8(call.string_bytes(name).unwrap())
                        .map_err(|error| call.error(error.to_string().as_bytes()))?;
                    self.names.iter().any(|candidate| candidate == name)
                } else {
                    false
                };
                match values[1] {
                    Value::Table(t) if matches && self.tables.insert(t) => {
                        self.count += 1;
                        call.raw_table_set(self.output, Value::Number(self.count as f64), values[1])
                    }
                    child @ (Value::Function(_) | Value::Native(_)) if self.nested => {
                        self.function(call, child)
                    }
                    _ => Ok(()),
                }
            })?;
        }
        Ok(())
    }
    fn actor(&mut self, call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
        let value = field(call, actor, "__songlua_update_function")?;
        if let Some(function) = optional_function(call, value)? {
            call.with_roots(&[function], |call| self.function(call, function))?;
        }
        if let Some(command) = recurring_command(call, actor)? {
            let value = field(call, actor, &command)?;
            if let Some(function) = optional_function(call, value)? {
                call.with_roots(&[function], |call| self.function(call, function))?;
            }
        }
        for i in 1usize.. {
            let value = call.raw_table_get(actor, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            if let Value::Table(child) = value {
                call.with_roots(&[value], |call| self.actor(call, child))?;
            }
        }
        Ok(())
    }
}

// Retain the real reader's deduplication sets between isolated unit probes.
// Its VM captures trace the output tables and debug callback, as in `read`.
#[cfg(feature = "test-support")]
impl Actors {
    pub(crate) fn function_tables_probe(
        &self,
        vm: &mut Vm,
        names: &[&str],
        nested: bool,
    ) -> Result<Root, Error> {
        let Value::Table(debug) = vm.get_global(b"debug")? else {
            return vm.with_native_stack(|stack| Err(stack.error(b"debug table expected")));
        };
        let key = vm.try_intern(b"getupvalue")?;
        let getupvalue = vm.raw_table_get(debug, key)?;
        let output = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(output))?;
        let result = (|| {
            let reader = Reader {
                output,
                getupvalue,
                names: names.iter().map(|name| (*name).to_owned()).collect(),
                tables: HashSet::new(),
                functions: HashSet::new(),
                count: 0,
                nested,
            };
            let function = vm.new_host_function_with_data(
                probe_function_tables,
                &[Value::Table(output), getupvalue],
                Arc::new(Mutex::new(Some(reader))),
            )?;
            vm.pin(function)
        })();
        vm.unpin(pin)?;
        result
    }
}

#[cfg(feature = "test-support")]
fn probe_function_tables(call: &mut HostCall<'_>) -> Result<(), Error> {
    let function = call.arguments().first().copied().unwrap_or(Value::Nil);
    let function =
        optional_function(call, function)?.ok_or_else(|| call.error(b"function expected"))?;
    let shared = call
        .function_data::<Arc<Mutex<Option<Reader>>>>()
        .unwrap()
        .clone();
    let mut reader = shared
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| call.error(b"function table probe is already active"))?;
    let first = reader.count;
    let result = reader.function(call, function);
    let last = reader.count;
    let output = reader.output;
    *shared.lock().unwrap() = Some(reader);
    result?;
    for index in first + 1..=last {
        let value = call.raw_table_get(output, Value::Number(index as f64))?;
        call.push(value)?;
    }
    Ok(())
}
fn contains(call: &mut HostCall<'_>, function: Value, name: &str) -> Result<bool, Error> {
    let inspect = call.new_host_function_with_data(
        |call| {
            let needle = call.function_data::<String>().unwrap().as_bytes().to_vec();
            let function = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(function)?;
            let mut bytes = Vec::new();
            stack.dump(|_, block| {
                bytes.extend_from_slice(block);
                Ok(0)
            })?;
            let found = bytes.windows(needle.len()).any(|window| window == needle);
            stack.push(Value::Boolean(found))?;
            stack.finish(1)
        },
        &[function],
        name.to_owned(),
    )?;
    Ok(call.call(inspect, &[])?[0] == Value::Boolean(true))
}
fn recurring_command(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<String>, Error> {
    let value = field(call, actor, "__songlua_recurring_update_command")?;
    if value != Value::Nil {
        return initial_state::value_text(call, value);
    }
    let command = call
        .store()
        .actors
        .get(&actor)
        .and_then(|actor| actor.capture.as_ref())
        .and_then(|capture| capture.recurring.command.clone());
    command
        .map(|mut command| {
            command.extend_from_slice(b"Command");
            String::from_utf8(command).map_err(|error| call.error(error.to_string().as_bytes()))
        })
        .transpose()
}

fn reads_global(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<bool, Error> {
    let value = field(call, actor, "__songlua_update_function")?;
    if let Some(function) = optional_function(call, value)?
        && contains(call, function, name)?
    {
        return Ok(true);
    }
    if let Some(command) = recurring_command(call, actor)? {
        // Only this branch of the native global-name predicate suppresses a
        // malformed/missing recurring handler. Table discovery propagates it.
        let function = field(call, actor, &command)
            .and_then(|v| optional_function(call, v))
            .ok()
            .flatten();
        if let Some(function) = function
            && contains(call, function, name)?
        {
            return Ok(true);
        }
    }
    for i in 1usize.. {
        let value = call.raw_table_get(actor, Value::Number(i as f64))?;
        if value == Value::Nil {
            break;
        }
        if let Value::Table(child) = value
            && call.with_roots(&[value], |call| reads_global(call, child, name))?
        {
            return Ok(true);
        }
    }
    let class = field(call, actor, "__songlua_actor_type")?;
    if initial_state::value_text(call, class)?
        .is_some_and(|s| s.eq_ignore_ascii_case("SongMeterDisplay"))
    {
        let stream = native_children::named_child(call, actor, b"Stream")?;
        if stream == Value::Nil {
            return Ok(false);
        }
        let id = table(call, stream)?;
        return call.with_roots(&[stream], |call| {
            super::super::set(call, stream, "__songlua_parent", Value::Table(actor))?;
            reads_global(call, id, name)
        });
    }
    Ok(false)
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { query } = call.function_data::<Binding>().unwrap().clone();
    let root = call.captures()[2];
    let output = table(call, call.captures()[3])?;
    let (source, names, nested, functions) = match query {
        Query::ReadsGlobal(name) => {
            let found = if let Value::Table(actor) = root {
                reads_global(call, actor, &name)?
            } else {
                false
            };
            return call.push(Value::Boolean(found));
        }
        Query::Updates { names, nested } => {
            if !matches!(root, Value::Table(_)) {
                return call.push(Value::Table(output));
            }
            (root, names, nested, None)
        }
        Query::Global {
            namespace,
            functions,
            names,
        } => {
            let source = super::super::field(call, helper(call, 5)?, &namespace)?;
            if source == Value::Nil {
                return call.push(Value::Table(output));
            }
            table(call, source)?;
            (source, names, true, Some(functions))
        }
    };
    call.with_roots(&[source], |call| {
        let Some(getupvalue) = getupvalue(call)? else {
            return call.push(Value::Table(output));
        };
        call.with_roots(&[getupvalue], |call| {
            let mut reader = Reader {
                output,
                getupvalue,
                names,
                tables: HashSet::new(),
                functions: HashSet::new(),
                count: 0,
                nested,
            };
            let actor = table(call, source)?;
            if let Some(functions) = functions {
                for name in functions {
                    let value = field(call, actor, &name)?;
                    if let Some(function) = optional_function(call, value)? {
                        call.with_roots(&[function], |call| reader.function(call, function))?;
                    }
                }
            } else {
                reader.actor(call, actor)?;
            }
            call.push(Value::Table(output))
        })
    })
}
