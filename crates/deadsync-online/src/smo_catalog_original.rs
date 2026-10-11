// Catalogue loop frozen from 4ca2c55fba8006d83b9ea427b1922a4dc6448b31; Accumulator::add is unchanged.
use super::*;

pub(super) fn from_catalog(catalog: &[stepmaniaonline::PackInfo], needle: &str) -> Accumulator {
    let mut acc = Accumulator::default();
    for pack in catalog.iter() {
        let name = pack.name.to_lowercase();
        let points = if name.starts_with(needle) {
            score::NAME_PREFIX
        } else if name.contains(needle) {
            score::NAME_MATCH
        } else {
            continue;
        };
        acc.add(pack.id, points, "pack name".to_owned());
    }
    acc
}
