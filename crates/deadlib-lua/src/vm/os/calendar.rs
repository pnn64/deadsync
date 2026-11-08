//! Lua 5.1 calendar field order, callback continuations and date formatting.
use super::*;
const FIELDS: [&[u8]; 7] = [b"sec", b"min", b"hour", b"day", b"month", b"year", b"isdst"];

pub(super) fn format_with(
    format: &[u8],
    output: &mut Vec<u8>,
    mut convert: impl FnMut(&[u8; 3], &mut [u8; 200]) -> usize,
) {
    let mut start = 0;
    let mut cursor = 0;
    while cursor + 1 < format.len() {
        if format[cursor] != b'%' {
            cursor += 1;
            continue;
        }
        output.extend_from_slice(&format[start..cursor]);
        let spec = [b'%', format[cursor + 1], 0];
        let mut bytes = [0; 200];
        let count = convert(&spec, &mut bytes);
        output.extend_from_slice(&bytes[..count]);
        cursor += 2;
        start = cursor;
    }
    output.extend_from_slice(&format[start..]);
}

impl Vm {
    pub(in crate::vm) fn time_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        let mut ready = frame.pc() != 0;
        if frame.pc() == 0 {
            let value = self.argument(frame.vararg_start - 1, frame.argc(), 0);
            if value == Value::Nil {
                self.registers[base] =
                    platform::now()?.map_or(Value::Nil, |n| Value::Number(n as f64));
                return Ok(self.return_frame(frame, base, 1));
            }
            if !matches!(value, Value::Table(_)) {
                return Err(self.argument_type_error(
                    frame.vararg_start - 1,
                    frame.argc(),
                    0,
                    "table",
                ));
            }
            // Lua 5.1 sets the C argument stack top to one before lookups.
            // These are the caller's owned call-argument temporaries; clearing
            // discarded extras also removes them from suspended-call GC maps.
            let start = frame.vararg_start;
            self.registers[start + 1..start + frame.argc()].fill(Value::Nil);
            self.resize_registers(self.registers.len().max(base + 9));
            self.registers[base] = value;
            frame.vararg_start = base;
            frame.vararg_count = 1;
            frame.pc = 1;
        }
        loop {
            if ready {
                let index = frame.pc() - 1;
                let value = self.registers[base + 8];
                let mut number = if index == 6 {
                    if value == Value::Nil {
                        -1
                    } else {
                        i32::from(value.is_truthy())
                    }
                } else if let Some(n) = self.number(value) {
                    if cfg!(target_pointer_width = "32") {
                        debug::integer(n)
                    } else {
                        io::integer(n) as i32
                    }
                } else if index < 3 {
                    [0, 0, 12][index]
                } else {
                    let mut text = b"field '".to_vec();
                    text.extend_from_slice(FIELDS[index]);
                    text.extend_from_slice(b"' missing in date table");
                    return Err(Error::from_bytes(ErrorKind::Runtime, 0, text));
                };
                if index == 4 {
                    number = number.wrapping_sub(1);
                }
                if index == 5 {
                    number = number.wrapping_sub(1900);
                }
                self.registers[base + index + 1] = Value::Number(f64::from(number));
                // Each successful C field read pops its original value immediately.
                self.registers[base + 8] = Value::Nil;
                if index == 6 {
                    let mut date = platform::Calendar::default();
                    for index in 0..6 {
                        let Value::Number(n) = self.registers[base + index + 1] else {
                            unreachable!()
                        };
                        date.fields[index] = n as i32;
                    }
                    date.fields[8] = number;
                    self.registers[base + 1] =
                        platform::make_time(date)?.map_or(Value::Nil, |n| Value::Number(n as f64));
                    return Ok(self.return_frame(frame, base + 1, 1));
                }
                frame.pc += 1;
            }
            self.frames[owner] = frame;
            let key = self.intern(FIELDS[frame.pc() - 1]);
            let depth = self.frames.len();
            self.get_table(self.registers[base], key, base + 8)?;
            if self.frames.len() != depth {
                return Ok(None);
            }
            // Raw fields and table-only __index chains need no scheduler turn.
            ready = true;
        }
    }

    pub(in crate::vm::os) fn date_native(
        &mut self,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        let format = if self.argument(base, count, 0) == Value::Nil {
            None
        } else {
            Some(self.string_argument(base, count, 0)?)
        };
        let time = match self.argument(base, count, 1) {
            Value::Nil => platform::now()?.unwrap_or(-1),
            value => platform::timestamp(
                self.number(value)
                    .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?,
            ),
        };
        let (start, end, utc, table) = {
            let bytes = format.map_or(b"%c".as_slice(), |id| &self.strings.bytes[slot_index(id.0)]);
            let bytes = io::c_bytes(bytes);
            let utc = bytes.first() == Some(&b'!');
            let start = usize::from(utc);
            (start, bytes.len(), utc, &bytes[start..] == b"*t")
        };
        if table {
            let Some(date) = platform::date_parts(time, utc)? else {
                self.registers[base] = Value::Nil;
                return Ok(1);
            };
            let table = self.new_record_table(8 + usize::from(date.fields[8] >= 0));
            for (index, name) in [
                b"sec".as_slice(),
                b"min",
                b"hour",
                b"day",
                b"month",
                b"year",
                b"wday",
                b"yday",
            ]
            .iter()
            .enumerate()
            {
                let offset = match index {
                    4 | 6 | 7 => 1,
                    5 => 1900,
                    _ => 0,
                };
                let name = self.intern(name);
                self.raw_set(
                    table,
                    name,
                    Value::Number(f64::from(date.fields[index].wrapping_add(offset))),
                )?;
            }
            if date.fields[8] >= 0 {
                let name = self.intern(b"isdst");
                self.raw_set(table, name, Value::Boolean(date.fields[8] != 0))?;
            }
            self.registers[base] = Value::Table(table);
        } else {
            let mut output = self.take_buffer(64);
            let bytes = format.map_or(b"%c".as_slice(), |id| &self.strings.bytes[slot_index(id.0)]);
            let valid = platform::format_date(time, utc, &bytes[start..end], &mut output)?;
            self.registers[base] = if valid {
                self.finish_buffer(output)
            } else {
                self.buffers.push(output);
                Value::Nil
            };
        }
        Ok(1)
    }
}
