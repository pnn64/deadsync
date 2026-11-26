//! Preserve the parser events and depth at each boundary of a successful load.
//! Every reader still supplies the next block. A mismatching block resumes the
//! original grammar after reconstructing its state from the accepted prefix.
use super::*;
use crate::memory::{Shared, text::Text};
use crate::parser::{NativeRootEvent, RootText};

const MIN_PREFIX: usize = 64;
const MAX_ENTRIES: usize = 24;
const MAX_BATCHES: usize = 256;
const MAX_EVENT_BYTES: usize = 128 * 1024;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct Interval {
    end: usize,
    nesting: usize,
    closed: bool,
    events: Box<[NativeRootEvent]>,
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
    bytes: usize,
}

impl Plan {
    pub(in crate::vm) fn events(&self, interval: usize) -> &[NativeRootEvent] {
        &self.intervals[interval].events
    }
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

    pub(super) fn replayed_interval(&self) -> Option<(Shared<Plan>, usize)> {
        let Self::Replay(playback) = self else {
            return None;
        };
        Some((playback.plan.clone(), playback.next - 1))
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
        let bytes = self.bytes.checked_add(std::mem::size_of_val(events))?;
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
            saved.push(copy_event(event)?);
        }
        self.intervals
            .try_push(Interval {
                end: source.len(),
                nesting,
                closed,
                events: saved.try_into_boxed_slice().ok()?,
            })
            .ok()?;
        self.bytes = bytes;
        Some(())
    }
}

fn hidden_first_constant(interval: &Interval, number: f64) -> bool {
    use NativeRootEvent::*;
    let [Open, tail @ ..] = interval.events.as_ref() else {
        return false;
    };
    let Some(first_child) = tail
        .iter()
        .position(|event| matches!(event, Open | OpenWithoutTable))
    else {
        return false;
    };
    if !matches!(tail[first_child], OpenWithoutTable) {
        return false;
    }
    // This function opened and closed within the first packet. It has no
    // public constant table, and a number adds no edge to the prototype.
    // The original event is safe to replay when this number changes.
    let mut seen = false;
    for event in &tail[first_child + 1..] {
        match event {
            Constant(crate::bytecode::Constant::Number(value)) if !seen && *value == number => {
                seen = true;
            }
            Close => return seen,
            Open | OpenWithoutTable | Constant(_) | ConstantText(_) => return false,
            _ => {}
        }
    }
    false
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
        let Trace::Record(recording) = trace else {
            return;
        };
        if !recording
            .intervals
            .last()
            .is_some_and(|interval| interval.closed)
            || recording.locale != crate::locale::revision()
            || !hidden_first_constant(&recording.intervals[0], recording.preamble.value)
        {
            return;
        }
        let Some(target) = recording.preamble.function_target(&program.data) else {
            return;
        };
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
                bytes,
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
        let first = &vm.compile_spare.reader_intervals.plans[0].intervals[0];
        assert!(hidden_first_constant(first, 11.0));
        assert_eq!(code[0], code[1]);
        assert_ne!(code[1], code[2]);
    }
}
