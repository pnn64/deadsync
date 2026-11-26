//! Native-reader roots follow incremental parser/decoder events.
use super::*;
use crate::parser::{NativeRootEvent, RootText};

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
        let storage_before = self.loads[index].native.as_ref().unwrap().storage_bytes();
        let job = &mut self.loads[index];
        let binary = job.binary_mode;
        let text_started = !binary && job.native.as_ref().is_some_and(|roots| roots.started);
        let recycle = text_started && job.parser.reusable();
        let mut events = if binary {
            job.decoder
                .as_mut()
                .map_or_else(Vec::new, |decoder| decoder.take_native_events())
        } else if text_started {
            job.parser.take_native_events()
        } else {
            Vec::new()
        };
        if !binary && self.registers.memory().uses_global_allocator() {
            // A constants table is visible only if its function is still
            // open at a reader boundary. No guest code runs inside this batch.
            let mut closed = 0;
            for event in events.iter_mut().rev() {
                match event {
                    NativeRootEvent::Close => closed += 1,
                    NativeRootEvent::Open if closed != 0 => {
                        *event = NativeRootEvent::OpenWithoutTable;
                        closed -= 1;
                    }
                    _ => {}
                }
            }
        }
        let result = (|| -> Result<(), Error> {
            for event in events.drain(..) {
                if matches!(
                    event,
                    NativeRootEvent::Open | NativeRootEvent::OpenWithoutTable
                ) {
                    let job = &self.loads[index];
                    let source = job
                        .native
                        .as_ref()
                        .unwrap()
                        .active
                        .last()
                        .map(|(_, p)| self.parser_prototypes[slot_index(p.0)].source);
                    let source = if let Some(source) = source {
                        source
                    } else {
                        let name = if binary {
                            b"=?".as_slice()
                        } else {
                            &self.loads[index].name
                        };
                        self.strings
                            .try_intern(name)
                            .map_err(|_| self.memory_failure())?
                    };
                    let id = ParserPrototypeId(self.parser_prototypes.try_insert(
                        loads::ParserPrototype {
                            source,
                            constant_count: 0,
                            constant_strings: Buffer::new_in(self.registers.memory().clone()),
                            children: Buffer::new_in(self.registers.memory().clone()),
                            debug_strings: Buffer::new_in(self.registers.memory().clone()),
                        },
                    )?);
                    let table = (!binary && matches!(event, NativeRootEvent::Open))
                        .then(|| self.try_new_table())
                        .transpose()?;
                    self.loads[index]
                        .native
                        .as_mut()
                        .unwrap()
                        .active
                        .try_push((table, id))?;
                    if binary {
                        self.loads[index]
                            .native
                            .as_mut()
                            .unwrap()
                            .prototypes
                            .try_push(id)?;
                    }
                    self.reserve_gc_value(Value::ParserPrototype(id))?;
                    self.gc_new_value(Value::ParserPrototype(id));
                    if !binary {
                        self.native_load_push(
                            base,
                            top,
                            table.map_or(Value::Nil, Value::Table),
                            limit_base,
                        )?;
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
                if let NativeRootEvent::Anchor(text) = event {
                    if let Some(table) = table {
                        let key = self.intern_parser_text(index, &text)?;
                        if self.tables[slot_index(table.0)].get_string(key) == Value::Nil {
                            // The string key is valid and the scalar-overwrite
                            // path would repeat the lookup before a miss.
                            self.raw_set_valid_key(
                                table,
                                Value::String(key),
                                Value::Boolean(true),
                            )?;
                        }
                    }
                    continue;
                }
                let before = self.parser_prototypes[slot_index(id.0)].heap_bytes();
                let result = (|| -> Result<(), Error> {
                    match event {
                        NativeRootEvent::Close => {
                            self.loads[index].native.as_mut().unwrap().active.pop();
                            if !binary {
                                self.loads[index]
                                    .native
                                    .as_mut()
                                    .unwrap()
                                    .prototypes
                                    .try_push(id)?;
                            }
                            let count = 1 + usize::from(!binary);
                            *top = top.checked_sub(count).ok_or_else(|| {
                                Error::runtime_static("native parser roots removed")
                            })?;
                            self.truncate_registers(base + *top);
                            if let Some((_, parent)) = self.loads[index]
                                .native
                                .as_ref()
                                .unwrap()
                                .active
                                .last()
                                .copied()
                            {
                                self.reserve_gc_barrier(
                                    Value::ParserPrototype(parent),
                                    Value::ParserPrototype(id),
                                )?;
                                let p = &mut self.parser_prototypes[slot_index(parent.0)];
                                let before = p.heap_bytes();
                                p.children.try_push(id)?;
                                let growth = p.heap_bytes() - before;
                                self.object_payload_bytes += growth;
                                self.parser_prototypes.note_alloc(growth);
                                self.gc_barrier(
                                    Value::ParserPrototype(parent),
                                    Value::ParserPrototype(id),
                                );
                            }
                        }
                        NativeRootEvent::Constant(_) | NativeRootEvent::ConstantText(_) => {
                            let value = match event {
                                NativeRootEvent::Constant(Constant::Nil) => Value::Nil,
                                NativeRootEvent::Constant(Constant::Boolean(value)) => {
                                    Value::Boolean(value)
                                }
                                NativeRootEvent::Constant(Constant::Number(value)) => {
                                    Value::Number(value)
                                }
                                NativeRootEvent::Constant(Constant::String(bytes)) => {
                                    self.try_intern(&bytes)?
                                }
                                NativeRootEvent::ConstantText(text) => {
                                    Value::String(self.intern_parser_text(index, &text)?)
                                }
                                _ => unreachable!("parser roots own their string constants"),
                            };
                            if let Value::String(string) = value {
                                self.reserve_gc_barrier(Value::ParserPrototype(id), value)?;
                                self.parser_prototypes[slot_index(id.0)]
                                    .constant_strings
                                    .try_push(string)?;
                                self.gc_barrier(Value::ParserPrototype(id), value);
                            }
                            if let Some(table) = table {
                                let p = &mut self.parser_prototypes[slot_index(id.0)];
                                let ordinal = p.constant_count;
                                p.constant_count += 1;
                                let key = if value == Value::Nil {
                                    Value::Table(table)
                                } else {
                                    value
                                };
                                self.raw_set(table, key, Value::Number(ordinal as f64))?;
                            }
                        }
                        NativeRootEvent::Source(bytes) => {
                            let string = self
                                .strings
                                .try_intern(&bytes)
                                .map_err(|_| self.memory_failure())?;
                            let value = Value::String(string);
                            self.reserve_gc_barrier(Value::ParserPrototype(id), value)?;
                            self.parser_prototypes[slot_index(id.0)].source = string;
                            self.gc_barrier(Value::ParserPrototype(id), value);
                        }
                        NativeRootEvent::DebugString(bytes) => {
                            let string = self
                                .strings
                                .try_intern(&bytes)
                                .map_err(|_| self.memory_failure())?;
                            let value = Value::String(string);
                            self.reserve_gc_barrier(Value::ParserPrototype(id), value)?;
                            self.parser_prototypes[slot_index(id.0)]
                                .debug_strings
                                .try_push(string)?;
                            self.gc_barrier(Value::ParserPrototype(id), value);
                        }
                        NativeRootEvent::Open
                        | NativeRootEvent::OpenWithoutTable
                        | NativeRootEvent::Anchor(_) => unreachable!(),
                    }
                    Ok(())
                })();
                let growth = self.parser_prototypes[slot_index(id.0)]
                    .heap_bytes()
                    .saturating_sub(before);
                self.object_payload_bytes += growth;
                self.parser_prototypes.note_alloc(growth);
                result?;
            }
            Ok(())
        })();
        if recycle {
            self.loads[index].parser.recycle_native_events(events);
        }
        let storage_after = self.loads[index].native.as_ref().unwrap().storage_bytes();
        self.gc_note_control(storage_after.saturating_sub(storage_before));
        result
    }
    fn intern_parser_text(&mut self, index: usize, text: &RootText) -> Result<StringId, Error> {
        let bytes = text.bytes(self.loads[index].source());
        self.strings
            .try_intern(bytes)
            .map_err(|_| self.memory_failure())
    }

    fn native_load_push(
        &mut self,
        base: usize,
        top: &mut usize,
        value: Value,
        limit_base: usize,
    ) -> Result<(), Error> {
        if (base + *top).saturating_sub(limit_base) >= limits::MAX_CSTACK {
            return Err(Error::runtime_static("native stack overflow"));
        }
        self.reserve_registers(base + *top + 1)?;
        self.reserve_gc_value(value)?;
        self.resize_registers((base + *top + 1).max(self.registers.len()));
        self.registers[base + *top] = value;
        self.gc_new_value(value);
        *top += 1;
        Ok(())
    }
}
