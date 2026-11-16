//! Reserve a complete trace item before consuming it or changing any marks.
use super::*;

impl Vm {
    pub(in crate::vm) fn reserve_gc_close_slot(&mut self) -> Result<(), Error> {
        let total = self
            .upvalues
            .live_len()
            .checked_add(1)
            .ok_or_else(|| self.memory_failure())?;
        let mut closed = std::mem::take(&mut self.gc.closed);
        let additional = total.saturating_sub(closed.len());
        let result = self.reserve_trace_work(&mut closed, additional);
        self.gc.closed = closed;
        result
    }
    pub(super) fn transfer_closed_work(&mut self, work: &mut Work) -> Result<(), Error> {
        self.reserve_trace_work(work, self.gc.closed.len())?;
        work.extend(self.gc.closed.iter().copied());
        self.gc.closed.clear();
        Ok(())
    }
    pub(super) fn reserve_trace_work(
        &mut self,
        work: &mut Work,
        additional: usize,
    ) -> Result<(), Error> {
        let before = work.capacity();
        let result = work.try_reserve(additional);
        self.gc_note_control((work.capacity() - before) * size_of::<Trace>());
        result.map_err(|_| self.memory_failure())
    }
    pub(super) fn reserve_gc_work(&mut self, additional: usize) -> Result<(), Error> {
        let mut work = std::mem::take(&mut self.gc.work);
        let result = self.reserve_trace_work(&mut work, additional);
        self.gc.work = work;
        result
    }
    pub(super) fn seed_gc_root_count(&self) -> usize {
        4usize
            .saturating_add(usize::from(self.io.file_tag.is_some()))
            .saturating_add(self.meta_names.len())
            .saturating_add(self.type_metatables.iter().flatten().count())
            .saturating_add(self.roots.iter().copied().filter_map(edge).count())
            .saturating_add(usize::from(self.standalone != 0))
            .saturating_add(
                self.programs
                    .iter()
                    .filter(|p| p.owner.load(std::sync::atomic::Ordering::Acquire) != 0)
                    .count(),
            )
    }
    pub(super) fn table_trace_count(&self, id: TableId, cursor: usize, restart: bool) -> usize {
        let table = &self.tables[slot_index(id.0)];
        let (cursor, weak_keys, weak_values) = match table.trace {
            TableTrace::Pending(wk, wv) if !restart => (cursor, wk, wv),
            _ => {
                let (wk, wv) = self.table_mode(table);
                (0, wk, wv)
            }
        };
        let (next, pairs) = table.trace_slice(cursor, 64);
        let mut count = usize::from(next.is_some())
            + usize::from(cursor == 0 && table.metatable().is_some())
            + usize::from(cursor == 0 && table.host_state().is_some());
        for (key, value) in pairs {
            count +=
                usize::from((!weak_keys || matches!(key, Value::String(_))) && edge(key).is_some());
            count += usize::from(
                (!weak_values || matches!(value, Value::String(_))) && edge(value).is_some(),
            );
        }
        count
    }
    pub(super) fn thread_trace_count(&self, id: ThreadId) -> (usize, usize) {
        let t = &self.threads[slot_index(id.0)];
        let (registers, frames, open, hooks, subs, sorts, loads) = if id == self.thread {
            (
                &self.registers,
                &self.frames,
                &self.open_upvalues,
                &self.hooks,
                &self.substitutions,
                &self.sorts,
                &self.loads,
            )
        } else {
            (
                &t.registers,
                &t.frames,
                &t.open_upvalues,
                &t.hooks,
                &t.substitutions,
                &t.sorts,
                &t.loads,
            )
        };
        // Register ranges can overlap, but register_roots queues each physical
        // slot once. Each frame contributes at most its own closure identity.
        let mut count = 1usize
            .saturating_add(registers.len())
            .saturating_add(frames.len())
            .saturating_add(open.len())
            .saturating_add(hooks.hidden.len())
            .saturating_add(usize::from(matches!(
                hooks.callback,
                hooks::Callback::Host(_)
            )))
            .saturating_add(usize::from(hooks.host_running.is_some()))
            .saturating_add(usize::from(t.resumer.is_some()));
        for state in &t.host_states {
            count = count.saturating_add(state.roots().len());
        }
        for job in subs {
            count = count.saturating_add(job.roots().count());
        }
        for job in sorts {
            count = count.saturating_add(job.roots().len());
        }
        for job in loads {
            count = count
                .saturating_add(usize::from(job.file.is_some()))
                .saturating_add(job.roots().count());
        }
        (count, registers.len().div_ceil(64))
    }
    pub(super) fn reserve_trace_item(&mut self, item: Trace, work: &mut Work) -> Result<(), Error> {
        let (item, forced) = match item {
            Trace::MarkedValue(value) => (Trace::Value(value), true),
            item => (item, false),
        };
        if let Trace::Value(value) = item
            && !forced
            && self.marked_value(value)
        {
            return Ok(());
        }
        let mut words = 0;
        let count = match item {
            Trace::Value(Value::Table(id)) | Trace::TableAgain(id) => {
                self.table_trace_count(id, 0, true)
            }
            Trace::TableSlice(id, cursor) => self.table_trace_count(id, cursor, false),
            Trace::Value(Value::Thread(id)) | Trace::ThreadAgain(id) => {
                let (count, needed) = self.thread_trace_count(id);
                words = needed;
                count
            }
            Trace::Value(Value::Function(id)) => {
                let f = &self.functions[slot_index(id.0)];
                f.upvalues
                    .len()
                    .saturating_add(2)
                    .saturating_add(usize::from(
                        self.programs[f.program]
                            .parser_prototypes
                            .get(f.prototype)
                            .is_some(),
                    ))
            }
            Trace::Value(Value::Userdata(id)) => {
                1 + usize::from(self.userdata[slot_index(id.0)].metatable.is_some())
            }
            Trace::Value(Value::ParserPrototype(id)) => self.parser_prototypes[slot_index(id.0)]
                .roots()
                .filter_map(edge)
                .count(),
            Trace::Value(Value::Native(id)) if id >= strings::ITERATOR_FIRST => self
                .native_closures[slot_index(id)]
            .captures
            .iter()
            .copied()
            .filter_map(edge)
            .count()
            .saturating_add(1),
            Trace::Value(Value::Native(id)) if id < 256 => 1,
            Trace::CellAgain(i) => usize::from(edge(self.cell_value(i)).is_some()),
            Trace::Cell(i)
                if self
                    .upvalues
                    .key(i)
                    .is_some_and(|key| !self.upvalues.marked(key)) =>
            {
                usize::from(edge(self.cell_value(i)).is_some())
            }
            Trace::Program(i)
                if self
                    .programs
                    .key(i)
                    .is_some_and(|key| !self.programs.marked(key)) =>
            {
                let program = &self.programs[i];
                let mut count = program.debug_strings.len();
                for constants in &program.constants {
                    count =
                        count.saturating_add(constants.iter().copied().filter_map(edge).count());
                }
                for names in &program.upvalue_names {
                    count = count.saturating_add(names.len());
                }
                count
            }
            _ => 0,
        };
        let before = self.gc.live.capacity();
        let result = self
            .gc
            .live
            .try_reserve(words.saturating_sub(self.gc.live.len()));
        self.gc_note_control((self.gc.live.capacity() - before) * size_of::<u64>());
        result.map_err(|_| self.memory_failure())?;
        // The current item still occupies its slot until both reservations
        // succeed. Popping it makes one slot available for its children.
        self.reserve_trace_work(work, count.saturating_sub(1))
    }
}
