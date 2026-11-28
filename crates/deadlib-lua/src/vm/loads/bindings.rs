//! Reuse empty constant buffers after an unbudgeted sweep.
use super::{Buffer, LoadedProgram, Memory, ProgramData, Value, fallible};

const MAX_VALUES: usize = 32;
const MAX_PER_SIZE: usize = 32;

pub(in crate::vm) struct Bindings {
    pub constants: Buffer<Buffer<Value>>,
    pub values: Buffer<Value>,
}

#[derive(Default)]
pub(in crate::vm) struct BindingPool {
    bins: Option<Box<[Vec<Bindings>; MAX_VALUES + 1]>>,
}

impl BindingPool {
    pub(in crate::vm) fn take(
        &mut self,
        program: &ProgramData,
        memory: &Memory,
    ) -> Option<Bindings> {
        let [prototype] = &*program.prototypes else {
            return None;
        };
        if !prototype.debug.upvalue_names().is_empty()
            || memory.allocator().is_some()
            || memory.budget_usage().is_some()
        {
            return None;
        }
        self.bins
            .as_mut()?
            .get_mut(prototype.constants().len())?
            .pop()
    }

    pub(in crate::vm) fn recycle(&mut self, program: &mut LoadedProgram) {
        let [values] = &*program.constants else {
            return;
        };
        let size = values.capacity();
        if size > MAX_VALUES
            || !program.upvalue_names.is_empty()
            || program.constants.capacity() != 1
        {
            return;
        }
        if self.bins.is_none() {
            self.bins = fallible::boxed(std::array::from_fn(|_| Vec::new())).ok();
        }
        let Some(bins) = &mut self.bins else { return };
        let bin = &mut bins[size];
        if bin.len() == MAX_PER_SIZE || fallible::reserve(bin, 1).is_err() {
            return;
        }
        let memory = program.constants.memory().clone();
        let mut constants = std::mem::replace(&mut program.constants, Buffer::new_in(memory));
        let mut values = constants.pop().expect("one retired constant buffer");
        // No bound Value may be retained outside the program arena.
        values.clear();
        bin.push(Bindings { constants, values });
    }
}

#[cfg(test)]
#[test]
fn a_collected_binding_reuses_storage_with_fresh_values() {
    use crate::{Vm, arena::index as slot_index, compile};

    let mut vm = Vm::new();
    let old = compile(b"return 'old', 41").unwrap();
    let old_function = vm.load_fresh(&old).unwrap();
    let old_program = vm.functions[slot_index(old_function.0)].program;
    let address = vm.programs[old_program].constants[0].as_ptr();
    drop(old);
    vm.collect_garbage().unwrap();
    assert!(!vm.functions.contains(old_function.0));

    let new = compile(b"return 'new', 52").unwrap();
    let new_function = vm.load_fresh(&new).unwrap();
    let new_program = vm.functions[slot_index(new_function.0)].program;
    assert_eq!(vm.programs[new_program].constants[0].as_ptr(), address);
    let results = vm.call(Value::Function(new_function), &[]).unwrap();
    let [Value::String(text), Value::Number(52.0)] = *results else {
        panic!("fresh binding returned an unexpected value");
    };
    assert_eq!(vm.string_bytes(text), Some(&b"new"[..]));
}
