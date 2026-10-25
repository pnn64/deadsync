//! Raw table operations and resumable traversal and sorting callbacks.
use super::*;

pub(super) const TAB_FIRST: u64 = 96;
pub(super) const TAB_CONCAT: u64 = 96;
pub(super) const TAB_FOREACH: u64 = 97;
pub(super) const TAB_FOREACHI: u64 = 98;
pub(super) const TAB_GETN: u64 = 99;
pub(super) const TAB_MAXN: u64 = 100;
pub(super) const TAB_INSERT: u64 = 101;
pub(super) const TAB_REMOVE: u64 = 102;
pub(super) const TAB_SETN: u64 = 103;
pub(super) const TAB_SORT: u64 = 104;

#[derive(Clone, Copy, Debug)]
enum SortStage {
    Next,
    Edge,
    MiddleLow,
    MiddleHigh,
    Left,
    Right,
}

#[derive(Debug)]
pub(super) struct Sort {
    pub owner: usize,
    table: TableId,
    comparator: Value,
    ranges: [(i64, i64); 32],
    range_count: usize,
    stage: SortStage,
    low: i64,
    high: i64,
    i: i64,
    j: i64,
    pivot: Value,
    saved: [Value; 2],
}
impl Sort {
    pub(super) fn roots(&self) -> [Value; 5] {
        [
            Value::Table(self.table),
            self.comparator,
            self.pivot,
            self.saved[0],
            self.saved[1],
        ]
    }
}

fn failure(message: impl Into<Box<str>>) -> Error {
    Error::new(ErrorKind::Runtime, 0, message)
}
fn key(index: i64) -> Value {
    Value::Number(index as f64)
}

