//! Retained inputs for the compiler's stable and deferred message passes.
use super::*;

pub(in crate::rust_engine) struct Commands {
    pub rows: Vec<(usize, String, Value)>,
    pub pin: Root,
}
pub(in crate::rust_engine) struct DeferredBatch {
    pub rows: Vec<Deferred>,
    pub pin: Root,
}
type Rows = Arc<Mutex<Vec<(usize, String, Value)>>>;
impl Actors {
    pub(in crate::rust_engine) fn message_command_functions(
        &self,
        vm: &mut Vm,
        actors: &[Value],
    ) -> Result<Commands, Error> {
        let retained = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(retained))?;
        let output: Rows = Arc::new(Mutex::new(Vec::new()));
        let result = (|| {
            let (inputs, inputs_pin) = retained::values(vm, actors.iter().copied())?;
            let scan =
                vm.new_host_function_with_data(
                    |call| {
                        let output = call.function_data::<Rows>().unwrap().clone();
                        let retained = table(call, call.captures()[0])?;
                        let inputs = table(call, call.captures()[1])?;
                        let actors = (1..=call.raw_table_len(inputs)?)
                            .map(|i| call.raw_table_get(inputs, Value::Number(i as f64)))
                            .collect::<Result<Vec<_>, _>>()?;
                        let rows = {
                            let mut stack = call.native_stack()?;
                            let mut rows = Vec::new();
                            for (source, actor) in actors.into_iter().enumerate() {
                                stack.push(actor)?;
                                let actor = stack.top_offset();
                                stack.push(Value::Nil)?;
                                while stack.next_table(actor)? {
                                    if matches!(stack.value(-2), Some(Value::String(_)))
                                        && let Some(
                                            function @ (Value::Function(_) | Value::Native(_)),
                                        ) = stack.value(-1)
                                        && let Some(bytes) = stack.to_bytes(-2)?
                                        && let Ok(name) = std::str::from_utf8(bytes)
                                        && name.ends_with("MessageCommand")
                                    {
                                        rows.push((source, name.to_owned(), function));
                                    }
                                    stack.set_top(stack.top() - 1)?;
                                }
                                stack.set_top(stack.top() - 1)?;
                            }
                            rows
                        };
                        let roots = rows.iter().map(|r| r.2).collect::<Vec<_>>();
                        call.with_roots(&roots, |call| {
                            for (i, function) in roots.iter().enumerate() {
                                call.raw_table_set(
                                    retained,
                                    Value::Number((i + 1) as f64),
                                    *function,
                                )?;
                            }
                            Ok(())
                        })?;
                        *output.lock().unwrap() = rows;
                        Ok(())
                    },
                    &[Value::Table(retained), Value::Table(inputs)],
                    output.clone(),
                );
            vm.unpin(inputs_pin)?;
            let scan = scan?;
            vm.call(scan, &[])?;
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(pin)?;
            return Err(error);
        }
        Ok(Commands {
            rows: std::mem::take(&mut *output.lock().unwrap()),
            pin,
        })
    }
    pub(in crate::rust_engine) fn take_deferred_messages(
        &self,
        vm: &mut Vm,
    ) -> Result<DeferredBatch, Error> {
        let key = vm.try_intern(b"deferred_message_actors")?;
        let roots = vm.raw_table_get(self.helpers, key)?;
        // Native removes its app data before retrying. Keep the removed actors
        // independently rooted, including actors already retired from a scene.
        let pin = vm.pin(roots)?;
        if let Err(error) = vm.raw_table_set(self.helpers, key, Value::Nil) {
            vm.unpin(pin)?;
            return Err(error);
        }
        let rows = std::mem::take(&mut self.store.lock().unwrap().deferred_messages);
        Ok(DeferredBatch { rows, pin })
    }
}
