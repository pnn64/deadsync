//! Lua 5.1 tag-method lookup and callback scheduling.
use super::*;
use std::cmp::Ordering;

impl Vm {
    #[inline]
    pub(super) fn number_op(op: Op, x: f64, y: f64) -> f64 {
        match op {
            Op::Add => x + y,
            Op::Sub => x - y,
            Op::Mul => x * y,
            Op::Div => x / y,
            Op::Mod => x - (x / y).floor() * y,
            Op::Pow => x.powf(y),
            Op::Neg => -x,
            _ => unreachable!("arithmetic opcode"),
        }
    }
    fn type_slot(value: Value) -> usize {
        match value {
            Value::Nil => 0,
            Value::Boolean(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Function(_) | Value::Native(_) => 4,
            Value::Table(_) => 5,
            Value::Thread(_) => 6,
            Value::Userdata(_) => 7,
            Value::LightUserdata(_) => 8,
        }
    }
    pub(super) fn metatable(&self, value: Value) -> Option<TableId> {
        match value {
            Value::Table(id) => self.tables.get(id.0).and_then(|t| t.metatable),
            Value::Userdata(id) => self.userdata.get(id.0).and_then(|u| u.metatable),
            _ => self.type_metatables[Self::type_slot(value)],
        }
    }
    pub(super) fn set_metatable(&mut self, value: Value, table: Option<TableId>) {
        match value {
            Value::Table(id) => self.tables[slot_index(id.0)].metatable = table,
            Value::Userdata(id) => self.userdata[slot_index(id.0)].metatable = table,
            _ => self.type_metatables[Self::type_slot(value)] = table,
        }
        self.gc_metatable_write(value, table);
    }
    pub(super) fn metafield(&self, value: Value, event: Meta) -> Value {
        self.metatable(value).map_or(Value::Nil, |mt| {
            self.tables[slot_index(mt.0)].get(Value::String(self.meta_names[event as usize]))
        })
    }
    fn binary_handler(&self, left: Value, right: Value, event: Meta) -> Value {
        let first = self.metafield(left, event);
        if first == Value::Nil {
            self.metafield(right, event)
        } else {
            first
        }
    }
    fn shared_handler(&self, left: Value, right: Value, event: Meta) -> Value {
        let first = self.metafield(left, event);
        if first != Value::Nil && first == self.metafield(right, event) {
            first
        } else {
            Value::Nil
        }
    }
    pub(super) fn get_table(
        &mut self,
        mut target: Value,
        key: Value,
        result: usize,
    ) -> Result<(), Error> {
        for step in 0..100 {
            let handler = if let Value::Table(id) = target {
                let value = self.tables[slot_index(id.0)].get(key);
                if value != Value::Nil {
                    self.registers[result] = value;
                    return Ok(());
                }
                let handler = self.metafield(target, Meta::Index);
                if handler == Value::Nil {
                    self.registers[result] = Value::Nil;
                    return Ok(());
                }
                handler
            } else {
                let handler = self.metafield(target, Meta::Index);
                if handler == Value::Nil {
                    return Err(self.type_error(
                        target,
                        "index",
                        if step == 0 {
                            self.error_register(false)
                        } else {
                            None
                        },
                    ));
                }
                handler
            };
            if matches!(handler, Value::Function(_) | Value::Native(_)) {
                return self.meta_call(handler, &[target, key], result, CONT_RAW);
            }
            target = handler;
        }
        Err(Error::new(ErrorKind::Runtime, 0, "loop in gettable"))
    }
    pub(super) fn set_table(
        &mut self,
        mut target: Value,
        key: Value,
        value: Value,
    ) -> Result<(), Error> {
        for step in 0..100 {
            let handler = if let Value::Table(id) = target {
                // Lua checks invalid keys before consulting __newindex on tables.
                if key == Value::Nil || matches!(key,Value::Number(n) if n.is_nan()) {
                    return self.raw_set(id, key, value);
                }
                let old = self.tables[slot_index(id.0)].get(key);
                let handler = self.metafield(target, Meta::NewIndex);
                if old != Value::Nil || handler == Value::Nil {
                    return self.raw_set(id, key, value);
                }
                handler
            } else {
                let handler = self.metafield(target, Meta::NewIndex);
                if handler == Value::Nil {
                    return Err(self.type_error(
                        target,
                        "index",
                        if step == 0 {
                            self.error_register(false)
                        } else {
                            None
                        },
                    ));
                }
                handler
            };
            if matches!(handler, Value::Function(_) | Value::Native(_)) {
                return self.meta_call(handler, &[target, key, value], 0, CONT_IGNORE);
            }
            target = handler;
        }
        Err(Error::new(ErrorKind::Runtime, 0, "loop in settable"))
    }
    pub(super) fn arithmetic(
        &mut self,
        op: Op,
        left: Value,
        right: Value,
        result: usize,
    ) -> Result<(), Error> {
        if let (Some(x), Some(y)) = (self.number(left), self.number(right)) {
            self.registers[result] = Value::Number(Self::number_op(op, x, y));
            return Ok(());
        }
        let event = match op {
            Op::Add => Meta::Add,
            Op::Sub => Meta::Sub,
            Op::Mul => Meta::Mul,
            Op::Div => Meta::Div,
            Op::Mod => Meta::Mod,
            Op::Pow => Meta::Pow,
            Op::Neg => Meta::Unm,
            _ => unreachable!("arithmetic opcode"),
        };
        let handler = self.binary_handler(left, right, event);
        if handler != Value::Nil {
            return self.meta_call(handler, &[left, right], result, CONT_RAW);
        }
        let right_is_bad = self.number(left).is_some();
        Err(self.type_error(
            if right_is_bad { right } else { left },
            "perform arithmetic on",
            self.error_register(right_is_bad),
        ))
    }
    fn concat_string(&mut self, register: usize) -> Option<StringId> {
        match self.registers[register] {
            Value::String(id) => Some(id),
            Value::Number(n) => {
                let value = self.intern(&format_number(n));
                self.registers[register] = value;
                let Value::String(id) = value else {
                    unreachable!()
                };
                Some(id)
            }
            _ => None,
        }
    }
    pub(super) fn concat_range(
        &mut self,
        result: usize,
        first: usize,
        mut last: usize,
    ) -> Result<(), Error> {
        while last > first {
            // Coercions modify their original slots, in the native VM's order.
            let left_ok = matches!(
                self.registers[last - 1],
                Value::String(_) | Value::Number(_)
            );
            let right = if left_ok {
                self.concat_string(last)
            } else {
                None
            };
            if let Some(right) = right {
                let right_len = self.strings.bytes[slot_index(right.0)].len();
                if right_len == 0 {
                    self.concat_string(last - 1);
                    last -= 1;
                    continue;
                }
                let mut start = last;
                let mut size = right_len;
                while start > first {
                    let Some(id) = self.concat_string(start - 1) else {
                        break;
                    };
                    size = size
                        .checked_add(self.strings.bytes[slot_index(id.0)].len())
                        .ok_or_else(|| {
                            Error::new(ErrorKind::Runtime, 0, "string length overflow")
                        })?;
                    start -= 1;
                }
                let mut bytes = self.take_buffer(size);
                for r in start..=last {
                    let Value::String(id) = self.registers[r] else {
                        unreachable!()
                    };
                    bytes.extend_from_slice(&self.strings.bytes[slot_index(id.0)]);
                }
                self.registers[start] = self.finish_buffer(bytes);
                last = start;
            } else {
                let left = self.registers[last - 1];
                let right = self.registers[last];
                let handler = self.binary_handler(left, right, Meta::Concat);
                if handler == Value::Nil {
                    let r = if matches!(left, Value::String(_) | Value::Number(_)) {
                        last
                    } else {
                        last - 1
                    };
                    return Err(self.type_error(self.registers[r], "concatenate", Some(r)));
                }
                let base = self.registers.len();
                self.push_record(Frame {
                    function: FrameKind::Native(CONT_CONCAT),
                    pc: self.top as u32,
                    base,
                    stack_start: base,
                    result,
                    wanted: 0,
                    barriers: self.frames.last().map_or(0, |f| f.barriers),
                    vararg_start: first,
                    vararg_count: (last - first) as u32,
                    tail_calls: 0,
                });
                return self.meta_call(handler, &[left, right], last - 1, CONT_RAW);
            }
        }
        self.registers[result] = self.registers[first];
        Ok(())
    }
    pub(super) fn concatenate(
        &mut self,
        left: Value,
        right: Value,
        result: usize,
    ) -> Result<(), Error> {
        if matches!(left, Value::String(_) | Value::Number(_))
            && matches!(right, Value::String(_) | Value::Number(_))
        {
            let mut bytes = self.concat_bytes(left)?;
            bytes.extend_from_slice(&self.concat_bytes(right)?);
            self.registers[result] = self.intern(&bytes);
            return Ok(());
        }
        let handler = self.binary_handler(left, right, Meta::Concat);
        if handler != Value::Nil {
            return self.meta_call(handler, &[left, right], result, CONT_RAW);
        }
        let right_is_bad = matches!(left, Value::String(_) | Value::Number(_));
        Err(self.type_error(
            if right_is_bad { right } else { left },
            "concatenate",
            self.error_register(right_is_bad),
        ))
    }
    pub(super) fn length(&mut self, value: Value, result: usize) -> Result<(), Error> {
        let n = match value {
            Value::Table(id) => self.tables[slot_index(id.0)].len(),
            Value::String(id) => self.string_bytes(id).unwrap_or_default().len(),
            _ => {
                let handler = self.binary_handler(value, Value::Nil, Meta::Len);
                if handler != Value::Nil {
                    return self.meta_call(handler, &[value, Value::Nil], result, CONT_RAW);
                }
                return Err(self.type_error(value, "get length of", self.error_register(false)));
            }
        };
        self.registers[result] = Value::Number(n as f64);
        Ok(())
    }
    pub(super) fn compare(
        &mut self,
        op: Op,
        mut left: Value,
        mut right: Value,
        result: usize,
    ) -> Result<(), Error> {
        if matches!(op, Op::Eq | Op::Ne) {
            if left != right
                && matches!(
                    (left, right),
                    (Value::Table(_), Value::Table(_)) | (Value::Userdata(_), Value::Userdata(_))
                )
            {
                let handler = self.shared_handler(left, right, Meta::Eq);
                if handler != Value::Nil {
                    return self.meta_call(
                        handler,
                        &[left, right],
                        result,
                        if op == Op::Eq { CONT_BOOL } else { CONT_NOT },
                    );
                }
            }
            self.registers[result] = Value::Boolean((left == right) == (op == Op::Eq));
            return Ok(());
        }
        if matches!(op, Op::Gt | Op::Ge) {
            std::mem::swap(&mut left, &mut right);
        }
        let equal = matches!(op, Op::Le | Op::Ge);
        let order = match (left, right) {
            (Value::Number(x), Value::Number(y)) => Some(x.partial_cmp(&y)),
            (Value::String(x), Value::String(y)) => Some(Some(crate::locale::compare(
                self.string_bytes(x).unwrap(),
                self.string_bytes(y).unwrap(),
            ))),
            _ => None,
        };
        if let Some(order) = order {
            self.registers[result] = Value::Boolean(
                order == Some(Ordering::Less) || (equal && order == Some(Ordering::Equal)),
            );
            return Ok(());
        }
        if std::mem::discriminant(&left) == std::mem::discriminant(&right)
            || matches!(
                (left, right),
                (Value::Function(_), Value::Native(_)) | (Value::Native(_), Value::Function(_))
            )
        {
            let handler = self.shared_handler(left, right, if equal { Meta::Le } else { Meta::Lt });
            if handler != Value::Nil {
                return self.meta_call(handler, &[left, right], result, CONT_BOOL);
            }
            if equal {
                let handler = self.shared_handler(right, left, Meta::Lt);
                if handler != Value::Nil {
                    return self.meta_call(handler, &[right, left], result, CONT_NOT);
                }
            }
        }
        Err(Error::new(
            ErrorKind::Runtime,
            0,
            if left.type_name().as_bytes()[2] == right.type_name().as_bytes()[2] {
                format!("attempt to compare two {} values", left.type_name())
            } else {
                format!(
                    "attempt to compare {} with {}",
                    left.type_name(),
                    right.type_name()
                )
            },
        ))
    }
}
