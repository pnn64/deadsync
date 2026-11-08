//! Lua 5.1 math operations, binary scaling and the pinned MSVC random stream.
use super::*;
use std::cell::Cell;

pub(super) const MATH_FIRST: u64 = 32;
pub(super) const MATH_LAST: u64 = 59;

// The Windows CRT used by the reference keeps rand/srand state per OS thread,
// shared by Lua states on that thread. This stream is portable and deterministic;
// its byte-exact reference contract is the Windows MSVC build.
thread_local! {static RANDOM:Cell<u32>=const {Cell::new(1)};}

fn fail(message: impl Into<Box<str>>) -> Error {
    Error::new(ErrorKind::Runtime, 0, message)
}

fn frexp(n: f64) -> (f64, i32) {
    let bits = n.to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    if exponent == 2047 {
        return (n, if cfg!(windows) { -1 } else { 0 });
    }
    if n == 0.0 {
        return (n, 0);
    }
    if exponent == 0 {
        let (fraction, e) = frexp(n * 18014398509481984.0);
        return (fraction, e - 54);
    }
    (
        f64::from_bits((bits & 0x800fffffffffffff) | (1022u64 << 52)),
        exponent - 1022,
    )
}

fn ldexp(n: f64, exponent: i32) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return n;
    }
    let (fraction, e) = frexp(n);
    let bits = fraction.to_bits();
    let sign = bits & (1u64 << 63);
    let target = i64::from(e) + i64::from(exponent) + 1022;
    if target >= 2047 {
        return f64::from_bits(sign | (2047u64 << 52));
    }
    if target > 0 {
        return f64::from_bits(sign | ((target as u64) << 52) | (bits & 0xfffffffffffff));
    }
    let shift = 1 - target;
    if shift > 53 {
        return f64::from_bits(sign);
    }
    let mantissa = (bits & 0xfffffffffffff) | (1u64 << 52);
    let mut rounded = mantissa >> shift;
    let remainder = mantissa & ((1u64 << shift) - 1);
    let half = 1u64 << (shift - 1);
    if remainder > half || (remainder == half && rounded & 1 != 0) {
        rounded += 1;
    }
    f64::from_bits(sign | rounded)
}

