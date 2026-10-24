//! Constant arenas for byte cells shared across independently compiled programs.
use super::*;
use std::{collections::HashMap, ops::Deref};

pub(super) enum Strings {
    Unique(Box<[Box<[u8]>]>),
    Shared(Arc<[Box<[u8]>]>),
}
impl Deref for Strings {
    type Target = [Box<[u8]>];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Unique(values) => values,
            Self::Shared(values) => values,
        }
    }
}
impl Strings {
    pub(super) fn retained_bytes(&self) -> usize {
        size_of_val(&**self) + self.iter().map(|s| s.len()).sum::<usize>()
    }
    pub(super) fn shared_with(&self, other: &Self) -> bool {
        matches!((self, other), (Self::Shared(a), Self::Shared(b)) if Arc::ptr_eq(a,b))
    }
}

pub(super) fn same_cells(a: &Code, b: &Code) -> bool {
    a.cells.len() == b.cells.len()
        && a.cells
            .iter()
            .zip(&b.cells)
            .all(|(a_value, b_value)| match (*a_value, *b_value) {
                (Value::String(x), Value::String(y)) => a.strings[x] == b.strings[y],
                _ => a_value == b_value,
            })
}

// A single arena gives every stored string ID the same meaning in every code
// block. Byte equality deduplicates immutable data; cell indices still retain
// the host's declared storage identities. Ordinary bool/number groups bypass
// this construction entirely.
pub(super) fn share(mut programs: Vec<Program>) -> Vec<Program> {
    let mut arena = Vec::new();
    let mut ids = HashMap::<&[u8], usize>::new();
    let mut mappings = Vec::with_capacity(programs.len());
    for program in &programs {
        let mut mapping = Vec::with_capacity(program.0.strings.len());
        for bytes in program.0.strings.iter() {
            let id = if let Some(&id) = ids.get(bytes.as_ref()) {
                id
            } else {
                let id = arena.len();
                ids.insert(bytes.as_ref(), id);
                arena.push(bytes.clone());
                id
            };
            mapping.push(id);
        }
        mappings.push(mapping);
    }
    drop(ids);
    let arena: Arc<[Box<[u8]>]> = arena.into();
    let mut rewritten = Vec::<(usize, Program)>::new();
    for (program, mapping) in programs.iter_mut().zip(mappings) {
        // Keys are identities of still-owned input Arcs, never dereferenced.
        let identity = Arc::as_ptr(&program.0) as usize;
        if let Some((_, existing)) = rewritten.iter().find(|(id, _)| *id == identity) {
            *program = existing.clone();
            continue;
        }
        if Arc::get_mut(&mut program.0).is_none() {
            let code = &program.0;
            program.0 = Arc::new(Code {
                ops: code.ops.clone(),
                cells: code.cells.clone(),
                parameters: code.parameters.clone(),
                registers: code.registers,
                strings: Strings::Shared(arena.clone()),
            });
        }
        let code = Arc::get_mut(&mut program.0).expect("unique rewritten code");
        fn value(v: &mut Value, mapping: &[usize]) {
            if let Value::String(id) = v {
                *id = mapping[*id];
            }
        }
        fn source(s: &mut Source, mapping: &[usize]) {
            if let Source::Value(v) = s {
                value(v, mapping);
            }
        }
        fn operand(o: &mut Operand, mapping: &[usize]) {
            if let Operand::Value(v) = o {
                value(v, mapping);
            }
        }
        for v in &mut code.cells {
            value(v, &mapping);
        }
        for s in &mut code.parameters {
            source(s, &mapping);
        }
        for op in &mut code.ops {
            match op {
                Op::Load { source: s, .. } => source(s, &mapping),
                Op::Write { source: s, .. } => operand(s, &mapping),
                Op::Arithmetic { left, right, .. } | Op::Compare { left, right, .. } => {
                    operand(left, &mapping);
                    operand(right, &mapping);
                }
                Op::Call { result, .. } => value(result, &mapping),
                _ => {}
            }
        }
        code.strings = Strings::Shared(arena.clone());
        rewritten.push((identity, program.clone()));
    }
    programs
}
