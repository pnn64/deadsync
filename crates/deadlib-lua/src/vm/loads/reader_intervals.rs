//! Preserve the parser events and depth at each boundary of a successful load.
//! Every reader still supplies the next block. A mismatching block resumes the
//! original grammar after reconstructing its state from the accepted prefix.
use super::*;
use crate::memory::{Shared, text::Text};
use crate::parser::{NativeRootEvent, RootText};
use crate::table::ParserTableLayout;
use std::{
    ops::Range,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

const MIN_PREFIX: usize = 64;
const MAX_ENTRIES: usize = 24;
const MAX_BATCHES: usize = 256;
const MAX_EVENT_BYTES: usize = 128 * 1024;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;
const MAX_LAYOUT_BYTES: usize = 128 * 1024;

#[derive(Debug)]
struct Interval {
    end: usize,
    nesting: usize,
    closed: bool,
    events: Box<[RecordedEvent]>,
    segments: Box<[RootSegment]>,
}

#[derive(Debug)]
struct RecordedEvent {
    event: NativeRootEvent,
    string_hint: AtomicU64,
}

#[derive(Debug)]
pub(in crate::vm) struct RootSegment {
    pub range: Range<usize>,
    pub exact: bool,
    pub snapshot: Mutex<Option<RootSegmentSnapshot>>,
}

#[derive(Debug)]
pub(in crate::vm) struct RootSegmentSnapshot {
    pub layout: Option<ParserTableLayout>,
    pub strings: Box<[StringId]>,
    pub anchors: Box<[StringId]>,
    pub count_before: usize,
    pub count_after: usize,
    pub reclaimed_epoch: u64,
}

impl RootSegmentSnapshot {
    pub fn heap_bytes(&self) -> usize {
        size_of_val(&*self.strings)
            + size_of_val(&*self.anchors)
            + self
                .layout
                .as_ref()
                .map_or(0, ParserTableLayout::heap_bytes)
    }
}

#[derive(Debug)]
pub(in crate::vm) struct Plan {
    source: Box<[u8]>,
    digits: std::ops::Range<usize>,
    first_end: usize,
    depth: usize,
    locale: u64,
    intervals: Vec<Interval>,
    program: Shared<ProgramData>,
    target: (usize, usize),
    preamble_event: (usize, usize),
    bytes: usize,
    layout_bytes: AtomicUsize,
}

impl Plan {
    pub fn segments(&self, interval: usize) -> &[RootSegment] {
        &self.intervals[interval].segments
    }

    pub fn event_count(&self, interval: usize) -> usize {
        self.intervals[interval].events.len()
    }

    pub fn layout_budget(&self) -> usize {
        MAX_LAYOUT_BYTES - self.layout_bytes.load(Ordering::Relaxed)
    }

    pub fn note_layout(&self, bytes: usize) {
        self.layout_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn forget_layout(&self, bytes: usize) {
        self.layout_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub(in crate::vm) fn events<'a>(
        &'a self,
        interval: usize,
        range: Range<usize>,
        number: &'a NativeRootEvent,
    ) -> impl Iterator<Item = (&'a NativeRootEvent, Option<&'a AtomicU64>)> {
        let first = range.start;
        self.intervals[interval].events[range]
            .iter()
            .enumerate()
            .map(move |(index, saved)| {
                if self.preamble_event == (interval, first + index) {
                    return (number, None);
                }
                // The replay checks every byte except the preamble number.
                let hint = exact_text(&saved.event, &self.digits).then_some(&saved.string_hint);
                (&saved.event, hint)
            })
    }
}

fn exact_text(event: &NativeRootEvent, digits: &Range<usize>) -> bool {
    match event {
        NativeRootEvent::Anchor(RootText::Source { start, end })
        | NativeRootEvent::ConstantText(RootText::Source { start, end }) => {
            *start as usize >= digits.end || *end as usize <= digits.start
        }
        _ => true,
    }
}

fn table_event(event: &NativeRootEvent) -> bool {
    matches!(
        event,
        NativeRootEvent::Anchor(_)
            | NativeRootEvent::Constant(_)
            | NativeRootEvent::ConstantText(_)
    )
}

fn record_segments(
    events: &[NativeRootEvent],
    digits: &Range<usize>,
) -> Option<Box<[RootSegment]>> {
    let mut segments = Vec::new();
    let mut next = 0;
    while next < events.len() {
        let start = next;
        if !table_event(&events[next]) {
            next += 1;
            continue;
        }
        let mut exact = true;
        while next < events.len() && table_event(&events[next]) {
            exact &= exact_text(&events[next], digits);
            next += 1;
        }
        segments
            .try_push(RootSegment {
                range: start..next,
                exact,
                snapshot: Mutex::new(None),
            })
            .ok()?;
    }
    segments.try_into_boxed_slice().ok()
}

#[derive(Debug)]
pub(super) struct Recording {
    preamble: source_templates::Preamble,
    first_end: usize,
    depth: usize,
    locale: u64,
    intervals: Vec<Interval>,
    bytes: usize,
}

#[derive(Debug)]
pub(super) struct Playback {
    plan: Shared<Plan>,
    value: f64,
    next: usize,
}

impl Playback {
    pub(super) fn step(&mut self, source: &[u8], closed: bool, depth: usize) -> bool {
        let Some(interval) = self.plan.intervals.get(self.next) else {
            return false;
        };
        if depth != self.plan.depth
            || crate::locale::revision() != self.plan.locale
            || closed != interval.closed
            || source.len() != interval.end
        {
            return false;
        }
        let start = self.plan.intervals[self.next - 1].end;
        if source[start..] != self.plan.source[start..interval.end] {
            return false;
        }
        self.next += 1;
        true
    }

    pub(super) fn publish(&self) -> Result<Program, Error> {
        let (prototype, constant) = self.plan.target;
        Program::with_number(&self.plan.program, prototype, constant, self.value)
    }

    pub(super) fn reconstruct(
        &self,
        parser: &mut crate::parser::Parser,
        source: &[u8],
        depth: usize,
    ) -> Result<(), Error> {
        // All accepted intervals were parsed at the saved depth. Restore any
        // change made by coroutine resumption after rebuilding that prefix.
        let shift = (self.plan.depth as u16).wrapping_sub(depth as u16);
        parser.shift_native_depth(shift);
        let result = (|| {
            for interval in &self.plan.intervals[..self.next] {
                let ast = parser.resume(&source[..interval.end], false)?;
                debug_assert!(ast.is_none());
                let mut events = parser.take_native_events();
                events.clear();
                parser.recycle_native_events(events);
            }
            Ok(())
        })();
        parser.shift_native_depth(shift.wrapping_neg());
        result
    }
}

#[derive(Debug, Default)]
pub(super) enum Trace {
    #[default]
    None,
    Waiting,
    Record(Recording),
    Replay(Playback),
}

impl Trace {
    pub(super) fn nesting(&self) -> Option<usize> {
        match self {
            Self::Replay(playback) => Some(playback.plan.intervals[playback.next - 1].nesting),
            _ => None,
        }
    }

    pub(super) fn replayed_interval(&self) -> Option<(Shared<Plan>, usize, f64)> {
        let Self::Replay(playback) = self else {
            return None;
        };
        Some((playback.plan.clone(), playback.next - 1, playback.value))
    }

    pub(super) fn record(
        &mut self,
        source: &[u8],
        depth: usize,
        nesting: usize,
        closed: bool,
        events: &[NativeRootEvent],
    ) {
        let Self::Record(recording) = self else {
            return;
        };
        if recording
            .try_record(source, depth, nesting, closed, events)
            .is_none()
        {
            *self = Self::None;
        }
    }

    pub(super) fn storage_bytes(&self) -> usize {
        match self {
            Self::Record(recording) => {
                recording.bytes + recording.intervals.capacity() * size_of::<Interval>()
            }
            _ => 0,
        }
    }
}

impl Recording {
    fn try_record(
        &mut self,
        source: &[u8],
        depth: usize,
        nesting: usize,
        closed: bool,
        events: &[NativeRootEvent],
    ) -> Option<()> {
        let start = self.intervals.last().map_or(0, |interval| interval.end);
        let bytes = self
            .bytes
            .checked_add(events.len().checked_mul(size_of::<RecordedEvent>())?)?;
        let bytes = events.iter().try_fold(bytes, |cost, event| {
            let owned = match event {
                NativeRootEvent::Anchor(RootText::Owned(text))
                | NativeRootEvent::ConstantText(RootText::Owned(text)) => text.heap_bytes(),
                _ => 0,
            };
            cost.checked_add(owned)
        })?;
        if self.depth != depth
            || self.locale != crate::locale::revision()
            || source.len() > SPARE_SOURCE_LIMIT
            || !source[start..].is_ascii()
            || self.intervals.len() == MAX_BATCHES
            || bytes > MAX_EVENT_BYTES
        {
            return None;
        }
        let mut saved = Vec::new();
        saved.try_reserve_exact(events.len()).ok()?;
        for event in events {
            saved.push(RecordedEvent {
                event: copy_event(event)?,
                string_hint: AtomicU64::new(0),
            });
        }
        let segments = record_segments(events, &self.preamble.digits)
            .filter(|segments| size_of_val(&**segments) <= MAX_EVENT_BYTES - bytes)
            .unwrap_or_default();
        let bytes = bytes + size_of_val(&*segments);
        self.intervals
            .try_push(Interval {
                end: source.len(),
                nesting,
                closed,
                events: saved.try_into_boxed_slice().ok()?,
                segments,
            })
            .ok()?;
        self.bytes = bytes;
        Some(())
    }
}

/// Locate the first function's sole numeric root even when its closing
/// event waits for the next token in a later reader packet.
fn first_function_number(intervals: &[Interval], number: f64) -> Option<(usize, usize)> {
    use NativeRootEvent::*;
    let mut events = intervals.iter().enumerate().flat_map(|(packet, interval)| {
        interval
            .events
            .iter()
            .enumerate()
            .map(move |(index, saved)| ((packet, index), &saved.event))
    });
    if !matches!(events.next()?.1, Open) {
        return None;
    }
    let mut child = false;
    let mut constant = None;
    for (position, event) in events {
        match event {
            Open | OpenWithoutTable if !child => child = true,
            Constant(crate::bytecode::Constant::Number(value))
                if child && constant.is_none() && value.to_bits() == number.to_bits() =>
            {
                constant = Some(position);
            }
            Close => return constant,
            Open | OpenWithoutTable | Constant(_) | ConstantText(_) if child => return None,
            _ => {}
        }
    }
    None
}

fn copy_text(text: &RootText) -> Option<RootText> {
    Some(match text {
        RootText::Source { start, end } => RootText::Source {
            start: *start,
            end: *end,
        },
        RootText::Static(bytes) => RootText::Static(bytes),
        RootText::Owned(bytes) => RootText::Owned(Text::new(bytes).ok()?),
    })
}

fn copy_event(event: &NativeRootEvent) -> Option<NativeRootEvent> {
    use NativeRootEvent::*;
    Some(match event {
        Open => Open,
        OpenWithoutTable => OpenWithoutTable,
        Close => Close,
        Anchor(text) => Anchor(copy_text(text)?),
        ConstantText(text) => ConstantText(copy_text(text)?),
        Constant(crate::bytecode::Constant::Nil) => Constant(crate::bytecode::Constant::Nil),
        Constant(crate::bytecode::Constant::Boolean(value)) => {
            Constant(crate::bytecode::Constant::Boolean(*value))
        }
        Constant(crate::bytecode::Constant::Number(value)) => {
            Constant(crate::bytecode::Constant::Number(*value))
        }
        _ => return None,
    })
}

#[derive(Default)]
pub(super) struct Cache {
    plans: Vec<Shared<Plan>>,
    bytes: usize,
}

impl Cache {
    pub(super) fn begin(&mut self, source: &[u8], name: &[u8], depth: usize) -> Trace {
        if !(MIN_PREFIX..=SPARE_SOURCE_LIMIT).contains(&source.len()) {
            return Trace::None;
        }
        let Some(preamble) = source_templates::Preamble::reader_function(source) else {
            return Trace::None;
        };
        let locale = crate::locale::revision();
        let cached = self.plans.iter().rposition(|plan| {
            plan.first_end == source.len()
                && plan.depth == depth
                && plan.locale == locale
                && plan.program.name.as_ref() == name
                && plan.digits == preamble.digits
                && plan.source[..plan.digits.start] == source[..preamble.digits.start]
                && plan.source[plan.digits.end..plan.first_end] == source[preamble.digits.end..]
        });
        if let Some(index) = cached {
            let plan = self.plans.remove(index);
            let playback = Playback {
                plan: plan.clone(),
                value: preamble.value,
                next: 1,
            };
            self.plans.push(plan);
            Trace::Replay(playback)
        } else {
            Trace::Record(Recording {
                preamble,
                first_end: source.len(),
                depth,
                locale,
                intervals: Vec::new(),
                bytes: 0,
            })
        }
    }

    pub(super) fn remember(&mut self, source: &[u8], trace: Trace, program: &Program) {
        let Trace::Record(mut recording) = trace else {
            return;
        };
        if !recording
            .intervals
            .last()
            .is_some_and(|interval| interval.closed)
            || recording.locale != crate::locale::revision()
        {
            return;
        }
        let Some(preamble_event) =
            first_function_number(&recording.intervals, recording.preamble.value)
        else {
            return;
        };
        let Some(target) = recording.preamble.function_target(&program.data) else {
            return;
        };
        // A visible preamble table may have a different numeric key or array
        // extent in each load. Its remaining segments use ordinary replay.
        if let Some(segment) = recording.intervals[preamble_event.0]
            .segments
            .iter_mut()
            .find(|segment| segment.range.contains(&preamble_event.1))
        {
            segment.exact = false;
        }

        let bytes = recording.bytes
            + recording.intervals.capacity() * size_of::<Interval>()
            + source.len()
            + program.data.heap_bytes()
            + ProgramData::heap_bytes(&program.data);
        if bytes > MAX_CACHE_BYTES {
            return;
        }
        let Ok(source) = source.try_boxed() else {
            return;
        };
        let Ok(plan) = Shared::try_new_in(
            Plan {
                source,
                digits: recording.preamble.digits,
                first_end: recording.first_end,
                depth: recording.depth,
                locale: recording.locale,
                intervals: recording.intervals,
                program: program.data.clone(),
                target,
                preamble_event,
                bytes,
                layout_bytes: AtomicUsize::new(0),
            },
            Memory::default(),
        ) else {
            return;
        };
        while self.plans.len() == MAX_ENTRIES || self.bytes + bytes > MAX_CACHE_BYTES {
            self.bytes -= self.plans.remove(0).bytes;
        }
        if self.plans.try_reserve(1).is_ok() {
            self.plans.push(plan);
            self.bytes += bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_intervals_share_verified_code_and_rebuild_on_a_late_change() {
        let program = crate::compile(
            br#"
            return function(sequence, changed)
                local blocks = {
                    "local function sequence() return " .. sequence .. " end\n" ..
                    "-- pad the first reader block to leave room for a prefix match\n" ..
                    "local stable = 6\n",
                    "local value = 37\nlocal tag = 41\n",
                    "local function result() return value end\n",
                    changed and "return sequence() - result()\n"
                             or "return sequence() + result()\n",
                }
                local offset = 0
                return assert(load(function()
                    offset = offset + 1
                    if offset == 3 then
                        local _, constants = debug.getlocal(2, 4)
                        assert(constants.value == true)
                        constants.observer = sequence
                    elseif offset >= 4 then
                        local _, constants = debug.getlocal(2, 4)
                        assert(constants.observer == sequence)
                    end
                    return blocks[offset]
                end, "@reader-intervals.lua"))
            end
            "#,
        )
        .unwrap();
        let mut vm = Vm::new();
        let reader = vm.execute(&program).unwrap()[0];
        let root = vm.pin(reader).unwrap();
        let mut code = Vec::new();
        for (sequence, changed, expected) in
            [(11., false, 48.), (21., false, 58.), (31., true, -6.)]
        {
            let chunk = vm
                .call(
                    vm.rooted(root).unwrap(),
                    &[Value::Number(sequence), Value::Boolean(changed)],
                )
                .unwrap()[0];
            let Value::Function(id) = chunk else {
                panic!("reader must return a loaded chunk");
            };
            let closure = &vm.functions[slot_index(id.0)];
            code.push(
                vm.programs[closure.program].data.prototypes[closure.prototype]
                    .code
                    .as_ptr(),
            );
            assert_eq!(vm.call(chunk, &[]).unwrap(), &[Value::Number(expected)]);
        }
        assert_eq!(vm.compile_spare.reader_intervals.plans.len(), 1);
        let replay = &vm.compile_spare.reader_intervals.plans[0];
        assert_eq!(replay.preamble_event.0, 0);
        assert_eq!(code[0], code[1]);
        assert_ne!(code[1], code[2]);
    }

    #[test]
    fn reader_replay_publishes_the_current_number_when_the_first_function_stays_open() {
        let program = crate::compile(
            br#"
            return function(sequence)
                local blocks = {
                    "local function sequence() return " .. sequence .. " end\n--" ..
                    string.rep("x", 96),
                    string.rep("y", 128),
                    "\nreturn sequence()\n",
                }
                local offset, child = 0, nil
                return assert(load(function()
                    offset = offset + 1
                    if offset == 2 or offset == 3 then
                        local _, root = debug.getlocal(2, 6)
                        assert(type(root) == "table")
                        child = child or root
                        assert(child == root)
                        assert(rawget(root, sequence) == 0)
                        assert(rawget(root, 3 - sequence) == nil)
                    elseif offset == 4 then
                        assert(debug.getlocal(2, 6) == nil)
                    end
                    return blocks[offset]
                end, "@split-first-function"))
            end
            "#,
        )
        .unwrap();
        for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
            let callback = vm.execute(&program).unwrap()[0];
            let root = vm.pin(callback).unwrap();
            let mut entry_code = None;
            for sequence in [1.0, 2.0] {
                let chunk = vm
                    .call(vm.rooted(root).unwrap(), &[Value::Number(sequence)])
                    .unwrap()[0];
                let Value::Function(id) = chunk else {
                    panic!("expected compiled chunk");
                };
                assert_eq!(vm.compile_spare.reader_intervals.plans.len(), 1);
                let closure = &vm.functions[slot_index(id.0)];
                let code = vm.programs[closure.program].data.prototypes[closure.prototype]
                    .code
                    .as_ptr();
                if let Some(first) = entry_code {
                    assert_eq!(first, code);
                }
                entry_code = Some(code);
                assert_eq!(vm.call(chunk, &[]).unwrap(), &[Value::Number(sequence)]);
            }
        }
    }

    #[test]
    fn replayed_root_layouts_keep_order_and_reader_array_writes() {
        let program = crate::compile(
            br#"
            return function(sequence, edit, name)
                local blocks = {
                    "local function sequence() return " .. sequence .. " end\n" ..
                    "-- make room for the first packet prefix match\nlocal stable = 6\n",
                    "local alpha = 1; local beta = 2; local gamma = 3\n",
                    "local delta = 4; local epsilon = 'value'; local zeta = 7\n",
                    "return sequence() + alpha + gamma + zeta\n",
                }
                local offset, first, seen = 0, nil, {}
                local loaded = assert(load(function()
                    offset = offset + 1
                    if offset > 1 then
                        local _, root = debug.getlocal(2, 4)
                        assert(type(root) == 'table')
                        first = first or root
                        assert(first == root)
                        if edit and offset == 3 then
                            assert(rawget(root, 1) ~= nil)
                            rawset(root, 1, 999)
                        elseif edit and offset > 3 then
                            assert(rawget(root, 1) == 999)
                        end
                        for k, v in pairs(root) do
                            seen[#seen + 1] = type(k) .. ':' .. tostring(k) .. '=' .. tostring(v)
                        end
                        seen[#seen + 1] = ';'
                    end
                    return blocks[offset]
                end, name))
                return table.concat(seen, '|'), loaded
            end
            "#,
        )
        .unwrap();
        for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
            let callback = vm.execute(&program).unwrap()[0];
            let callback = vm.pin(callback).unwrap();
            let run = |vm: &mut Vm, sequence: f64, edit, name: &[u8]| {
                let name = vm.try_intern(name).unwrap();
                let [Value::String(log), loaded] = *vm
                    .call(
                        vm.rooted(callback).unwrap(),
                        &[Value::Number(sequence), Value::Boolean(edit), name],
                    )
                    .unwrap()
                else {
                    panic!("expected observations and a loaded chunk");
                };
                let log = vm.string_bytes(log).unwrap().to_vec();
                assert_eq!(
                    vm.call(loaded, &[]).unwrap(),
                    &[Value::Number(sequence + 11.0)]
                );
                log
            };
            run(&mut vm, 11.0, false, b"@warm-roots");
            run(&mut vm, 12.0, false, b"@warm-roots");
            assert!(
                vm.compile_spare.reader_intervals.plans[0]
                    .intervals
                    .iter()
                    .flat_map(|interval| &interval.segments)
                    .any(|segment| segment
                        .snapshot
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|s| s.layout.is_some()))
            );
            let edited = run(&mut vm, 13.0, true, b"@warm-roots");
            assert_eq!(edited, run(&mut vm, 13.0, true, b"@cold-edited"));
            vm.collect_garbage().unwrap();
            let unedited = run(&mut vm, 14.0, false, b"@warm-roots");
            assert_eq!(unedited, run(&mut vm, 14.0, false, b"@cold-clean"));
        }
    }
}