impl Vm {
    pub(super) fn open_math(&mut self) {
        let table = self.new_table();
        for (i, name) in [
            b"abs".as_slice(),
            b"floor",
            b"ceil",
            b"sqrt",
            b"sin",
            b"cos",
            b"tan",
            b"exp",
            b"log",
            b"log10",
            b"pow",
            b"min",
            b"max",
            b"fmod",
            b"asin",
            b"acos",
            b"atan",
            b"sinh",
            b"cosh",
            b"tanh",
            b"atan2",
            b"deg",
            b"rad",
            b"modf",
            b"frexp",
            b"ldexp",
            b"random",
            b"randomseed",
        ]
        .iter()
        .enumerate()
        {
            let key = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(key, Value::Native(MATH_FIRST + i as u64))
                .expect("math names are valid");
        }
        for (name, value) in [
            (b"mod".as_slice(), Value::Native(45)),
            (b"pi", Value::Number(std::f64::consts::PI)),
            (b"huge", Value::Number(f64::INFINITY)),
        ] {
            let key = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(key, value)
                .expect("math names are valid");
        }
        self.set_global(b"math", Value::Table(table))
            .expect("math name is valid");
    }
    fn math_number(&self, base: usize, count: usize, index: usize) -> Result<f64, Error> {
        self.number(self.argument(base, count, index))
            .ok_or_else(|| self.argument_type_error(base, count, index, "number"))
    }
    pub(super) fn math_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        if id == 58 {
            // Lua 5.1 draws before checking bounds or argument count, including errors.
            let r = RANDOM.with(|state| {
                let next = state.get().wrapping_mul(214013).wrapping_add(2531011);
                state.set(next);
                f64::from(((next >> 16) & 32767) % 32767) / 32767.0
            });
            let value = match count {
                0 => r,
                1 => {
                    let upper = self.integer_argument(base, count, 0, None)? as i32;
                    if upper < 1 {
                        return Err(self.argument_error(0, b"interval is empty"));
                    }
                    (r * f64::from(upper)).floor() + 1.0
                }
                2 => {
                    let lower = self.integer_argument(base, count, 0, None)? as i32;
                    let upper = self.integer_argument(base, count, 1, None)? as i32;
                    if lower > upper {
                        return Err(self.argument_error(1, b"interval is empty"));
                    }
                    (r * f64::from(upper.wrapping_sub(lower).wrapping_add(1))).floor()
                        + f64::from(lower)
                }
                _ => return Err(fail("wrong number of arguments")),
            };
            self.registers[base] = Value::Number(value);
            return Ok(1);
        }
        if id == 59 {
            let seed = self.integer_argument(base, count, 0, None)? as u32;
            RANDOM.with(|state| state.set(seed));
            return Ok(0);
        }
        if id == 57 {
            // The pinned MSVC Lua 5.1 build evaluates ldexp's exponent check
            // before its mantissa check. This determines which error is raised
            // when both arguments are invalid, including a missing exponent.
            let exponent = self.integer_argument(base, count, 1, None)? as i32;
            let x = self.math_number(base, count, 0)?;
            self.registers[base] = Value::Number(ldexp(x, exponent));
            return Ok(1);
        }
        if matches!(id, 42 | 45 | 52) {
            // The pinned Linux/glibc GCC oracle checks the second C-call
            // operand first; the MSVC oracle checks the first operand first.
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            let (x, y) = {
                let y = self.math_number(base, count, 1)?;
                (self.math_number(base, count, 0)?, y)
            };
            #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
            let (x, y) = {
                let x = self.math_number(base, count, 0)?;
                (x, self.math_number(base, count, 1)?)
            };
            let value = match id {
                42 => x.powf(y),
                45 => x % y,
                52 => x.atan2(y),
                _ => unreachable!("binary math IDs are fixed"),
            };
            self.registers[base] = Value::Number(value);
            return Ok(1);
        }
        let x = self.math_number(base, count, 0)?;
        let result = match id {
            32 => x.abs(),
            33 => x.floor(),
            34 => x.ceil(),
            35 => x.sqrt(),
            36 => x.sin(),
            37 => x.cos(),
            38 => x.tan(),
            39 => x.exp(),
            40 => x.ln(),
            41 => x.log10(),
            46 => x.asin(),
            47 => x.acos(),
            48 => x.atan(),
            49 => x.sinh(),
            50 => x.cosh(),
            51 => x.tanh(),
            53 => x / (std::f64::consts::PI / 180.0),
            54 => x * (std::f64::consts::PI / 180.0),
            43 | 44 => {
                let mut best = x;
                for index in 1..count {
                    let n = self.math_number(base, count, index)?;
                    if if id == 43 { n < best } else { n > best } {
                        best = n;
                    }
                }
                best
            }
            55 => {
                let integral = x.trunc();
                let fraction = if x.is_infinite() {
                    0.0f64.copysign(x)
                } else {
                    (x - integral).copysign(x)
                };
                self.resize_registers(self.registers.len().max(base + 2));
                self.registers[base] = Value::Number(integral);
                self.registers[base + 1] = Value::Number(fraction);
                return Ok(2);
            }
            56 => {
                let (fraction, exponent) = frexp(x);
                self.resize_registers(self.registers.len().max(base + 2));
                self.registers[base] = Value::Number(fraction);
                self.registers[base + 1] = Value::Number(f64::from(exponent));
                return Ok(2);
            }
            _ => unreachable!("math ID range is fixed"),
        };
        self.registers[base] = Value::Number(result);
        Ok(1)
    }
    pub(super) fn tonumber_native(&mut self, base: usize, count: usize) -> Result<Value, Error> {
        let radix = self.integer_argument(base, count, 1, Some(10))? as i32;
        if radix == 10 {
            if count == 0 {
                return Err(self.argument_error(0, b"value expected"));
            }
            return Ok(self
                .number(self.argument(base, count, 0))
                .map_or(Value::Nil, Value::Number));
        }
        let string = self.string_argument(base, count, 0)?;
        if !(2..=36).contains(&radix) {
            return Err(self.argument_error(1, b"base out of range"));
        }
        let text = &self.strings.bytes[slot_index(string.0)];
        let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
        let whitespace = |c: &u8| crate::locale::is_space(*c);
        let first = text
            .iter()
            .position(|c| !whitespace(c))
            .unwrap_or(text.len());
        let last = text
            .iter()
            .rposition(|c| !whitespace(c))
            .map_or(first, |i| i + 1);
        let text = &text[first..last];
        let (negative, text) = match text.first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        let text = if radix == 16 && (text.starts_with(b"0x") || text.starts_with(b"0X")) {
            &text[2..]
        } else {
            text
        };
        if text.is_empty() {
            return Ok(Value::Nil);
        }
        let maximum = if std::mem::size_of::<std::ffi::c_ulong>() == 4 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };
        let mut value = 0u64;
        let mut overflow = false;
        for &byte in text {
            let digit = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'z' => byte - b'a' + 10,
                b'A'..=b'Z' => byte - b'A' + 10,
                _ => return Ok(Value::Nil),
            };
            if i32::from(digit) >= radix {
                return Ok(Value::Nil);
            }
            match value
                .checked_mul(radix as u64)
                .and_then(|v| v.checked_add(u64::from(digit)))
                .filter(|&v| v <= maximum)
            {
                Some(n) => value = n,
                None => {
                    value = maximum;
                    overflow = true;
                }
            }
        }
        if negative && !overflow {
            value = value.wrapping_neg() & maximum;
        }
        Ok(Value::Number(value as f64))
    }
}
