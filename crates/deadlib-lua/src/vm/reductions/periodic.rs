use super::{Body, Instruction, Series, add_exact_sum, integer};

// Analysis visits each residue once, independent of the loop trip count.
const MAX_PERIOD: i128 = 64;

impl Body<'_> {
    pub(super) fn remainder(&mut self, input: Series, divisor: Series) -> Option<Series> {
        if divisor.first != divisor.last || divisor.first == 0 {
            return None;
        }
        // Within this range, binary64 division cannot round a noninteger
        // quotient past an integer. Flooring and multiplying back are exact.
        i32::try_from(input.first).ok()?;
        i32::try_from(input.last).ok()?;
        i32::try_from(divisor.first).ok()?;
        let modulus = divisor.first.abs();
        let remainder = |value: i128| {
            let value = value.rem_euclid(modulus);
            if divisor.first < 0 && value != 0 {
                value - modulus
            } else {
                value
            }
        };
        let quotient = |value: i128| {
            let numerator = if divisor.first < 0 { -value } else { value };
            numerator.div_euclid(modulus)
        };
        let first = remainder(input.first);
        if quotient(input.first) == quotient(input.last) {
            return Some(Series {
                first,
                last: remainder(input.last),
            });
        }
        let stride = (input.last - input.first) / (self.iterations - 1);
        let residue_step = stride.rem_euclid(modulus);
        if residue_step == 0 {
            return Some(Series::splat(first));
        }
        let (mut a, mut b) = (modulus, residue_step);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let period = modulus / a;
        if period <= MAX_PERIOD && period < self.iterations {
            self.period = Some(period);
        }
        None
    }

    pub(super) fn periodic_sum(
        &self,
        code: &[Instruction],
        start_pc: usize,
        indices: Series,
        step: i128,
        count: i128,
        mut period: i128,
    ) -> Option<((usize, f64), Option<usize>)> {
        // Bound analysis work across refinements and clipping splits.
        let mut attempts = MAX_PERIOD as usize * 4;
        'periods: loop {
            let final_lane = (count - 1) % period;
            let lane_step = step * period;
            let mut accumulated = None;
            let mut final_term = None;
            let mut registers = None;
            for lane in 0..period {
                let mut remaining = (count - 1 - lane) / period + 1;
                let mut segment_count = remaining;
                let mut first = indices.first + lane * step;
                loop {
                    attempts = attempts.checked_sub(1)?;
                    let last = first + (segment_count - 1) * lane_step;
                    let mut analysis = Self::new(
                        self.vm,
                        self.registers,
                        self.constants,
                        self.control,
                        self.env,
                        Series { first, last },
                        segment_count,
                    )?;
                    let Some((register, terms)) =
                        analysis.accumulator(code, start_pc, false).flatten()
                    else {
                        if let Some(factor) = analysis.period {
                            // This period is measured in lane iterations.
                            // Re-prove all lanes with the combined stride.
                            period *= factor;
                            if period > MAX_PERIOD || period >= count {
                                return None;
                            }
                            continue 'periods;
                        }
                        segment_count = analysis
                            .segment_limit
                            .filter(|boundary| (1..segment_count).contains(boundary))?;
                        continue;
                    };
                    if analysis.parked.is_some()
                        || analysis.assertion
                        || analysis.forwarded_calls_fit.is_some()
                    {
                        return None;
                    }
                    let total = match accumulated {
                        Some((previous, total)) if previous == register => total,
                        Some(_) => return None,
                        None => integer(self.registers[register])?,
                    };
                    // Exactness covers the original interleaved order too.
                    accumulated = Some((register, add_exact_sum(total, terms, segment_count)?));
                    registers = registers.max(analysis.call_registers);
                    remaining -= segment_count;
                    if remaining == 0 {
                        if lane == final_lane {
                            final_term = Some(terms.last);
                        }
                        break;
                    }
                    first = last + lane_step;
                    segment_count = remaining;
                }
            }
            let (register, total) = accumulated?;
            return Some(((register, (total - final_term?) as f64), registers));
        }
    }
}