impl Vm {
    pub(super) fn open_tables(&mut self) {
        let table = self.new_table();
        for (i, name) in [
            b"concat".as_slice(),
            b"foreach",
            b"foreachi",
            b"getn",
            b"maxn",
            b"insert",
            b"remove",
            b"setn",
            b"sort",
        ]
        .iter()
        .enumerate()
        {
            let name = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(name, Value::Native(TAB_FIRST + i as u64))
                .expect("table names are valid");
        }
        self.set_global(b"table", Value::Table(table))
            .expect("table name is valid");
    }
    pub(super) fn table_argument(&self, base: usize, count: usize) -> Result<TableId, Error> {
        if let Value::Table(table) = self.argument(base, count, 0) {
            Ok(table)
        } else {
            Err(self.argument_type_error(base, count, 0, "table"))
        }
    }
    fn raw_index(&self, table: TableId, index: i64) -> Value {
        self.tables[slot_index(table.0)].get(key(index))
    }
    fn set_index(&mut self, table: TableId, index: i64, value: Value) {
        self.raw_set(table, key(index), value)
            .expect("finite integer is a valid key");
    }
    pub(super) fn table_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        // concat checks its separator before checking the table, as Lua 5.1 does.
        let separator = if id == TAB_CONCAT {
            Some(if self.argument(base, count, 1) == Value::Nil {
                self.strings.intern(b"")
            } else {
                self.string_argument(base, count, 1)?
            })
        } else {
            None
        };
        let table = self.table_argument(base, count)?;
        let len = self.tables[slot_index(table.0)].len() as i64;
        let value = match id {
            TAB_GETN => Value::Number(len as f64),
            TAB_SETN => return Err(failure("'setn' is obsolete")),
            TAB_MAXN => {
                let mut maximum = 0.0f64;
                let mut current = Value::Nil;
                while let Some((k, _)) = self.tables[slot_index(table.0)].next(current)? {
                    if let Value::Number(n) = k {
                        maximum = maximum.max(n);
                    }
                    current = k;
                }
                Value::Number(maximum)
            }
            TAB_INSERT => {
                let position = match count {
                    2 => len + 1,
                    3 => self.integer_argument(base, count, 1, None)? as i32 as i64,
                    _ => return Err(failure("wrong number of arguments to 'insert'")),
                };
                let end = (len + 1).max(position);
                if end.saturating_sub(position) > 10_000_000 {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        0,
                        "table operation limit exceeded",
                    ));
                }
                for index in (position + 1..=end).rev() {
                    let v = self.raw_index(table, index - 1);
                    self.set_index(table, index, v);
                }
                let v = self.argument(base, count, count - 1);
                self.set_index(table, position, v);
                return Ok(0);
            }
            TAB_REMOVE => {
                let position = self.integer_argument(base, count, 1, Some(len))? as i32 as i64;
                if position < 1 || position > len {
                    return Ok(0);
                }
                let value = self.raw_index(table, position);
                for index in position..len {
                    let v = self.raw_index(table, index + 1);
                    self.set_index(table, index, v);
                }
                self.set_index(table, len, Value::Nil);
                value
            }
            TAB_CONCAT => {
                let first = self.integer_argument(base, count, 2, Some(1))? as i32 as i64;
                let last = self.integer_argument(base, count, 3, Some(len))? as i32 as i64;
                let separator = separator.expect("concat has a separator");
                let mut output = self.take_buffer(0);
                if last.saturating_sub(first) > 10_000_000 {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        0,
                        "table operation limit exceeded",
                    ));
                }
                for index in first..=last {
                    if index != first {
                        output.extend_from_slice(&self.strings.bytes[slot_index(separator.0)]);
                    }
                    let v = self.raw_index(table, index);
                    match v {
                        Value::String(id) => {
                            output.extend_from_slice(&self.strings.bytes[slot_index(id.0)])
                        }
                        Value::Number(n) => output.extend_from_slice(format_number(n).as_bytes()),
                        _ => {
                            return Err(failure(format!(
                                "invalid value ({}) at index {index} in table for 'concat'",
                                v.type_name()
                            )));
                        }
                    }
                }
                self.finish_buffer(output)
            }
            _ => unreachable!("resumable table function dispatched separately"),
        };
        self.registers[base] = value;
        Ok(1)
    }
    pub(super) fn foreach_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        let args = frame.vararg_start - 1;
        let table = self.table_argument(args, frame.argc())?;
        let callback = self.argument(args, frame.argc(), 1);
        if frame.pc() == 0 {
            if !matches!(callback, Value::Function(_) | Value::Native(_)) {
                return Err(self.argument_type_error(args, frame.argc(), 1, "function"));
            }
            self.registers[base + 3] = if id == TAB_FOREACHI {
                key(0)
            } else {
                Value::Nil
            };
            self.registers[base + 4] = key(self.tables[slot_index(table.0)].len() as i64);
            self.frames[owner].pc = 1;
        } else if self.registers[base] != Value::Nil {
            return Ok(self.return_frame(frame, base, 1));
        }
        let pair = if id == TAB_FOREACHI {
            let Value::Number(index) = self.registers[base + 3] else {
                unreachable!("foreachi index is numeric")
            };
            let Value::Number(limit) = self.registers[base + 4] else {
                unreachable!("foreachi bound is numeric")
            };
            if index >= limit {
                None
            } else {
                Some((
                    Value::Number(index + 1.0),
                    self.raw_index(table, index as i64 + 1),
                ))
            }
        } else {
            self.tables[slot_index(table.0)].next(self.registers[base + 3])?
        };
        let Some((k, v)) = pair else {
            return Ok(self.return_frame(frame, base, 0));
        };
        self.registers[base + 3] = k;
        self.registers[base] = callback;
        self.registers[base + 1] = k;
        self.registers[base + 2] = v;
        if let Some(actual) = self.start_call(base, 2, 1, false)? {
            self.finish_call(base, actual, 1);
        }
        Ok(None)
    }
    pub(super) fn trim_sorts(&mut self) {
        while self
            .sorts
            .last()
            .is_some_and(|s| s.owner >= self.frames.len())
        {
            self.sorts.pop();
        }
    }
    fn sort_compare(
        &mut self,
        index: usize,
        left: Value,
        right: Value,
        stage: SortStage,
        base: usize,
    ) -> Result<Option<usize>, Error> {
        self.sorts[index].stage = stage;
        let comparator = self.sorts[index].comparator;
        if comparator == Value::Nil {
            self.compare(Op::Lt, left, right, base)?;
        } else {
            self.registers[base] = comparator;
            self.registers[base + 1] = left;
            self.registers[base + 2] = right;
            if let Some(actual) = self.start_call(base, 2, 1, false)? {
                self.finish_call(base, actual, 1);
            }
        }
        Ok(None)
    }
    fn sort_partition(&mut self, index: usize) {
        let sort = &self.sorts[index];
        let (table, low, high) = (sort.table, sort.low, sort.high);
        if high - low == 2 {
            self.sorts[index].stage = SortStage::Next;
            return;
        }
        let mid = (low + high) / 2;
        let pivot = self.raw_index(table, mid);
        let other = self.raw_index(table, high - 1);
        self.set_index(table, mid, other);
        self.set_index(table, high - 1, pivot);
        let sort = &mut self.sorts[index];
        sort.pivot = pivot;
        sort.i = low;
        sort.j = high - 1;
    }
    fn sort_left(&mut self, index: usize, base: usize) -> Result<Option<usize>, Error> {
        self.sorts[index].i += 1;
        let sort = &self.sorts[index];
        let (table, i, pivot) = (sort.table, sort.i, sort.pivot);
        let value = self.raw_index(table, i);
        self.sorts[index].saved[0] = value;
        self.sort_compare(index, value, pivot, SortStage::Left, base)
    }
    fn sort_right(&mut self, index: usize, base: usize) -> Result<Option<usize>, Error> {
        self.sorts[index].j -= 1;
        let sort = &self.sorts[index];
        let (table, j, pivot) = (sort.table, sort.j, sort.pivot);
        let value = self.raw_index(table, j);
        self.sorts[index].saved[1] = value;
        self.sort_compare(index, pivot, value, SortStage::Right, base)
    }
    pub(super) fn sort_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let index = if frame.pc() == 0 {
            let args = frame.vararg_start - 1;
            let count = frame.argc();
            let table = self.table_argument(args, count)?;
            let comparator = self.argument(args, count, 1);
            if !matches!(
                comparator,
                Value::Nil | Value::Function(_) | Value::Native(_)
            ) {
                return Err(self.argument_type_error(args, count, 1, "function"));
            }
            let len = self.tables[slot_index(table.0)].len() as i64;
            let index = self.sorts.len();
            let mut ranges = [(0, 0); 32];
            ranges[0] = (1, len);
            self.sorts.push(Sort {
                owner,
                table,
                comparator,
                ranges,
                range_count: 1,
                stage: SortStage::Next,
                low: 1,
                high: len,
                i: 0,
                j: 0,
                pivot: Value::Nil,
                saved: [Value::Nil; 2],
            });
            self.frames[owner].pc = (index + 1) as u32;
            index
        } else {
            frame.pc() - 1
        };
        loop {
            let sort = &self.sorts[index];
            let (table, low, high, i, j) = (sort.table, sort.low, sort.high, sort.i, sort.j);
            let saved = sort.saved;
            let result = self.registers[base].is_truthy();
            match sort.stage {
                SortStage::Next => {
                    if self.sorts[index].range_count == 0 {
                        self.sorts.pop();
                        return Ok(self.return_frame(frame, base, 0));
                    }
                    self.sorts[index].range_count -= 1;
                    let (low, high) = self.sorts[index].ranges[self.sorts[index].range_count];
                    if low >= high {
                        continue;
                    }
                    let a = self.raw_index(table, low);
                    let b = self.raw_index(table, high);
                    let sort = &mut self.sorts[index];
                    sort.low = low;
                    sort.high = high;
                    sort.saved = [a, b];
                    return self.sort_compare(index, b, a, SortStage::Edge, base);
                }
                SortStage::Edge => {
                    if result {
                        self.set_index(table, low, saved[1]);
                        self.set_index(table, high, saved[0]);
                    }
                    if high - low == 1 {
                        self.sorts[index].stage = SortStage::Next;
                        continue;
                    }
                    let a = self.raw_index(table, (low + high) / 2);
                    let b = self.raw_index(table, low);
                    self.sorts[index].saved = [a, b];
                    return self.sort_compare(index, a, b, SortStage::MiddleLow, base);
                }
                SortStage::MiddleLow => {
                    if result {
                        self.set_index(table, (low + high) / 2, saved[1]);
                        self.set_index(table, low, saved[0]);
                    } else {
                        let b = self.raw_index(table, high);
                        self.sorts[index].saved[1] = b;
                        return self.sort_compare(index, b, saved[0], SortStage::MiddleHigh, base);
                    }
                    self.sort_partition(index);
                    if high - low == 2 {
                        continue;
                    }
                    return self.sort_left(index, base);
                }
                SortStage::MiddleHigh => {
                    if result {
                        self.set_index(table, (low + high) / 2, saved[1]);
                        self.set_index(table, high, saved[0]);
                    }
                    self.sort_partition(index);
                    if high - low == 2 {
                        continue;
                    }
                    return self.sort_left(index, base);
                }
                SortStage::Left => {
                    if result {
                        if i > high {
                            return Err(failure("invalid order function for sorting"));
                        }
                        return self.sort_left(index, base);
                    }
                    return self.sort_right(index, base);
                }
                SortStage::Right => {
                    if result {
                        if j < low {
                            return Err(failure("invalid order function for sorting"));
                        }
                        return self.sort_right(index, base);
                    }
                    if j >= i {
                        self.set_index(table, i, saved[1]);
                        self.set_index(table, j, saved[0]);
                        return self.sort_left(index, base);
                    }
                    let a = self.raw_index(table, high - 1);
                    let b = self.raw_index(table, i);
                    self.set_index(table, high - 1, b);
                    self.set_index(table, i, a);
                    let sort = &mut self.sorts[index];
                    let left = (low, i - 1);
                    let right = (i + 1, high);
                    let count = sort.range_count;
                    if count + 2 > sort.ranges.len() {
                        return Err(Error::new(
                            ErrorKind::Limit,
                            0,
                            "sort partition limit exceeded",
                        ));
                    }
                    if i - low < high - i {
                        sort.ranges[count..count + 2].copy_from_slice(&[right, left]);
                    } else {
                        sort.ranges[count..count + 2].copy_from_slice(&[left, right]);
                    }
                    sort.range_count += 2;
                    sort.stage = SortStage::Next;
                }
            }
        }
    }
}
