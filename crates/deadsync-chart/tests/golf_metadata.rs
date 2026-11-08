use deadsync_chart::golf::GolfPars;
use std::fmt::Write;
use std::hint::black_box;
use std::path::Path;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/golf_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

const DIFFICULTIES: [&str; 6] = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];

fn fixture(songs: usize, overrides: usize) -> (String, Vec<String>) {
    let mut text = String::new();
    let names: Vec<_> = (0..songs)
        .map(|i| format!("Song [{i:04}] - \u{96ea}:Folder"))
        .collect();
    for (i, name) in names.iter().enumerate() {
        writeln!(text, "[{name}]\nPar={}.5", i % 40).unwrap();
        for (index, difficulty) in DIFFICULTIES.iter().enumerate().take(overrides) {
            writeln!(text, "[{name}:{difficulty}]\nPar={}.25", (i + index) % 40).unwrap();
        }
    }
    (text, names)
}

fn check(text: &str, names: &[String], difficulties: &[String]) {
    let old = original::GolfPars::parse(text);
    let current = GolfPars::parse(text);
    for name in names {
        for difficulty in difficulties {
            assert_eq!(
                current.get(name, difficulty).map(f64::to_bits),
                old.get(name, difficulty).map(f64::to_bits),
                "{name:?}, {difficulty:?}"
            );
        }
    }
}

#[test]
fn golf_metadata_retains_overrides_defaults_and_invalid_section_semantics() {
    let mut difficulties: Vec<_> = DIFFICULTIES
        .iter()
        .flat_map(|name| {
            [
                name.to_string(),
                name.to_ascii_lowercase(),
                name.to_ascii_uppercase(),
            ]
        })
        .collect();
    difficulties.extend(["", "unknown", " Hard", "Edit ", "\u{96ea}", "Hard\0"].map(String::from));
    for overrides in 0..=6 {
        let (mut text, mut names) = fixture(64, overrides);
        text.push_str("\u{feff}; comment\r\n[Other]\r\nPar=0\r\nPar=-0\r\n[Other:Hard]\r\nPar=1.25\r\nPar=2.5\r\nPar=NaN\r\n[Other:Unknown]\r\nPar=3\r\n[NoDefault:Edit]\r\nPar=4\r\n[Bad]\r\nPar=-1\r\nPar=inf\r\nPar=\r\n[Trimmed ]\r\nPar=5\r\n[Has:Colon:Hard]\r\nPar=6\r\n");
        names.extend(
            [
                "Other",
                "Other:Unknown",
                "NoDefault",
                "Bad",
                "Trimmed ",
                "Trimmed",
                "Has:Colon",
                "absent",
                "",
            ]
            .map(String::from),
        );
        check(&text, &names, &difficulties);
    }
    check(
        "Par=1\n; before section\n[]\nPar=0\n#comment\n[Only:Hard]\nOther=3\nPar=2\n",
        &["".into(), "Only".into()],
        &difficulties,
    );
}

#[test]
fn golf_difficulty_dispatch_preserves_ascii_mutations_and_unicode_rejections() {
    let mut text = String::from("[Song]\nPar=-0\n");
    for (i, name) in DIFFICULTIES.iter().enumerate() {
        writeln!(text, "[Song:{name}]\nPar={}", i + 1).unwrap();
    }
    let mut difficulties = Vec::new();
    for name in DIFFICULTIES {
        for mask in 0..(1usize << name.len()) {
            let bytes: Vec<_> = name
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    if mask & (1 << i) != 0 {
                        b.to_ascii_uppercase()
                    } else {
                        b.to_ascii_lowercase()
                    }
                })
                .collect();
            difficulties.push(String::from_utf8(bytes).unwrap());
        }
        for index in 0..name.len() {
            for byte in 0..=127 {
                let mut candidate = name.as_bytes().to_vec();
                candidate[index] = byte;
                difficulties.push(String::from_utf8(candidate).unwrap());
            }
        }
        for extra in ["\u{00e9}", "\u{0130}", "\u{2003}", "\u{96ea}"] {
            difficulties.extend([format!("{name}{extra}"), format!("{extra}{name}")]);
        }
    }
    check(&text, &["Song".into()], &difficulties);
    // The same dispatch classifies section suffixes during parsing.
    for (index, difficulty) in difficulties.iter().enumerate() {
        writeln!(text, "[Section:{difficulty}]\nPar={index}").unwrap();
    }
    let names: Vec<_> = difficulties
        .iter()
        .map(|difficulty| format!("Section:{difficulty}"))
        .chain(std::iter::once("Section".into()))
        .collect();
    check(
        &text,
        &names,
        &["Hard".into(), "Edit".into(), "unknown".into()],
    );
}

#[test]
fn public_pack_lookup_preserves_replacement_and_removal() {
    let pack = Path::new("golf-metadata-reload");
    let simfile = pack.join("Song").join("chart.ssc");
    for text in [
        "[Song]\nPar=-0\n[Song:Hard]\nPar=2.5\n",
        "[Song:Edit]\nPar=3.25\n",
        "",
    ] {
        let old = original::GolfPars::parse(text);
        deadsync_chart::golf::replace_pack(pack, GolfPars::parse(text));
        for difficulty in ["Hard", "Challenge", "Edit", "unknown"] {
            assert_eq!(
                deadsync_chart::golf::par(&simfile, difficulty).map(f64::to_bits),
                old.get("Song", difficulty).map(f64::to_bits)
            );
        }
    }
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_golf_metadata() {
    for (label, overrides) in [("defaults", 0), ("sparse", 1), ("complete", 6)] {
        let (text, names) = fixture(256, overrides);
        measure(&format!("parse/{label}"), 16, || {
            black_box(GolfPars::parse(black_box(&text)));
        });
        let pars = GolfPars::parse(&text);
        measure(&format!("lookup/{label}"), 64, || {
            for name in black_box(&names) {
                for difficulty in black_box(DIFFICULTIES) {
                    black_box(pars.get(black_box(name), black_box(difficulty)));
                }
            }
        });
        measure(&format!("load_query/{label}"), 16, || {
            let pars = GolfPars::parse(black_box(&text));
            for name in black_box(&names) {
                for difficulty in black_box(DIFFICULTIES) {
                    black_box(pars.get(black_box(name), black_box(difficulty)));
                }
            }
        });
        let pack = Path::new("golf-metadata-benchmark").join(label);
        let simfiles: Vec<_> = names
            .iter()
            .map(|name| pack.join(name).join("chart.ssc"))
            .collect();
        deadsync_chart::golf::replace_pack(&pack, GolfPars::parse(&text));
        measure(&format!("pack_lookup/{label}"), 8, || {
            for simfile in black_box(&simfiles) {
                for difficulty in black_box(DIFFICULTIES) {
                    black_box(deadsync_chart::golf::par(
                        black_box(simfile),
                        black_box(difficulty),
                    ));
                }
            }
        });
    }
}
