//! Reserve a complete trace item before consuming it or changing any marks.
use super::*;

const TABLE_SLICE_WORK: usize = 2 * TABLE_TRACE_SLOTS + 2;

impl Vm {
    pub(in crate::vm) fn reserve_gc_close_slot(&mut self) -> Result<(), Error> {
        let total = self
            .upvalues
            .live_len()
            .checked_add(1)
            .ok_or_else(|| self.memory_failure())?;
        let additional = total.saturating_sub(self.gc.closed.len());
        try_reserve_work(
            &mut self.gc.closed,
            additional,
            &mut self.gc.control_allocated,
        )
        .map_err(|_| self.memory_failure())
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
        try_reserve_work(work, additional, &mut self.gc.control_allocated)
            .map_err(|_| self.memory_failure())
    }
    pub(super) fn reserve_gc_work(&mut self, additional: usize) -> Result<(), Error> {
        try_reserve_work(
            &mut self.gc.work,
            additional,
            &mut self.gc.control_allocated,
        )
        .map_err(|_| self.memory_failure())
    }
    fn fixed_gc_root_count(&self) -> usize {
        4 + usize::from(self.io.file_tag.is_some())
            + self.meta_names.len()
            + self.type_metatables.iter().flatten().count()
            + usize::from(self.standalone != 0)
    }
    pub(super) fn seed_gc_root_count(&self, available: usize) -> usize {
        let fixed = self.fixed_gc_root_count();
        let upper = fixed
            .saturating_add(self.roots.live_len())
            .saturating_add(self.programs.live_len());
        if upper <= available {
            return upper;
        }
        fixed
            .saturating_add(self.roots.iter().copied().filter_map(edge).count())
            .saturating_add(
                self.programs
                    .iter()
                    .filter(|p| p.data.has_host_owner())
                    .count(),
            )
    }
    fn table_trace_count(
        &mut self,
        slot: u32,
        cursor: usize,
        restart: bool,
        direct_leaves: bool,
        work: &mut Work,
    ) -> usize {
        let table = &self.tables[slot as usize];
        let (cursor, weak_keys, weak_values) = match table.trace {
            TableTrace::Pending(wk, wv) if !restart => (cursor, wk, wv),
            _ => {
                let (wk, wv) = self.table_mode(table);
                (0, wk, wv)
            }
        };
        let (next, array, pairs) = table.trace_slice(cursor, TABLE_TRACE_SLOTS);
        // Full marking can leave a smaller queue after marking leaves in
        // place. Make a wide scan reserve once instead of counting every
        // subsequent slice. An unsuccessful reservation falls back to the
        // actual edge count, which can fit in the existing queue.
        if direct_leaves
            && (cursor != 0 || next.is_some())
            && try_reserve_work(work, TABLE_SLICE_WORK, &mut self.gc.control_allocated).is_ok()
        {
            return 0;
        }
        let array_edges = if weak_values {
            array
                .iter()
                .filter(|v| matches!(v, Value::String(_)))
                .count()
        } else {
            array.iter().copied().filter_map(edge).count()
        };
        let mut count = usize::from(next.is_some())
            + usize::from(cursor == 0 && table.metatable().is_some())
            + usize::from(cursor == 0 && table.host_state().is_some())
            + array_edges;
        for (_, key, value) in pairs {
            count += usize::from(table_edge(key, weak_keys).is_some());
            count += usize::from(table_edge(value, weak_values).is_some());
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
    pub(super) fn reserve_trace_item(
        &mut self,
        item: Trace,
        work: &mut Work,
        direct_leaves: bool,
    ) -> Result<(), Error> {
        let mut words = 0;
        let count = match item {
            // A slice queues at most two edges per slot, a continuation, a
            // metatable, and host state. The current item frees one slot.
            Trace::Table(_, _) | Trace::TableAgain(_) | Trace::TableSlice(_, _)
                if work.capacity() - work.len() >= TABLE_SLICE_WORK =>
            {
                return Ok(());
            }
            Trace::Table(id, _) | Trace::TableAgain(id) => {
                self.table_trace_count(id.slot(), 0, true, direct_leaves, work)
            }
            Trace::TableSlice(id, cursor) => {
                self.table_trace_count(id, cursor, false, direct_leaves, work)
            }
            Trace::Thread(id, _) | Trace::ThreadAgain(id) => {
                let (count, needed) = self.thread_trace_count(id);
                words = needed;
                count
            }
            Trace::Function(id, _) => {
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
            Trace::Userdata(id, _) => {
                1 + usize::from(self.userdata[slot_index(id.0)].metatable.is_some())
            }
            Trace::ParserPrototype(id, _) if direct_leaves => {
                self.parser_prototypes[slot_index(id.0)].children.len()
            }
            Trace::ParserPrototype(id, _) => {
                let prototype = &self.parser_prototypes[slot_index(id.0)];
                // The current work item supplies space for the source edge.
                let additional = prototype.constant_strings.len()
                    + prototype.children.len()
                    + prototype.debug_strings.len();
                if additional <= work.capacity() - work.len() {
                    return Ok(());
                }
                prototype.roots().filter_map(edge).count()
            }
            Trace::Native(id, _) if id >= strings::ITERATOR_FIRST => {
                let captures = &self.native_closures[slot_index(id)].captures;
                // The current work item supplies space for the environment edge.
                if captures.len() <= work.capacity() - work.len() {
                    return Ok(());
                }
                captures
                    .iter()
                    .copied()
                    .filter_map(edge)
                    .count()
                    .saturating_add(1)
            }
            Trace::Program(i) if self.programs.contains_slot(i) => {
                let program = &self.programs[i];
                let names = program
                    .upvalue_names
                    .iter()
                    .fold(program.debug_strings.len(), |count, names| {
                        count.saturating_add(names.len())
                    });
                let upper = names.saturating_add(program.constants.len());
                if upper.saturating_sub(1) <= work.capacity() - work.len() {
                    return Ok(());
                }
                names.saturating_add(program.constants.iter().copied().filter_map(edge).count())
            }
            // Remaining items queue at most one edge, reusing their own slot.
            _ => return Ok(()),
        };
        let before = self.gc.live.capacity();
        if words > before {
            let result = self.gc.live.try_reserve(words - self.gc.live.len());
            self.gc_note_control((self.gc.live.capacity() - before) * size_of::<u64>());
            result.map_err(|_| self.memory_failure())?;
        }
        // The current item still occupies its slot until both reservations
        // succeed. Popping it makes one slot available for its children.
        self.reserve_trace_work(work, count.saturating_sub(1))
    }
}

fn try_reserve_work(work: &mut Work, additional: usize, debt: &mut usize) -> Result<(), Error> {
    let before = work.capacity();
    if additional <= before - work.len() {
        return Ok(());
    }
    let result = work.try_reserve(additional);
    *debt = debt.saturating_add((work.capacity() - before) * size_of::<Trace>());
    result
}
