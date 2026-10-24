//! Share immutable code without merging callback slots or mutable state.
use super::*;
use rustc_hash::{FxHashMap, FxHasher};
use std::hash::{Hash, Hasher};

fn string_hash(strings: &Strings) -> u64 {
    let mut hash = FxHasher::default();
    strings[..].hash(&mut hash);
    hash.finish()
}

fn fingerprint(code: &Code, strings: u64) -> u64 {
    let mut hash = FxHasher::default();
    code.ops.hash(&mut hash);
    code.parameters.hash(&mut hash);
    code.cells.hash(&mut hash);
    code.registers.hash(&mut hash);
    code.clear_registers.hash(&mut hash);
    strings.hash(&mut hash);
    hash.finish()
}

fn same(a: &Code, b: &Code) -> bool {
    a.registers == b.registers
        && a.clear_registers == b.clear_registers
        && a.ops == b.ops
        && a.parameters == b.parameters
        && a.cells == b.cells
        && a.strings[..] == b.strings[..]
}

pub(super) fn code(programs: &mut [Program]) {
    let first = &programs[0].0;
    if programs.iter().all(|p| Arc::ptr_eq(first, &p.0)) {
        return;
    }
    // Normalized byte-cell programs share one arena. Hash its contents once.
    let shared = programs
        .iter()
        .all(|p| first.strings.shared_with(&p.0.strings))
        .then(|| string_hash(&first.strings));
    let mut seen = FxHashMap::<u64, usize>::with_capacity_and_hasher(programs.len(), Default::default());
    for index in 0..programs.len() {
        let hash = fingerprint(
            &programs[index].0,
            shared.unwrap_or_else(|| string_hash(&programs[index].0.strings)),
        );
        if let Some(&existing) = seen.get(&hash) {
            // A fingerprint is only a candidate. Exact value/bit equality is
            // required, and a collision may safely leave other code unshared.
            if same(&programs[existing].0, &programs[index].0) {
                programs[index] = programs[existing].clone();
            }
        } else {
            seen.insert(hash, index);
        }
    }
}
