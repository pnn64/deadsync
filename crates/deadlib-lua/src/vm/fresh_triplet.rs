//! Inline raw writes for proven, unobserved Lua triplet arguments.

use super::{Error, FunctionId, Op, TableId, Value, Vm, slot_index};
use crate::bytecode::{Instruction, Prototype};

const MAX_FILL_INSTRUCTIONS: usize = 24;
const SITES: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct Pending {
    pub begin: usize,
    pub end: usize,
    pub register: u8,
}

#[derive(Clone, Copy)]
struct Site {
    function: FunctionId,
    pc: usize,
    end: Option<usize>,
}

#[derive(Default)]
pub(super) struct Sites([Option<Site>; SITES]);

impl Sites {
    #[inline]
    pub(super) fn fill_end(
        &mut self,
        function: FunctionId,
        prototype: &Prototype,
        pc: usize,
    ) -> Option<usize> {
        let slot = &mut self.0[pc % SITES];
        if let Some(site) = slot
            && site.function == function
            && site.pc == pc
        {
            return site.end;
        }
        let end = find_fill_end(prototype, pc);
        *slot = Some(Site { function, pc, end });
        end
    }
}

fn fills_temporary(instruction: Instruction, table: u8) -> bool {
    if instruction.op == Op::Load && instruction.b != 0 {
        // A synthetic load records bytecode export operands only.
        return true;
    }
    if instruction.a <= table {
        return false;
    }
    let reads_table = |register| register == u32::from(table);
    let b = u32::from(instruction.b);
    let c = instruction.c;
    match instruction.op {
        Op::Load => b == 0,
        Op::Move => c == 0 && !reads_table(b),
        Op::GetUpvalue => true,
        Op::Neg => !reads_table(b),
        Op::GetTable | Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
            !reads_table(b) && !reads_table(c)
        }
        _ => false,
    }
}

#[cold]
fn find_fill_end(prototype: &Prototype, begin: usize) -> Option<usize> {
    if prototype.debug.native.is_some() {
        return None;
    }
    let code = &prototype.code;
    let table = code[begin - 1].a;
    let mut end = begin;
    while end - begin < MAX_FILL_INSTRUCTIONS && fills_temporary(*code.get(end)?, table) {
        end += 1;
    }
    let set = *code.get(end)?;
    let call = *code.get(end + 1)?;
    if set.op != Op::SetList
        || set.a != table
        || set.b != 3
        || set.c != 1
        || call.op != Op::Call
        || u16::from(call.a) + 3 != u16::from(table)
        || call.b != 3
        || call.c != 0
    {
        return None;
    }
    let after = end + 2;
    let before_map = prototype.metadata.get(after)?.gc_live as u16 as usize;
    let register = usize::from(table);
    if prototype.gc_maps[before_map][register / 64] & (1 << (register % 64)) != 0
        || prototype.debug.locals.iter().any(|local| {
            usize::from(local.register) == register
                && (local.start as usize) <= after
                && (local.end as usize) > begin - 1
        })
    {
        return None;
    }
    Some(end)
}

impl Vm {
    pub(super) fn overwrite_private_triplet_at(
        &mut self,
        root: TableId,
        index: usize,
        values: [f64; 3],
    ) -> bool {
        let Some(&Value::Table(row)) = self.tables[slot_index(root.0)].array_values().get(index)
        else {
            return false;
        };
        self.tables[slot_index(row.0)].overwrite_private_triplet(values)
    }

    #[cold]
    pub(super) fn write_inlined_triplet(&mut self, slot: usize) -> Result<bool, Error> {
        let call = slot - 3;
        if self.frames.len() >= self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
            || self.registers.len() == self.registers.capacity()
        {
            return Ok(false);
        }
        let [Value::Number(x), Value::Number(y), Value::Number(z)] =
            self.registers[slot + 1..slot + 4]
        else {
            return Ok(false);
        };
        let Some((target, index, values, private)) =
            self.resolve_triplet_write(self.registers[call], self.registers[slot - 1], [x, y, z])
        else {
            return Ok(false);
        };
        let Some(index) = index.checked_sub(1).map(|index| index as usize) else {
            return Ok(false);
        };
        if self.validate_value(Value::Table(target)).is_err()
            || !self.tables[slot_index(target.0)].can_replace_array_value(index)
        {
            return Ok(false);
        }
        if private && self.overwrite_private_triplet_at(target, index, values) {
            return Ok(true);
        }
        let array = self.try_allocate_dense_table(3)?;
        self.tables[slot_index(array.0)].initialize_number_array(&values);
        let replaced =
            self.tables[slot_index(target.0)].replace_array_value(index, Value::Table(array));
        debug_assert!(replaced);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Error, HostCall, IndexView, TableId, Value, Vm, compile};

    fn put(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Table(points) = call.captures()[0] else {
            unreachable!()
        };
        let [_, Value::Number(index), Value::Table(input)] = *call.arguments() else {
            unreachable!()
        };
        let [Value::Number(x), Value::Number(y), Value::Number(z)] =
            call.raw_table_read_array(input)?
        else {
            unreachable!()
        };
        *call.host_state_mut::<usize>().unwrap() += 1;
        call.raw_table_set_number_array(points, index as u32, &[x + 1., y, z])
    }

    fn resolve(
        view: &IndexView<'_>,
        index: Value,
        [x, y, z]: [f64; 3],
    ) -> Option<(TableId, u32, [f64; 3])> {
        let Value::Table(points) = view.captures()[0] else {
            return None;
        };
        let Value::Number(index) = index else {
            return None;
        };
        Some((points, index as u32, [x + 1., y, z]))
    }

    #[test]
    fn completes_plain_literals_and_runs_observable_calls_normally() {
        let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
        vm.execute(&compile(b"points = {false,false,false,false}; for i = 1,100 do local row = {} end; collectgarbage('collect'); collectgarbage('stop')").unwrap()).unwrap();
        vm.set_host_state(Box::new(0usize));
        let points = vm.get_global(b"points").unwrap();
        let put = vm.new_host_function(put, &[points]).unwrap();
        vm.set_triplet_resolver(put, resolve).unwrap();
        vm.set_global(b"put", put).unwrap();
        let program = compile(
            br#"
            local named, x = {1, 2, 3}, 2
            put(points, 1, {x * 2, 2, 3})
            put(points, 2, named)
            assert(named[1] == 1 and points[2] ~= named)
            local proxy = setmetatable({}, {__index = function() return 6 end})
            put(points, 3, {proxy.x, 2, 3})
            debug.sethook(function() end, 'l')
            put(points, 4, {8, 2, 3})
            debug.sethook()
            return points[1][1], points[2][1], points[3][1], points[4][1]
        "#,
        )
        .unwrap();
        assert_eq!(
            vm.execute(&program).unwrap(),
            &[5., 2., 7., 9.].map(Value::Number)
        );
        assert_eq!(vm.host_state::<usize>(), Some(&3));
    }
}
