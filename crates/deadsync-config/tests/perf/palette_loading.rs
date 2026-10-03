use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/palette_loading/baseline.rs"
    ));
}

fn fixture(count: usize) -> String {
    let mut text = String::from("[General]\nDefaultPalette=custom-0\n");
    for i in 0..count {
        writeln!(
            text,
            "\n[Palette custom-{i}]\nName=Palette {i}\nOrder={}",
            count - i
        )
        .unwrap();
        for role in JudgmentColorRole::ALL {
            writeln!(text, "{}=#123456", role.config_key()).unwrap();
        }
    }
    text
}

#[test]
fn palette_loader_preserves_duplicate_sections_defaults_colors_and_order() {
    for text in [
        "",
        "key=value\n",
        "[General]\nDefaultPalette=missing\n",
        "[Palette TEST-THEME]\nName=Override\nMiss=#000000\n",
    ] {
        assert_eq!(
            JudgmentPaletteCatalog::from_ini(text, PRESET),
            baseline::from_ini(text, PRESET)
        );
    }
    for count in [1, 2, 16, 128] {
        for name in [
            "",
            " Normal ",
            "日本語É",
            "bad=name",
            "bad[name",
            "12345678901234567890123456789012345",
        ] {
            for order in ["", "0", "999999999999999999999999", "bad"] {
                let mut text = fixture(count);
                writeln!(text, "\n[Palette custom-0]\nName={name}\nOrder={order}\nMiss=invalid\nMiss=#80000000\n[Palette  custom-0 ]\nName=Duplicate ID\nOrder=0\n[Palette ]\nName=Empty ID\n[Unrelated]\nName=ignored\n[General]\nDefaultPalette=custom-0\n").unwrap();
                let old = baseline::from_ini(&text, PRESET);
                let new = JudgmentPaletteCatalog::from_ini(&text, PRESET);
                assert_eq!(old, new, "count={count}, name={name:?}, order={order:?}");
                assert_eq!(old.to_ini(), new.to_ini());
                assert_eq!(new.resolve(None), old.resolve(None));
            }
        }
    }
}

#[test]
fn palette_loader_keeps_only_retained_strings_owned() {
    for count in [1, 16, 128] {
        let text = fixture(count);
        assert_reduced_churn(
            || drop(black_box(baseline::from_ini(black_box(&text), PRESET))),
            || {
                drop(black_box(JudgmentPaletteCatalog::from_ini(
                    black_box(&text),
                    PRESET,
                )))
            },
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn palette_loading_benchmark() {
    let original =
        black_box(baseline::from_ini as fn(&str, JudgmentPalettePreset) -> JudgmentPaletteCatalog);
    let current = black_box(
        JudgmentPaletteCatalog::from_ini
            as fn(&str, JudgmentPalettePreset) -> JudgmentPaletteCatalog,
    );
    let mut unrelated = fixture(1);
    for i in 0..128 {
        writeln!(
            unrelated,
            "[Other {i}]\nKey1=unused value\nKey2=unused value\nKey3=unused value\n"
        )
        .unwrap();
    }
    for (name, text) in [
        ("empty", String::new()),
        ("one", fixture(1)),
        ("sixteen", fixture(16)),
        ("many", fixture(128)),
        ("unrelated", unrelated),
    ] {
        let run = |variant, f: fn(&str, JudgmentPalettePreset) -> JudgmentPaletteCatalog| {
            measure_sampled(&format!("palette-loading/{name}/{variant}"), 256, 1, || {
                f(black_box(&text), black_box(PRESET))
            });
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
}
