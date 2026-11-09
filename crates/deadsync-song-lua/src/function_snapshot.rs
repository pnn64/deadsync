//! Shared script state around compiler probes. Actor state has its own capture.
use deadlib_lua::{Error, HostCall, NativeStack, TableId, Value, Vm};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

struct SavedTable {
    table: Value,
    entries: Vec<(Value, Value)>,
}
struct SavedCell {
    function: Value,
    index: usize,
    value: Value,
}
pub(super) struct Snapshot {
    owner: TableId,
    retained: TableId,
    previous: Option<TableId>,
    count: i32,
    tables: Vec<SavedTable>,
    cells: Vec<SavedCell>,
}
impl Snapshot {
    /// The owner traces one private table containing every saved handle. Rust
    /// collections alone do not keep Lua allocations alive across a probe.
    pub(super) fn capture(vm: &mut Vm, owner: TableId, actors: &[TableId]) -> Result<Self, Error> {
        let functions = vm.with_native_stack(|stack| {
            let mut functions = Vec::new();
            let mut seen = HashSet::new();
            for &actor in actors {
                stack.push(Value::Table(actor))?;
                let index = stack.top_offset();
                stack.push(Value::Nil)?;
                while stack.next_table(index)? {
                    let value = stack.value(-1).unwrap();
                    if let Value::Function(id) = value
                        && seen.insert(id)
                    {
                        functions.push(value);
                    }
                    stack.set_top(stack.top() - 1)?;
                }
                stack.set_top(stack.top() - 1)?;
            }
            Ok(functions)
        })?;
        Self::capture_inputs(vm, owner, &functions, actors)
    }

    /// Capture the selected command roots, including their custom environments,
    /// referenced helper functions, shared cells and cyclic local tables.
    pub(super) fn capture_functions(
        vm: &mut Vm,
        owner: TableId,
        functions: &[Value],
    ) -> Result<Self, Error> {
        Self::capture_inputs(vm, owner, functions, &[])
    }

    fn capture_inputs(
        vm: &mut Vm,
        owner: TableId,
        functions: &[Value],
        actors: &[TableId],
    ) -> Result<Self, Error> {
        // Root inputs before allocating the attachment. A selected function can
        // have been removed from its source actor before the probe begins.
        let mut pins = Vec::new();
        let result = (|| {
            pins.push(vm.pin(Value::Table(owner))?);
            for &actor in actors {
                pins.push(vm.pin(Value::Table(actor))?);
            }
            for &function in functions {
                if !matches!(function, Value::Function(_) | Value::Native(_)) {
                    return vm.with_native_stack(|stack| Err(stack.error(b"function expected")));
                }
                pins.push(vm.pin(function)?);
            }
            Self::capture_rooted_functions(vm, owner, functions, actors)
        })();
        for pin in pins {
            vm.unpin(pin)?;
        }
        result
    }

    fn capture_rooted_functions(
        vm: &mut Vm,
        owner: TableId,
        functions: &[Value],
        actors: &[TableId],
    ) -> Result<Self, Error> {
        let previous = vm.table_host_state(owner)?;
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let attached = (|| {
            if let Some(previous) = previous {
                vm.raw_table_set(retained, Value::Number(1.), Value::Table(previous))?;
            }
            vm.set_table_host_state(owner, Some(retained))
        })();
        vm.unpin(pin)?;
        attached?;
        let mut saved = Self {
            owner,
            retained,
            previous,
            count: 1,
            tables: Vec::new(),
            cells: Vec::new(),
        };
        let result = vm.with_native_stack(|stack| {
            stack.set_top(0)?;
            stack.push(Value::Table(retained))?;
            saved.capture_stack(stack, functions, actors, true)
        });
        if let Err(error) = result {
            saved.release(vm)?;
            return Err(error);
        }
        Ok(saved)
    }

