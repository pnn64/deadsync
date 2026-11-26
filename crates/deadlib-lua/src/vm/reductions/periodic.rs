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
        period: i128,
    ) -> Option<((usize, f64), Option<usize>)> {
        let final_lane = (count - 1) % period;
        let lane_step = step * period;
        let mut accumulated = None;
        let mut final_term = None;
        let mut registers = None;
        for lane in 0..period {
            let lane_count = (count - 1 - lane) / period + 1;
            let first = indices.first + lane * step;
            let last = first + (lane_count - 1) * lane_step;
            let mut analysis = Self::new(
                self.vm,
                self.registers,
                self.constants,
                self.control,
                self.env,
                Series { first, last },
                lane_count,
            )?;
            let (register, terms) = analysis.accumulator(code, start_pc, false)??;
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
            // The exactness bound also covers the original interleaved order.
            accumulated = Some((register, add_exact_sum(total, terms, lane_count)?));
            if lane == final_lane {
                final_term = Some(terms.last);
            }
            registers = registers.max(analysis.call_registers);
        }
        let (register, total) = accumulated?;
        Some(((register, (total - final_term?) as f64), registers))
    }
}
