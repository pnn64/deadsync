//! Structural checking of all 38 Lua 5.1 opcodes without executing a chunk.
use super::{Chunk, Constant};

// Independent format/operand descriptors: ABC, ABx, AsBx; none, unsigned,
// register, register-or-constant. Test instructions consume a following jump.
const MODES: [(u8, u8, u8, bool); 38] = [
    (0, 2, 0, false),
    (1, 3, 0, false),
    (0, 1, 1, false),
    (0, 2, 0, false),
    (0, 1, 0, false),
    (1, 3, 0, false),
    (0, 2, 3, false),
    (1, 3, 0, false),
    (0, 1, 0, false),
    (0, 3, 3, false),
    (0, 1, 1, false),
    (0, 2, 3, false),
    (0, 3, 3, false),
    (0, 3, 3, false),
    (0, 3, 3, false),
    (0, 3, 3, false),
    (0, 3, 3, false),
    (0, 3, 3, false),
    (0, 2, 0, false),
    (0, 2, 0, false),
    (0, 2, 0, false),
    (0, 2, 2, false),
    (2, 2, 0, false),
    (0, 3, 3, true),
    (0, 3, 3, true),
    (0, 3, 3, true),
    (0, 2, 1, true),
    (0, 2, 1, true),
    (0, 1, 1, false),
    (0, 1, 1, false),
    (0, 1, 0, false),
    (2, 2, 0, false),
    (2, 2, 0, false),
    (0, 0, 1, true),
    (0, 1, 1, false),
    (0, 0, 0, false),
    (1, 1, 0, false),
    (0, 1, 0, false),
];
fn op(word: u32) -> usize {
    (word & 63) as usize
}
fn b(word: u32) -> usize {
    (word >> 23) as usize
}
fn c(word: u32) -> usize {
    ((word >> 14) & 511) as usize
}
fn open(word: u32) -> bool {
    matches!(op(word), 28 | 29 | 30 | 34) && b(word) == 0
}

pub(super) fn verify(chunk: &Chunk, id: usize) -> Result<(), &'static str> {
    let p = &chunk.prototypes[id];
    let fail = || Err("bad code");
    let n = p.code.len();
    let stack = usize::from(p.stack);
    if stack == 0
        || stack > 250
        || usize::from(p.params) + usize::from(p.vararg & 1) > stack
        || (p.vararg & 4 != 0 && p.vararg & 1 == 0)
        || p.names.len() > usize::from(p.nups)
        || (!p.lines.is_empty() && p.lines.len() != n)
        || p.code.last().is_none_or(|&w| op(w) != 30)
    {
        return fail();
    }
    // A single linear pass identifies SETLIST payloads. Their raw count can
    // itself resemble SETLIST; treating it as another opcode would be wrong.
    let mut payload = vec![false; n];
    let mut pc = 0;
    while pc < n {
        let word = p.code[pc];
        if op(word) == 34 && c(word) == 0 {
            if pc + 1 >= n - 1 {
                return fail();
            }
            payload[pc + 1] = true;
            pc += 1;
        }
        pc += 1;
    }
    let reg = |r: usize| r < stack;
    let arg = |r: usize, mode| match mode {
        0 => r == 0,
        1 => true,
        2 => reg(r),
        _ => {
            if r & 256 != 0 {
                r & 255 < p.constants.len()
            } else {
                reg(r)
            }
        }
    };
    for (pc, &word) in p.code.iter().enumerate() {
        if payload[pc] {
            continue;
        }
        let opcode = op(word);
        let Some(&(mode, bm, cm, test)) = MODES.get(opcode) else {
            return fail();
        };
        let a = ((word >> 6) & 255) as usize;
        let (b, c) = (b(word), c(word));
        let bx = (word >> 14) as usize;
        if !reg(a) {
            return fail();
        }
        match mode {
            0 if !arg(b, bm) || !arg(c, cm) => return fail(),
            1 if bm == 3 && bx >= p.constants.len() => return fail(),
            2 => {
                let dest = pc as i64 + 1 + bx as i64 - 131071;
                if dest < 0 || dest >= n as i64 || payload[dest as usize] {
                    return fail();
                }
            }
            _ => {}
        }
        if test && (pc + 2 >= n || op(p.code[pc + 1]) != 22) {
            return fail();
        }
        match opcode {
            2 if b > 1 || c > 1 || (c != 0 && (pc + 2 >= n || payload[pc + 2])) => return fail(),
            3 if a > b => return fail(),
            4 | 8 if b >= usize::from(p.nups) => return fail(),
            5 | 7 if !matches!(p.constants[bx], Constant::String(_)) => return fail(),
            11 if !reg(a + 1) => return fail(),
            21 if b >= c => return fail(),
            23..=25 if a > 1 => return fail(),
            26 | 27 if c > 1 => return fail(),
            28 | 29 => {
                if (b != 0 && !reg(a + b - 1))
                    || (c > 1 && !reg(a + c - 2))
                    || (c == 0 && !p.code.get(pc + 1).is_some_and(|&w| open(w)))
                {
                    return fail();
                }
            }
            30 if b > 1 && !reg(a + b - 2) => return fail(),
            31 | 32 if !reg(a + 3) => return fail(),
            33 if c == 0 || !reg(a + 2 + c) => return fail(),
            34 if b != 0 && !reg(a + b) => return fail(),
            36 => {
                let Some(&child) = p.children.get(bx) else {
                    return fail();
                };
                let nups = usize::from(chunk.prototypes[child].nups);
                if pc + nups >= n {
                    return fail();
                }
                for &binding in &p.code[pc + 1..pc + 1 + nups] {
                    if !matches!(op(binding), 0 | 4) {
                        return fail();
                    }
                }
            }
            37 if p.vararg & 2 == 0
                || p.vararg & 4 != 0
                || (b > 1 && !reg(a + b - 2))
                || (b == 0 && !p.code.get(pc + 1).is_some_and(|&w| open(w))) =>
            {
                return fail();
            }
            _ => {}
        }
    }
    Ok(())
}
