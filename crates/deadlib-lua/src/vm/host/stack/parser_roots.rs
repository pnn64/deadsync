//! Native-reader roots follow incremental parser/decoder events.
use super::*;

impl NativeStack<'_, '_> {
    pub(super) fn apply_load_roots(&mut self, index: usize) -> Result<(), Error> {
        use crate::parser::NativeRootEvent;
        let storage_before = self.call.vm.loads[index]
            .native
            .as_ref()
            .unwrap()
            .storage_bytes();
        let job = &mut self.call.vm.loads[index];
        let events = if job.binary_mode {
            job.decoder
                .as_mut()
                .map_or_else(Vec::new, |decoder| decoder.take_native_events())
        } else if job.native.as_ref().is_some_and(|r| r.started) {
            job.parser.take_native_events()
        } else {
            Vec::new()
        };
        for event in events {
            if matches!(event, NativeRootEvent::Open) {
                let job = &self.call.vm.loads[index];
                let source = job
                    .native
                    .as_ref()
                    .unwrap()
                    .active
                    .last()
                    .map(|(_, p)| self.call.vm.parser_prototypes[slot_index(p.0)].source);
                let source = source.unwrap_or_else(|| {
                    let name = if self.call.vm.loads[index].binary_mode {
                        b"=?".to_vec()
                    } else {
                        self.call.vm.loads[index].name.to_vec()
                    };
                    self.intern(&name)
                });
                let id = ParserPrototypeId(self.call.vm.parser_prototypes.insert(
                    loads::ParserPrototype {
                        source,
                        constants: Vec::new(),
                        children: Vec::new(),
                        debug_strings: Vec::new(),
                    },
                ));
                let table =
                    (!self.call.vm.loads[index].binary_mode).then(|| self.call.vm.new_table());
                self.call.vm.loads[index]
                    .native
                    .as_mut()
                    .unwrap()
                    .active
                    .push((table, id));
                if self.call.vm.loads[index].binary_mode {
                    self.call.vm.loads[index]
                        .native
                        .as_mut()
                        .unwrap()
                        .prototypes
                        .push(id);
                }
                self.call.vm.gc_new_value(Value::ParserPrototype(id));
                if let Some(table) = table {
                    self.push(Value::Table(table))?;
                }
                self.push(Value::ParserPrototype(id))?;
                continue;
            }
            let (table, id) = *self.call.vm.loads[index]
                .native
                .as_ref()
                .unwrap()
                .active
                .last()
                .expect("active parser prototype");
            let before = self.call.vm.parser_prototypes[slot_index(id.0)].heap_bytes();
            match event {
                NativeRootEvent::Close => {
                    self.call.vm.loads[index]
                        .native
                        .as_mut()
                        .unwrap()
                        .active
                        .pop();
                    if table.is_some() {
                        self.call.vm.loads[index]
                            .native
                            .as_mut()
                            .unwrap()
                            .prototypes
                            .push(id);
                    }
                    let count = 1 + usize::from(table.is_some());
                    let top = self
                        .top()
                        .checked_sub(count)
                        .ok_or_else(|| self.error(b"native parser roots removed"))?;
                    self.set_top(top)?;
                    if let Some((_, parent)) = self.call.vm.loads[index]
                        .native
                        .as_ref()
                        .unwrap()
                        .active
                        .last()
                        .copied()
                    {
                        let p = &mut self.call.vm.parser_prototypes[slot_index(parent.0)];
                        let before = p.heap_bytes();
                        p.children.push(Value::ParserPrototype(id));
                        let growth = p.heap_bytes() - before;
                        self.call.vm.parser_prototypes.note_alloc(growth);
                        self.call
                            .vm
                            .gc_barrier(Value::ParserPrototype(parent), Value::ParserPrototype(id));
                    }
                }
                NativeRootEvent::Anchor(bytes) => {
                    let key = self.intern(&bytes);
                    let table = table.expect("text parser constants table");
                    if self.call.vm.tables[slot_index(table.0)].get(key) == Value::Nil {
                        self.call.vm.raw_set(table, key, Value::Boolean(true))?;
                    }
                }
                NativeRootEvent::Constant(constant) => {
                    let value = match constant {
                        Constant::Nil => Value::Nil,
                        Constant::Boolean(value) => Value::Boolean(value),
                        Constant::Number(value) => Value::Number(value),
                        Constant::String(bytes) => self.intern(&bytes),
                    };
                    let p = &mut self.call.vm.parser_prototypes[slot_index(id.0)];
                    let ordinal = p.constants.len();
                    p.constants.push(value);
                    self.call.vm.gc_barrier(Value::ParserPrototype(id), value);
                    if let Some(table) = table {
                        let key = if value == Value::Nil {
                            Value::Table(table)
                        } else {
                            value
                        };
                        self.call
                            .vm
                            .raw_set(table, key, Value::Number(ordinal as f64))?;
                    }
                }
                NativeRootEvent::Source(bytes) => {
                    let value = self.intern(&bytes);
                    self.call.vm.parser_prototypes[slot_index(id.0)].source = value;
                    self.call.vm.gc_barrier(Value::ParserPrototype(id), value);
                }
                NativeRootEvent::DebugString(bytes) => {
                    let value = self.intern(&bytes);
                    self.call.vm.parser_prototypes[slot_index(id.0)]
                        .debug_strings
                        .push(value);
                    self.call.vm.gc_barrier(Value::ParserPrototype(id), value);
                }
                NativeRootEvent::Open => unreachable!(),
            }
            let growth = self.call.vm.parser_prototypes[slot_index(id.0)]
                .heap_bytes()
                .saturating_sub(before);
            self.call.vm.parser_prototypes.note_alloc(growth);
        }
        let storage_after = self.call.vm.loads[index]
            .native
            .as_ref()
            .unwrap()
            .storage_bytes();
        self.call
            .vm
            .gc_note_control(storage_after.saturating_sub(storage_before));
        Ok(())
    }
}
