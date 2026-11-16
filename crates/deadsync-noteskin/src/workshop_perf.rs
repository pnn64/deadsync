//! Workshop manifest parity, including ordering, Unicode and error paths.
use super::*;
use crate::perf as allocations;


fn fixture(groups: usize, files_per_group: usize) -> Vec<PathBuf> {
    (0..groups).flat_map(|group| (0..files_per_group).map(move |file| {
        let category = ["Arrows", "Receptors", "Holds", "Rolls", "Tap Explosions", "Hold Explosions", "Mines", "Mine Size", "Lifts"][group % 9];
        let label = if group % 7 == 0 { "RGB" } else { "DDR Vivid \u{66f2}" };
        let state = if file % 2 == 0 { "Inactive" } else { "Active" };
        Path::new("fixture").join(format!("Customizations/{category}/Cel - {label} {group:04}/Subfolder/tap {file} {state}.png"))
    })).collect()
}


#[test]
fn rejected_workshop_family_does_not_allocate_normalized_paths() {
    let files: Vec<_> = fixture(90, 8)
        .into_iter()
        .map(|p| PathBuf::from(p.to_str().unwrap().replace("Cel", "Metal")))
        .collect();
    let (result, after) = allocations::measure(|| choices_for(&files, Path::new("fixture"), "Cel"));
    assert!(result.unwrap().is_empty());
    assert_eq!(after.allocs, 0);
    assert_eq!(after.reallocs, 0);
}
