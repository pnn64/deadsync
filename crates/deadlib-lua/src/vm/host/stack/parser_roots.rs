//! Native-reader roots follow incremental parser/decoder events.
use super::*;
use crate::memory::fallible::{SliceExt, VecExt};
use crate::parser::{NativeRootEvent, RootText};
use crate::vm::loads::reader_intervals::{Plan, RootSegmentSnapshot};
use std::sync::atomic::{AtomicU64, Ordering};

fn collect_parser_anchors(ids: impl Iterator<Item = StringId>) -> Option<Box<[StringId]>> {
    let mut anchors = Vec::new();
    for id in ids {
        if id.0 == 0 {
            return None;
        }
        anchors.try_push(id).ok()?;
    }
    anchors.sort_unstable_by_key(|id| id.0);
    anchors.dedup();
    anchors.try_into_boxed_slice().ok()
}

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
        let job = &mut self.loads[index];
        let storage_before = job.root_storage_bytes();
        let binary = job.binary_mode;
        let text_started = !binary && job.native.as_ref().is_some_and(|roots| roots.started);
        let replayed = job.replayed_interval();
        let recycle = replayed.is_none() && text_started && job.parser.reusable();
        let mut events = if binary {
            job.decoder
                .as_mut()
                .map_or_else(Vec::new, |decoder| decoder.take_native_events())
        } else if replayed.is_none() && text_started {
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
        let result = if let Some((plan, interval, value)) = &replayed {
            let number = NativeRootEvent::Constant(Constant::Number(*value));
            self.apply_replayed_roots(index, base, top, limit_base, plan, *interval, &number)
        } else {
            self.apply_root_events(
                index,
                base,
                top,
                limit_base,
                binary,
                false,
                events.iter().map(|event| (event, None)),
            )
        };
        if result.is_ok() && replayed.is_none() {
            self.loads[index].record_interval(&events);
        }
        events.clear();
        if recycle {
            self.loads[index].parser.recycle_native_events(events);
        }
        let storage_after = self.loads[index].root_storage_bytes();
        self.gc_note_control(storage_after.saturating_sub(storage_before));
        result
    }

    fn apply_replayed_roots(
        &mut self,
        index: usize,
        base: usize,
        top: &mut usize,
        limit_base: usize,
        plan: &Plan,
        interval: usize,
        number: &NativeRootEvent,
    ) -> Result<(), Error> {
        let mut next = 0;
        for segment in plan.segments(interval) {
            if next < segment.range.start {
                self.apply_root_events(
                    index,
                    base,
                    top,
                    limit_base,
                    false,
                    true,
                    plan.events(interval, next..segment.range.start, number),
                )?;
            }
            next = segment.range.end;
            let (table, id) = *self.loads[index]
                .native
                .as_ref()
                .unwrap()
                .active
                .last()
                .expect("active parser prototype");
            let events = plan.events(interval, segment.range.clone(), number);
            let Some(table) = table else {
                self.apply_root_events(index, base, top, limit_base, false, true, events)?;
                continue;
            };
            // Both the recording and this replay must have reached this
            // segment through the parser's own writes to the table.
            let pristine = self.tables[slot_index(table.0)].parser_pristine();
            if !segment.exact || !pristine {
                self.tables[slot_index(table.0)].clear_parser_pristine();
                self.apply_root_events(index, base, top, limit_base, false, true, events)?;
                continue;
            }

            let mut snapshot = (segment.range.len() >= 3)
                .then(|| segment.snapshot.lock().ok())
                .flatten();
            if let Some(saved) = snapshot.as_deref_mut().and_then(Option::as_mut)
                && self.apply_parser_snapshot(table, id, saved)
            {
                continue;
            }

            let proto = &self.parser_prototypes[slot_index(id.0)];
            let count_before = proto.constant_count;
            let strings_before = proto.constant_strings.len();
            self.apply_root_events(index, base, top, limit_base, false, true, events)?;
            let changed = !self.tables[slot_index(table.0)].parser_pristine();
            self.tables[slot_index(table.0)].mark_parser_pristine();
            if let Some(snapshot) = snapshot.as_deref_mut() {
                let anchors = plan
                    .events(interval, segment.range.clone(), number)
                    .filter_map(|(event, hint)| {
                        if let (NativeRootEvent::Anchor(_), Some(hint)) = (event, hint) {
                            Some(StringId(hint.load(Ordering::Relaxed)))
                        } else {
                            None
                        }
                    });
                self.capture_parser_snapshot(
                    plan,
                    snapshot,
                    table,
                    id,
                    changed,
                    count_before,
                    strings_before,
                    anchors,
                );
            }
        }
        self.apply_root_events(
            index,
            base,
            top,
            limit_base,
            false,
            true,
            plan.events(interval, next..plan.event_count(interval), number),
        )
    }

    fn capture_parser_snapshot(
        &mut self,
        plan: &Plan,
        snapshot: &mut Option<RootSegmentSnapshot>,
        table: TableId,
        id: ParserPrototypeId,
        changed: bool,
        count_before: usize,
        strings_before: usize,
        anchors: impl Iterator<Item = StringId>,
    ) {
        if let Some(old) = snapshot.take() {
            plan.forget_layout(old.heap_bytes());
        }
        let proto = &self.parser_prototypes[slot_index(id.0)];
        let strings = &proto.constant_strings[strings_before..];
        let Some(anchors) = collect_parser_anchors(anchors) else {
            return;
        };
        let Some(budget) = plan
            .layout_budget()
            .checked_sub(size_of_val(strings) + size_of_val(&*anchors))
        else {
            return;
        };
        let layout = if changed {
            let Some(layout) = self.tables[slot_index(table.0)].capture_parser_layout(budget)
            else {
                return;
            };
            Some(layout)
        } else {
            None
        };
        let Ok(strings) = strings.try_boxed() else {
            return;
        };
        let saved = RootSegmentSnapshot {
            layout,
            strings,
            anchors,
            count_before,
            count_after: proto.constant_count,
            reclaimed_epoch: self.strings.reclaimed_epoch,
        };
        let bytes = saved.heap_bytes();
        plan.note_layout(bytes);
        self.gc_note_control(bytes);
        *snapshot = Some(saved);
    }

    fn apply_parser_snapshot(
        &mut self,
        table: TableId,
        id: ParserPrototypeId,
        snapshot: &mut RootSegmentSnapshot,
    ) -> bool {
        let proto = &mut self.parser_prototypes[slot_index(id.0)];
        if proto.constant_count != snapshot.count_before {
            return false;
        }
        // Saved IDs are weak. Only a string sweep can retire them.
        let epoch = self.strings.reclaimed_epoch;
        let segment_strings = snapshot
            .strings
            .iter()
            .chain(snapshot.anchors.iter())
            .copied();
        if snapshot.reclaimed_epoch != epoch || epoch == u64::MAX {
            if !segment_strings
                .clone()
                .chain(
                    snapshot
                        .layout
                        .iter()
                        .flat_map(|layout| layout.string_ids()),
                )
                .all(|id| self.strings.bytes.contains(id.0))
            {
                return false;
            }
            snapshot.reclaimed_epoch = epoch;
        }
        let before = proto.constant_strings.capacity();
        if proto
            .constant_strings
            .try_reserve(snapshot.strings.len())
            .is_err()
        {
            return false;
        }
        let growth = (proto.constant_strings.capacity() - before) * size_of::<StringId>();
        self.object_payload_bytes += growth;
        self.parser_prototypes.note_alloc(growth);
        if let Some(layout) = &snapshot.layout {
            let table = &mut self.tables[slot_index(table.0)];
            let before = table.heap_bytes();
            let result = table.install_parser_layout(layout);
            let after = table.heap_bytes();
            self.note_table_resize(before, after);
            if result.is_err() {
                return false;
            }
        }
        let proto = &mut self.parser_prototypes[slot_index(id.0)];
        proto.constant_strings.extend_from_slice(&snapshot.strings);
        proto.constant_count = snapshot.count_after;
        // Interning preserves these leaf objects during propagation and sweep.
        // The saved layout and prototype roots introduce no other GC edges.
        if self.strings.bytes.preserves_new_references() {
            for id in segment_strings {
                self.strings.bytes.preserve_new_reference(id.0);
            }
        }
        true
    }

    fn apply_root_events<'a>(
        &mut self,
        index: usize,
        base: usize,
        top: &mut usize,
        limit_base: usize,
        binary: bool,
        replayed: bool,
        interval: impl Iterator<Item = (&'a NativeRootEvent, Option<&'a AtomicU64>)>,
    ) -> Result<(), Error> {
        for (event, string_hint) in interval {
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
                if replayed && let Some(table) = table {
                    self.tables[slot_index(table.0)].mark_parser_pristine();
                }
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
                    let key = self.intern_parser_text(index, text, string_hint)?;
                    if self.tables[slot_index(table.0)].get_string(key) == Value::Nil {
                        // The string key is valid and the scalar-overwrite
                        // path would repeat the lookup before a miss.
                        self.raw_set_valid_key(table, Value::String(key), Value::Boolean(true))?;
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
                        *top = top
                            .checked_sub(count)
                            .ok_or_else(|| Error::runtime_static("native parser roots removed"))?;
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
                                Value::Boolean(*value)
                            }
                            NativeRootEvent::Constant(Constant::Number(value)) => {
                                Value::Number(*value)
                            }
                            NativeRootEvent::Constant(Constant::String(bytes)) => {
                                self.try_intern(bytes)?
                            }
                            NativeRootEvent::ConstantText(text) => {
                                Value::String(self.intern_parser_text(index, text, string_hint)?)
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
                            .try_intern(bytes)
                            .map_err(|_| self.memory_failure())?;
                        let value = Value::String(string);
                        self.reserve_gc_barrier(Value::ParserPrototype(id), value)?;
                        self.parser_prototypes[slot_index(id.0)].source = string;
                        self.gc_barrier(Value::ParserPrototype(id), value);
                    }
                    NativeRootEvent::DebugString(bytes) => {
                        let string = self
                            .strings
                            .try_intern(bytes)
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
    }

    fn intern_parser_text(
        &mut self,
        index: usize,
        text: &RootText,
        hint: Option<&AtomicU64>,
    ) -> Result<StringId, Error> {
        if let Some(hint) = hint {
            let id = StringId(hint.load(Ordering::Relaxed));
            if self.strings.bytes.contains(id.0) {
                self.strings.bytes.preserve_new_reference(id.0);
                return Ok(id);
            }
        }
        let bytes = text.bytes(self.loads[index].source());
        let id = self
            .strings
            .try_intern(bytes)
            .map_err(|_| self.memory_failure())?;
        if let Some(hint) = hint {
            hint.store(id.0, Ordering::Relaxed);
        }
        Ok(id)
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
