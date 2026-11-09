//! Explicit lexical ownership while assembling existing compiler output.
use super::*;

#[derive(Default)]
pub(super) struct Roots(Vec<Option<Root>>);
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
        runtime: &mut RustSongRuntime,
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
impl RustSongRuntime {
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