    /// Nested command calls preserve locals and referenced table contents,
    /// without taking the outer action scope's shallow global binding snapshot.
    pub(super) fn capture_locals(
        call: &mut HostCall<'_>,
        owner: TableId,
        function: Value,
    ) -> Result<Self, Error> {
        let output = Arc::new(Mutex::new(None::<Self>));
        let capture = call.new_host_function_with_data(
            Self::capture_locals_callback,
            &[Value::Table(owner), function],
            output.clone(),
        )?;
        if let Err(error) = call.call(capture, &[]) {
            if let Some(saved) = output.lock().unwrap().take() {
                saved.release_call(call)?;
            }
            return Err(error);
        }
        Ok(output.lock().unwrap().take().unwrap())
    }
    fn capture_locals_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Table(owner) = call.captures()[0] else {
            unreachable!()
        };
        let function = call.captures()[1];
        let output = call
            .function_data::<Arc<Mutex<Option<Self>>>>()
            .unwrap()
            .clone();
        let previous = call.table_host_state(owner)?;
        let retained = call.try_new_table()?;
        call.with_roots(&[Value::Table(retained)], |call| {
            if let Some(previous) = previous {
                call.raw_table_set(retained, Value::Number(1.), Value::Table(previous))?;
            }
            call.set_table_host_state(owner, Some(retained))
        })?;
        let mut saved = Self {
            owner,
            retained,
            previous,
            count: 1,
            tables: Vec::new(),
            cells: Vec::new(),
        };
        let result = (|| {
            // This callback owns one stack lease; its caller may already have
            // taken another lease for actor-state inspection in the same probe.
            let mut stack = call.native_stack()?;
            stack.set_top(0)?;
            stack.push(Value::Table(retained))?;
            saved.capture_stack(&mut stack, &[function], &[], false)
        })();
        if let Err(error) = result {
            saved.release_call(call)?;
            return Err(error);
        }
        *output.lock().unwrap() = Some(saved);
        Ok(())
    }

    fn keep(&mut self, stack: &mut NativeStack<'_, '_>, value: Value) -> Result<(), Error> {
        if matches!(
            value,
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::LightUserdata(_)
        ) {
            return Ok(());
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| stack.error(b"too many saved script values"))?;
        stack.push(value)?;
        stack.raw_set_integer(1, self.count)
    }

    fn capture_stack(
        &mut self,
        stack: &mut NativeStack<'_, '_>,
        roots: &[Value],
        actors: &[TableId],
        bindings: bool,
    ) -> Result<(), Error> {
        // Actor identities belong to this existing startup snapshot as well as
        // their command functions, even when no global or closure references them.
        for &actor in actors {
            self.keep(stack, Value::Table(actor))?;
        }
        let globals = stack.value(-10002).unwrap();
        let mut functions = Vec::new();
        let mut seen_functions = HashSet::new();
        for &value in roots {
            self.keep(stack, value)?;
            // Native host closures own binding data, not Lua upvalue cells.
            if let Value::Function(id) = value
                && seen_functions.insert(id)
            {
                functions.push(value);
            }
        }
        let Value::Table(global_id) = globals else {
            unreachable!()
        };
        let mut seen_tables = HashSet::from([global_id]);
        let mut targets = vec![globals];
        if bindings {
            // Resolve every root environment before snapshotting shared bindings.
            for &function in &functions {
                let target = environment(stack, function)?;
                let Value::Table(id) = target else {
                    unreachable!()
                };
                if seen_tables.insert(id) {
                    self.keep(stack, target)?;
                    targets.push(target);
                }
            }
            for target in targets {
                self.save_table(stack, target)?;
            }
        }
        seen_functions.clear();
        let mut pending = functions;
        while let Some(value) = pending.pop() {
            match value {
                Value::Function(id) if seen_functions.insert(id) => {
                    let target = environment(stack, value)?;
                    self.keep(stack, target)?;
                    stack.push(value)?;
                    let mut bytes = Vec::new();
                    stack.dump(|_, part| {
                        bytes.extend_from_slice(part);
                        Ok(0)
                    })?;
                    stack.set_top(stack.top() - 1)?;
                    for namespace in [Some(globals), (target != globals).then_some(target)]
                        .into_iter()
                        .flatten()
                    {
                        for (key, value) in self.entries(stack, namespace)? {
                            if matches!(key, Value::String(_))
                                && matches!(value, Value::Function(_) | Value::Table(_))
                            {
                                stack.push(key)?;
                                let name = stack.to_bytes(-1)?.unwrap();
                                let referenced = !name.is_empty()
                                    && bytes.windows(name.len()).any(|part| part == name);
                                stack.set_top(stack.top() - 1)?;
                                if referenced {
                                    self.keep(stack, value)?;
                                    pending.push(value);
                                }
                            }
                        }
                    }
                    stack.push(value)?;
                    let function_index = stack.top_offset();
                    let mut index = 1;
                    while let Some(name) = stack.get_upvalue(function_index, index)? {
                        let name = name.strip_suffix(&[0]).unwrap_or(name);
                        let include = name != b"_ENV";
                        let invalid = std::str::from_utf8(name).err();
                        if let Some(error) = invalid {
                            return Err(stack.error(error.to_string().as_bytes()));
                        }
                        let captured = stack.value(-1).unwrap();
                        if include {
                            self.keep(stack, captured)?;
                            pending.push(captured);
                            self.cells.push(SavedCell {
                                function: value,
                                index,
                                value: captured,
                            });
                        }
                        stack.set_top(stack.top() - 1)?;
                        index += 1;
                    }
                    stack.set_top(stack.top() - 1)?;
                }
                Value::Table(id)
                    if seen_tables.insert(id)
                        && raw_field(stack, value, b"__songlua_actor_type")? == Value::Nil =>
                {
                    let entries = self.save_table(stack, value)?;
                    pending.extend(entries.into_iter().map(|(_, value)| value));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn entries(
        &mut self,
        stack: &mut NativeStack<'_, '_>,
        table: Value,
    ) -> Result<Vec<(Value, Value)>, Error> {
        self.keep(stack, table)?;
        stack.push(table)?;
        let index = stack.top_offset();
        stack.push(Value::Nil)?;
        let mut entries = Vec::new();
        while stack.next_table(index)? {
            let key = stack.value(-2).unwrap();
            let value = stack.value(-1).unwrap();
            entries.push((key, value));
            stack.set_top(stack.top() - 1)?;
        }
        stack.set_top(stack.top() - 1)?;
        Ok(entries)
    }
    fn save_table(
        &mut self,
        stack: &mut NativeStack<'_, '_>,
        table: Value,
    ) -> Result<Vec<(Value, Value)>, Error> {
        let entries = self.entries(stack, table)?;
        for &(key, value) in &entries {
            self.keep(stack, key)?;
            self.keep(stack, value)?;
        }
        self.tables.push(SavedTable {
            table,
            entries: entries.clone(),
        });
        Ok(entries)
    }

    pub(super) fn restore(self, vm: &mut Vm) -> Result<(), Error> {
        let result = vm.with_native_stack(|stack| {
            stack.set_top(0)?;
            stack.push(Value::Table(self.retained))?;
            // Restore cells first; aliases continue to point at the same cells
            // and tables, including cells owned by suspended Lua frames.
            for cell in &self.cells {
                stack.push(cell.function)?;
                stack.push(cell.value)?;
                stack.set_upvalue(2, cell.index)?;
                stack.set_top(1)?;
            }
            for saved in &self.tables {
                stack.push(saved.table)?;
                // Collect keys before deleting: do not rely on deleted-key
                // cursor behavior while the table changes shape.
                stack.push(Value::Nil)?;
                let mut keys = Vec::new();
                while stack.next_table(2)? {
                    keys.push(stack.value(-2).unwrap());
                    stack.set_top(stack.top() - 1)?;
                }
                // Keys remain reachable from the target until removed; these
                // raw writes do not invoke Lua or perform a GC step.
                for key in keys {
                    stack.push(key)?;
                    stack.push(Value::Nil)?;
                    stack.set_table(2, true)?;
                }
                for &(key, value) in &saved.entries {
                    stack.push(key)?;
                    stack.push(value)?;
                    stack.set_table(2, true)?;
                }
                stack.set_top(1)?;
            }
            Ok(())
        });
        self.release(vm)?;
        result
    }
    pub(super) fn discard(self, vm: &mut Vm) -> Result<(), Error> {
        self.release(vm)
    }
    fn release_call(&self, call: &mut HostCall<'_>) -> Result<(), Error> {
        if call.table_host_state(self.owner)? == Some(self.retained) {
            call.set_table_host_state(self.owner, self.previous)?;
        } else {
            for index in 2..=self.count {
                call.raw_table_set(self.retained, Value::Number(f64::from(index)), Value::Nil)?;
            }
        }
        Ok(())
    }
    fn release(&self, vm: &mut Vm) -> Result<(), Error> {
        if vm.table_host_state(self.owner)? == Some(self.retained) {
            vm.set_table_host_state(self.owner, self.previous)?;
        } else {
            // A later capture may link this attachment into its own chain.
            // Keep that chain intact but release our saved objects.
            for index in 2..=self.count {
                vm.raw_table_set(self.retained, Value::Number(f64::from(index)), Value::Nil)?;
            }
        }
        Ok(())
    }
}

fn raw_field(stack: &mut NativeStack<'_, '_>, target: Value, name: &[u8]) -> Result<Value, Error> {
    stack.push(target)?;
    stack.push_bytes(name)?;
    stack.get_table(-2, true)?;
    let value = stack.value(-1).unwrap();
    stack.set_top(stack.top() - 2)?;
    Ok(value)
}
fn environment(stack: &mut NativeStack<'_, '_>, function: Value) -> Result<Value, Error> {
    stack.push(function)?;
    stack.get_environment(-1)?;
    let env = stack.value(-1).unwrap();
    let target = raw_field(stack, env, b"__songlua_env_target")?;
    stack.set_top(stack.top() - 2)?;
    match target {
        Value::Nil => Ok(env),
        Value::Table(_) => Ok(target),
        _ => Err(stack.error(b"__songlua_env_target: table expected")),
    }
}
