// Additional actual-library checks that force the indexed suffix path.
use deadsync_profile::folder_name_for_display;
mod original {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/folder_paths_original.rs"
    ));
}
fn check(base: &str, names: &[String], suffix: u32) {
    let before = original::folder_name_for_display(base, "fallback", names);
    let after = folder_name_for_display(base, "fallback", names);
    assert_eq!(before, after);
    assert_eq!(before.capacity(), after.capacity());
    assert_eq!(after, format!("{base}-{suffix}"));
}
#[test]
fn indexed_parser_rejects_noncanonical_suffixes_and_prefix_aliases() {
    for base in ["Player", "é東", "A-B"] {
        let mut names: Vec<_> = (1..=8)
            .map(|n| {
                if n == 1 {
                    base.to_string()
                } else {
                    format!("{base}-{n}")
                }
            })
            .collect();
        names.extend((0..64).map(|n| format!("Other {n}")));
        let upper = base.to_ascii_uppercase();
        for tail in [
            "09", "009", "+9", "9 ", " 9", "9x", "91x", "9x1", "-9", "0", "1", "10000", "99999",
            "٢", "９", "9\0", "9.0", "",
        ] {
            names.push(format!("{upper}-{tail}"));
        }
        names.push(format!("{base}x-9"));
        names.push(format!("{base}--9"));
        names.push("aaaaé-9".to_string());
        names.push(format!("{upper}-9999"));
        check(base, &names, 9);
        names.push(format!("{upper}-9"));
        check(base, &names, 10);
        for tail in ["010", "+10", "10 ", "10x"] {
            names.push(format!("{upper}-{tail}"));
        }
        check(base, &names, 10);
        names.push(format!("{upper}-10"));
        check(base, &names, 11);
    }
}
