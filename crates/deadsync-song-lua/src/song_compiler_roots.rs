//! Explicit lexical ownership while assembling existing compiler output.
use super::*;

#[derive(Default)]
pub(super) struct Roots(Vec<Option<Root>>);

/// A growing batch whose values remain pinned until the compiler releases it.
/// Mutable slice access is deliberately absent: every insertion must pin first.
#[derive(Default)]
pub(super) struct RootedValues {
    values: Vec<Value>,
    roots: Roots,
}
impl RootedValues {
    pub fn push(&mut self, vm: &mut Vm, value: Value) -> Result<(), String> {
        self.validate(vm)?;
        self.roots.keep(vm, value)?;
        self.values.push(value);
        Ok(())
    }
    pub fn validate(&self, vm: &Vm) -> Result<(), String> {
        // All pins come from one VM and none can be released individually.
        if !self.values.is_empty() {
            self.roots.value(vm, 0)?;
        }
        Ok(())
    }
    pub fn release(mut self, vm: &mut Vm) -> Result<(), String> {
        self.validate(vm)?;
        self.roots.clear(vm)
    }
}
impl std::ops::Deref for RootedValues {
    type Target = [Value];
    fn deref(&self) -> &[Value] {
        &self.values
    }
}

impl Roots {
    pub fn keep(&mut self, vm: &mut Vm, value: Value) -> Result<usize, String> {
        let pin = vm.pin(value).map_err(|e| e.to_string())?;
        self.0.push(Some(pin));
        Ok(self.0.len() - 1)
    }
    pub fn value(&self, vm: &Vm, index: usize) -> Result<Value, String> {
        vm.rooted(self.0[index].ok_or("released compiler value")?)
            .map_err(|e| e.to_string())
    }
    pub fn release(&mut self, vm: &mut Vm, index: usize) -> Result<(), String> {
        if let Some(pin) = self.0[index].take() {
            vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn validate(&self, vm: &Vm) -> Result<(), String> {
        for pin in self.0.iter().flatten() {
            vm.rooted(*pin).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn clear(&mut self, vm: &mut Vm) -> Result<(), String> {
        for i in 0..self.0.len() {
            self.release(vm, i)?;
        }
        Ok(())
    }
    pub fn take_batch(
        &mut self,
        runtime: &mut SongRuntime,
        batch: &mut SongCallbackTables,
    ) -> Result<Vec<usize>, String> {
        let values = runtime.song_callback_tables(batch)?;
        let result = values
            .into_iter()
            .map(|v| self.keep(&mut runtime.vm, v))
            .collect();
        runtime
            .release_song_callback_tables(batch)
            .map_err(|e| e.to_string())?;
        result
    }
}
impl SongRuntime {
    pub(super) fn compiler_globals(&mut self) -> Result<Value, String> {
        self.vm
            .with_native_stack(|stack| Ok(stack.value(-10002).unwrap()))
            .map_err(|e| e.to_string())
    }
    pub(super) fn compiler_table_field(
        &mut self,
        source: Value,
        name: &str,
    ) -> Result<Value, String> {
        let read = self
            .vm
            .new_host_function_with_data(
                |call| {
                    let key = call.function_data::<String>().unwrap().clone();
                    let value = super::field(call, call.captures()[0], &key)?;
                    if !matches!(value, Value::Nil | Value::Table(_)) {
                        return Err(call.error(b"table expected"));
                    }
                    call.push(value)
                },
                &[source],
                name.to_owned(),
            )
            .map_err(|e| e.to_string())?;
        Ok(self.vm.call(read, &[]).map_err(|e| e.to_string())?[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execute(vm: &mut Vm, source: &str) -> Vec<Value> {
        vm.execute(&deadlib_lua::compile_named(source.as_bytes(), b"=rooted batch").unwrap())
            .unwrap()
            .to_vec()
    }

    #[test]
    fn growing_batch_retains_values_through_gc_errors_and_rejects_other_vms() {
        let mut vm = Vm::new();
        let mut batch = RootedValues::default();
        let first = execute(
            &mut vm,
            "weak=setmetatable({}, {__mode='v'}); local a={}; weak[1]=a; return a",
        )[0];
        batch.push(&mut vm, first).unwrap();
        let second = execute(
            &mut vm,
            "collectgarbage('collect'); assert(weak[1]); local a={}; weak[2]=a; return a",
        )[0];
        batch.push(&mut vm, second).unwrap();
        let mut other = Vm::new();
        assert!(batch.validate(&other).is_err());
        assert!(batch.push(&mut other, Value::Nil).is_err());
        assert_eq!(&*batch, &[first, second]);
        let fail = deadlib_lua::compile_named(
            b"collectgarbage('collect'); assert(weak[1] and weak[2]); error('callback failure')",
            b"=rooted batch error",
        )
        .unwrap();
        assert!(
            vm.execute(&fail)
                .unwrap_err()
                .to_string()
                .contains("callback failure")
        );
        batch.validate(&vm).unwrap();
        assert_eq!(
            execute(
                &mut vm,
                "collectgarbage('collect'); return weak[1]~=nil, weak[2]~=nil"
            ),
            [Value::Boolean(true), Value::Boolean(true)],
        );
        batch.release(&mut vm).unwrap();
        assert_eq!(
            execute(
                &mut vm,
                "collectgarbage('collect'); collectgarbage('collect'); return weak[1]==nil, weak[2]==nil"
            ),
            [Value::Boolean(true), Value::Boolean(true)],
        );
    }
}
