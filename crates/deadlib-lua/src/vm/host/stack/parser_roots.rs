//! Native-reader roots follow incremental parser/decoder events.
use super::*;

impl NativeStack<'_, '_> {
    pub(super) fn apply_load_roots(&mut self, index: usize) -> Result<usize, Error> {
        let mut top = self.base + self.top();
        let result = self
            .call
            .vm
            .apply_native_load_roots(index, 0, &mut top, self.base);
        self.call.results = top.saturating_sub(self.base);
        self.sync();
        result.map(|()| top)
    }
}

impl Vm {
    pub(in crate::vm) fn apply_native_load_roots(
        &mut self,
        index: usize,
        base: usize,
        top: &mut usize,
        limit_base: usize,
    ) -> Result<(), Error> {
        use crate::parser::NativeRootEvent;
        let storage_before = self.loads[index].native.as_ref().unwrap().storage_bytes();
        let job = &mut self.loads[index];
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
                let job = &self.loads[index];
                let source = job
                    .native
                    .as_ref()
                    .unwrap()
                    .active
                    .last()
                    .map(|(_, p)| self.parser_prototypes[slot_index(p.0)].source);
                let source = source.unwrap_or_else(|| {
                    let name = if self.loads[index].binary_mode {
                        b"=?".to_vec()
                    } else {
                        self.loads[index].name.to_vec()
                    };
                    self.intern(&name)
                });
                let id = ParserPrototypeId(self.parser_prototypes.insert(loads::ParserPrototype {
                    source,
                    constants: Vec::new(),
                    children: Vec::new(),
                    debug_strings: Vec::new(),
                }));
                let table = (!self.loads[index].binary_mode).then(|| self.new_table());
                self.loads[index]
                    .native
                    .as_mut()
                    .unwrap()
                    .active
                    .push((table, id));
                if self.loads[index].binary_mode {
                    self.loads[index]
                        .native
                        .as_mut()
                        .unwrap()
                        .prototypes
                        .push(id);
                }
                self.gc_new_value(Value::ParserPrototype(id));
                if let Some(table) = table {
                    self.native_load_push(base, top, Value::Table(table), limit_base)?;
                }
                self.native_load_push(base, top, Value::ParserPrototype(id), limit_base)?;
                continue;
            }
            let (table, id) = *self.loads[index]
                .native
                .as_ref()
                .unwrap()
                .active
                .last()
                .expect("active parser prototype");
            let before = self.parser_prototypes[slot_index(id.0)].heap_bytes();
            match event {
                NativeRootEvent::Close => {
                    self.loads[index].native.as_mut().unwrap().active.pop();
                    if table.is_some() {
                        self.loads[index]
                            .native
                            .as_mut()
                            .unwrap()
                            .prototypes
                            .push(id);
                    }
                    let count = 1 + usize::from(table.is_some());
                    *top = top.checked_sub(count).ok_or_else(|| {
                        Error::new(ErrorKind::Runtime, 0, "native parser roots removed")
                    })?;
                    self.registers.truncate(base + *top);
                    if let Some((_, parent)) = self.loads[index]
                        .native
                        .as_ref()
                        .unwrap()
                        .active
                        .last()
                        .copied()
                    {
                        let p = &mut self.parser_prototypes[slot_index(parent.0)];
                        let before = p.heap_bytes();
                        p.children.push(Value::ParserPrototype(id));
                        let growth = p.heap_bytes() - before;
                        self.parser_prototypes.note_alloc(growth);
                        self.gc_barrier(Value::ParserPrototype(parent), Value::ParserPrototype(id));
                    }
                }
                NativeRootEvent::Anchor(bytes) => {
                    let key = self.intern(&bytes);
                    let table = table.expect("text parser constants table");
                    if self.tables[slot_index(table.0)].get(key) == Value::Nil {
                        self.raw_set(table, key, Value::Boolean(true))?;
                    }
                }
                NativeRootEvent::Constant(constant) => {
                    let value = match constant {
                        Constant::Nil => Value::Nil,
                        Constant::Boolean(value) => Value::Boolean(value),
                        Constant::Number(value) => Value::Number(value),
                        Constant::String(bytes) => self.intern(&bytes),
                    };
                    let p = &mut self.parser_prototypes[slot_index(id.0)];
                    let ordinal = p.constants.len();
                    p.constants.push(value);
                    self.gc_barrier(Value::ParserPrototype(id), value);
                    if let Some(table) = table {
                        let key = if value == Value::Nil {
                            Value::Table(table)
                        } else {
                            value
                        };
                        self.raw_set(table, key, Value::Number(ordinal as f64))?;
                    }
                }
                NativeRootEvent::Source(bytes) => {
                    let value = self.intern(&bytes);
                    self.parser_prototypes[slot_index(id.0)].source = value;
                    self.gc_barrier(Value::ParserPrototype(id), value);
                }
                NativeRootEvent::DebugString(bytes) => {
                    let value = self.intern(&bytes);
                    self.parser_prototypes[slot_index(id.0)]
                        .debug_strings
                        .push(value);
                    self.gc_barrier(Value::ParserPrototype(id), value);
                }
                NativeRootEvent::Open => unreachable!(),
            }
            let growth = self.parser_prototypes[slot_index(id.0)]
                .heap_bytes()
                .saturating_sub(before);
            self.parser_prototypes.note_alloc(growth);
        }
        let storage_after = self.loads[index].native.as_ref().unwrap().storage_bytes();
        self.gc_note_control(storage_after.saturating_sub(storage_before));
        Ok(())
    }
    fn native_load_push(
        &mut self,
        base: usize,
        top: &mut usize,
        value: Value,
        limit_base: usize,
    ) -> Result<(), Error> {
        if (base + *top).saturating_sub(limit_base) >= limits::MAX_CSTACK {
            return Err(Error::new(ErrorKind::Runtime, 0, "native stack overflow"));
        }
        self.resize_registers((base + *top + 1).max(self.registers.len()));
        self.registers[base + *top] = value;
        self.gc_new_value(value);
        *top += 1;
        Ok(())
    }
}
