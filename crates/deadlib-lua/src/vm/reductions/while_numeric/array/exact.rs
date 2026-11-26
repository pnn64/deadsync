//! Reuse an extrema sum when every intermediate addition is exact.
use super::branches::CachedBranch;
use super::calls::CachedCall;
use super::{Plan, TableId, Term, Value, array_extremum};
use crate::memory::{Buffer, Memory};
use crate::table::{ARRAY_WATCH_BLOCK, ARRAY_WATCH_LIMIT, ArrayChange, Table};

pub(super) const MIN_LENGTH: i64 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Clamp {
    order: u8,
    bound_bits: u64,
}

impl Clamp {
    fn of(term: Term) -> Option<Self> {
        let (order, bound) = array_extremum(term)?;
        Some(Self {
            order,
            bound_bits: bound.to_bits(),
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Key {
    table: TableId,
    bounds: (i64, i64),
    ascending: bool,
    clamps: [Option<Clamp>; 2],
}

#[derive(Debug)]
enum State {
    Seen,
    WaitForClean,
    Computed(Option<Exact>),
}

#[derive(Debug)]
struct Entry {
    key: Key,
    state: State,
}

#[derive(Debug)]
enum Cached {
    Extrema(Entry),
    Call(CachedCall),
    Branch(CachedBranch),
}

// A table watch belongs to the single cache entry that armed it. Replacing
// that entry discards its proof before the new owner can rearm any watches.
#[derive(Debug)]
pub(in crate::vm) struct ArrayReduction {
    entry: Option<Cached>,
    blocks: Buffer<Exact>,
}

impl ArrayReduction {
    pub(in crate::vm) fn new_in(memory: Memory) -> Self {
        Self {
            entry: None,
            blocks: Buffer::new_in(memory),
        }
    }

    pub(in crate::vm) fn storage_bytes(&self) -> usize {
        self.blocks.capacity() * size_of::<Exact>()
    }

    pub(super) fn cached_call(&self) -> Option<&CachedCall> {
        match &self.entry {
            Some(Cached::Call(call)) => Some(call),
            _ => None,
        }
    }

    pub(super) fn cache_call(&mut self, call: CachedCall) {
        self.blocks.clear();
        self.entry = Some(Cached::Call(call));
    }

    pub(super) fn cached_branch(&self) -> Option<&CachedBranch> {
        match &self.entry {
            Some(Cached::Branch(branch)) => Some(branch),
            _ => None,
        }
    }

    pub(super) fn cache_branch(&mut self, branch: CachedBranch) {
        self.blocks.clear();
        self.entry = Some(Cached::Branch(branch));
    }

    pub(super) fn evaluate(
        &mut self,
        table: &mut Table,
        plan: &Plan,
        bounds: (i64, i64),
        ascending: bool,
        initial: f64,
    ) -> Option<f64> {
        if bounds.1 - bounds.0 + 1 < MIN_LENGTH {
            return None;
        }
        let terms = &plan.terms.terms[..plan.terms.count];
        let clamps = match *terms {
            [first] => [Some(Clamp::of(first)?), None],
            [first, second] => [Some(Clamp::of(first)?), Some(Clamp::of(second)?)],
            _ => return None,
        };
        let key = Key {
            table: plan.table,
            bounds,
            ascending,
            clamps,
        };
        let change = table.array_change();
        let matches = match change {
            ArrayChange::Unwatched => false,
            ArrayChange::Clean => true,
            ArrayChange::Block(_) => {
                bounds.1 <= ARRAY_WATCH_LIMIT as i64 && self.blocks.memory().uses_global_allocator()
            }
        };
        let Some(entry) = self.entry.as_mut().and_then(|entry| match entry {
            Cached::Extrema(entry) if matches && entry.key == key => Some(entry),
            _ => None,
        }) else {
            table.watch_array();
            self.blocks.clear();
            self.entry = Some(Cached::Extrema(Entry {
                key,
                state: State::Seen,
            }));
            return None;
        };
        match change {
            ArrayChange::Block(block) => {
                if matches!(entry.state, State::Computed(None) | State::WaitForClean) {
                    // A rejected proof can be retried once the array stops changing.
                    entry.state = State::WaitForClean;
                } else {
                    let exact = table.array_slice(bounds.0, bounds.1).and_then(|values| {
                        let prefix = Exact::prefix(values, ascending)?;
                        let start = bounds.0 as usize - usize::from(ascending);
                        repair(&mut self.blocks, prefix, start, block, terms)
                    });
                    if exact.is_none() {
                        self.blocks.clear();
                    }
                    entry.state = State::Computed(exact);
                }
                table.watch_array();
            }
            ArrayChange::Clean if matches!(entry.state, State::Seen | State::WaitForClean) => {
                let exact = table
                    .array_slice(bounds.0, bounds.1)
                    .and_then(|values| Exact::scan(values, terms, ascending));
                entry.state = State::Computed(exact);
            }
            _ => {}
        }
        match &entry.state {
            State::Computed(Some(exact)) => exact.apply(initial),
            _ => None,
        }
    }
}

fn repair(
    blocks: &mut Buffer<Exact>,
    values: &[Value],
    start: usize,
    dirty: usize,
    terms: &[Term],
) -> Option<Exact> {
    let first = start / ARRAY_WATCH_BLOCK;
    let block_values = |block: usize| {
        let lo = (block * ARRAY_WATCH_BLOCK).saturating_sub(start);
        let hi = ((block + 1) * ARRAY_WATCH_BLOCK - start).min(values.len());
        &values[lo..hi]
    };
    if blocks.is_empty() {
        let end = (start + values.len()).div_ceil(ARRAY_WATCH_BLOCK);
        blocks.try_reserve_exact(end - first).ok()?;
        for block in first..end {
            blocks.push(Exact::terms(block_values(block), terms)?);
        }
    } else if let Some(block) = dirty
        .checked_sub(first)
        .and_then(|index| blocks.get_mut(index))
    {
        *block = Exact::terms(block_values(dirty), terms)?;
    }
    let mut exact = Exact::ZERO;
    for block in blocks.iter() {
        exact.delta += block.delta;
        exact.precision = exact.precision.join(block.precision);
    }
    exact.check_count(values.len(), terms.len())
}

#[derive(Clone, Copy, Debug)]
struct Precision {
    // Absolute values are integer multiples of 2^least and less than 2^upper.
    least: i32,
    upper: i32,
}

impl Precision {
    const ZERO: Self = Self {
        least: 1024,
        upper: -1074,
    };

    fn of(value: f64) -> Option<Self> {
        if value == 0.0 {
            return Some(Self::ZERO);
        }
        let bits = value.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as i32;
        if exponent == 0 || exponent == 0x7ff {
            return None;
        }
        let upper = exponent - 1022;
        // The implicit leading bit also handles a zero fraction.
        let significand = (bits & ((1 << 52) - 1)) | (1 << 52);
        let least = upper - 53 + significand.trailing_zeros() as i32;
        Some(Self { least, upper })
    }

    fn join(self, other: Self) -> Self {
        Self {
            least: self.least.min(other.least),
            upper: self.upper.max(other.upper),
        }
    }

    fn fits(self) -> bool {
        self.upper <= 1024 && self.upper - self.least <= f64::MANTISSA_DIGITS as i32
    }
}

#[derive(Debug)]
pub(super) struct Exact {
    delta: f64,
    precision: Precision,
}

impl Exact {
    pub(super) const ZERO: Self = Self {
        delta: 0.0,
        precision: Precision::ZERO,
    };

    fn prefix(values: &[Value], ascending: bool) -> Option<&[Value]> {
        let (last, prefix) = if ascending {
            values.split_last()?
        } else {
            values.split_first()?
        };
        matches!(last, Value::Number(_)).then_some(prefix)
    }

    fn scan(values: &[Value], terms: &[Term], ascending: bool) -> Option<Self> {
        let prefix = Self::prefix(values, ascending)?;
        Self::terms(prefix, terms)?.check_count(prefix.len(), terms.len())
    }

    fn terms(values: &[Value], terms: &[Term]) -> Option<Self> {
        let mut exact = Self::ZERO;
        for &value in values {
            let Value::Number(current) = value else {
                return None;
            };
            for &term in terms {
                exact.push(term.value(current))?;
            }
        }
        Some(exact)
    }

    pub(super) fn push(&mut self, value: f64) -> Option<()> {
        self.precision = self.precision.join(Precision::of(value)?);
        self.delta += value;
        Some(())
    }

    pub(super) fn check_count(mut self, values: usize, terms: usize) -> Option<Self> {
        // The triangle inequality bounds all partial sums, including the
        // regrouped sums of blocks and the original sequence of additions.
        let count = values.checked_mul(terms)?;
        self.precision.upper += count
            .checked_sub(1)?
            .checked_ilog2()
            .map_or(0, |log| log as i32 + 1);
        self.precision.fits().then_some(self)
    }

    pub(super) fn apply(&self, initial: f64) -> Option<f64> {
        if initial == 0.0 {
            // Starting at -0 can preserve its sign when all terms are -0.
            return (!initial.is_sign_negative()).then_some(self.delta);
        }
        let mut combined = self.precision.join(Precision::of(initial)?);
        combined.upper += 1;
        combined.fits().then(|| initial + self.delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vm;
    use crate::vm::reductions::while_numeric::Input;

    #[test]
    fn exact_array_sum_rejects_rounding_and_negative_zero() {
        let values = [Value::Number(0.25); 65];
        let exact = Exact::scan(&values, &[Term::Input(Input::Current)], true).unwrap();
        assert_eq!(exact.apply(0.0), Some(16.0));
        assert_eq!(exact.apply(-16.0).unwrap().to_bits(), 0);
        assert_eq!(exact.apply(1.25), Some(17.25));
        assert!(exact.apply(9_007_199_254_740_992.0).is_none());
        assert!(exact.apply(f64::INFINITY).is_none());
        assert!(exact.apply(f64::from_bits(1)).is_none());

        let zeros = [Value::Number(-0.0); 65];
        let exact = Exact::scan(&zeros, &[Term::Input(Input::Current)], false).unwrap();
        assert_eq!(exact.apply(0.0).unwrap().to_bits(), 0);
        assert!(exact.apply(-0.0).is_none());
    }

    #[test]
    fn array_sum_cache_rechecks_writes_sort_and_clamp_arguments() {
        let program = crate::compile(
            br#"
            local values = {}
            for i = 1, 64 do values[i] = 2 end
            local min, max = math.min, math.max
            collectgarbage("stop")
            return function(mode, total, bound)
                if mode == 1 then values[1] = 9
                elseif mode == 2 then table.sort(values)
                elseif mode == 3 then rawset(values, 1, '10')
                elseif mode == 4 then values[1], values[33] = 3, 5
                elseif mode == 5 then values[1] = 3
                end
                for i = 1, 64 do
                    local value = values[i]
                    total = total + min(value, bound) + max(0, value)
                end
                return total
            end
            "#,
        )
        .unwrap();
        let mut vm = Vm::new();
        let scan = vm.execute(&program).unwrap()[0];
        let scan = vm.pin(scan).unwrap();
        let run = |vm: &mut Vm, mode, initial, bound, expected| {
            assert_eq!(
                vm.call(
                    vm.rooted(scan).unwrap(),
                    &[
                        Value::Number(mode),
                        Value::Number(initial),
                        Value::Number(bound)
                    ],
                )
                .unwrap(),
                &[Value::Number(expected)]
            );
        };
        let warmed = |vm: &Vm| {
            matches!(
                vm.array_reduction.entry.as_ref(),
                Some(Cached::Extrema(Entry {
                    state: State::Computed(Some(_)),
                    ..
                }))
            )
        };
        run(&mut vm, 0., 0., 3., 256.);
        run(&mut vm, 0., 0., 3., 256.);
        assert!(warmed(&vm));
        run(&mut vm, 1., 0., 3., 264.);
        assert!(warmed(&vm));
        run(&mut vm, 0., 0., 3., 264.);
        assert!(warmed(&vm));
        run(&mut vm, 2., 0., 3., 264.);
        assert!(!warmed(&vm));
        run(&mut vm, 0., 0., 3., 264.);
        assert!(warmed(&vm));
        run(&mut vm, 0., 0., 1., 199.);
        assert!(!warmed(&vm));
        run(&mut vm, 0., 0., 1., 199.);
        assert!(warmed(&vm));
        run(&mut vm, 4., 0., 1., 203.);
        assert!(!warmed(&vm));
        run(&mut vm, 0., 0., 1., 203.);
        assert!(warmed(&vm));
        run(&mut vm, 3., 0., 1., 210.);
        run(&mut vm, 0., 0., 1., 210.);
        assert!(!warmed(&vm));
        run(&mut vm, 5., 0., 1., 203.);
        assert!(!warmed(&vm));
        run(&mut vm, 0., 0., 1., 203.);
        assert!(warmed(&vm));
    }
}
