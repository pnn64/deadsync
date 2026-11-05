use super::{SelectMusicSeriesSource, SrpgShopFolder, TournamentScoringSystem};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use std::io::Write;
use std::str::FromStr;
#[path = "choice_original.rs"]
mod original;
fn check(s: &str) {
    assert_eq!(
        original::series(s),
        SelectMusicSeriesSource::from_str(s),
        "series {s:?}"
    );
    assert_eq!(original::shop(s), SrpgShopFolder::from_str(s), "shop {s:?}");
    assert_eq!(
        original::scoring(s),
        TournamentScoringSystem::from_str(s),
        "scoring {s:?}"
    );
}
fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for name in [
        "packini",
        "pack.ini",
        "metadata",
        "folder",
        "folders",
        "directory",
        "filesystem",
        "unlocks",
        "srpg10 unlocks",
        "shops",
        "srpg10 - shops",
        "faction",
        "factions",
        "by faction",
        "ex",
        "itg",
    ] {
        for mask in 0..1usize << name.len() {
            let s: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) == 0 {
                        b
                    } else {
                        b.to_ascii_uppercase()
                    })
                })
                .collect();
            visit(&s);
            visit(&format!("\u{2003}{s}\t\n"));
        }
        for at in 0..=name.len() {
            for byte in 0..128u8 {
                visit(&format!(
                    "{}{}{}",
                    &name[..at],
                    char::from(byte),
                    &name[at..]
                ));
            }
        }
    }
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&ch.to_string());
            visit(&format!("{ch}pack.ini"));
            visit(&format!("srpg10 - shops{ch}"));
        }
    }
    let mut seed = 0x1234_5678u32;
    let chars = [
        'p', 'a', 'c', 'k', 'i', 'n', 's', 'r', 'g', 'h', 'o', 'f', 'l', 'd', 'e', 't', 'y', 'x',
        'M', 'R', '_', '-', ' ', '.', '\0', 'é', '雪', '\u{2003}',
    ];
    for _ in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let len = seed % 25;
        let s: String = (0..len)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                chars[seed as usize % chars.len()]
            })
            .collect();
        visit(&s);
    }
    for s in [
        "",
        "pack-ini",
        "pack_ini",
        "srpg10-shops",
        "by  faction",
        "ITG!",
        "ＥＸ",
        "\u{feff}folder",
        "\0",
        "srpg10\tunlocks",
    ] {
        visit(s);
    }
}
#[test]
fn strict_choices_case_whitespace_unicode_and_rejections_match() {
    cases(1, check);
}
fn compare(label: &str, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, 50000, work);
}
#[test]
#[ignore]
fn benchmark() {
    use std::hint::black_box as bb;
    for (label, s) in [
        ("packini", "PaCkInI"),
        ("dotted", "PACK.ini"),
        ("metadata", "MetaDATA"),
        ("folder", "FOLDER"),
        ("folders", "Folders"),
        ("directory", "DiRECTory"),
        ("filesystem", "FileSYSTEM"),
        ("space", "  pack.INI\t"),
        ("invalid", "pack-ini"),
        ("empty", ""),
    ] {
        compare(&format!("series/{label}"), |c| {
            let _ = bb(if c {
                SelectMusicSeriesSource::from_str(bb(s))
            } else {
                original::series(bb(s))
            });
        });
    }
    for (label, s) in [
        ("unlocks", "UNLOCKS"),
        ("legacy-unlocks", "SrPg10 Unlocks"),
        ("shops", "SHOPS"),
        ("legacy-shops", "SRPG10 - shops"),
        ("faction", "FaCTion"),
        ("factions", "FACTions"),
        ("by-faction", "By Faction"),
        ("space", "  Srpg10 - Shops \t"),
        ("invalid", "srpg10-shops"),
        ("empty", ""),
    ] {
        compare(&format!("shop/{label}"), |c| {
            let _ = bb(if c {
                SrpgShopFolder::from_str(bb(s))
            } else {
                original::shop(bb(s))
            });
        });
    }
    for (label, s) in [
        ("ex", "eX"),
        ("itg", "ItG"),
        ("space", "  EX\t"),
        ("invalid", "I-T-G"),
        ("empty", ""),
    ] {
        compare(&format!("scoring/{label}"), |c| {
            let _ = bb(if c {
                TournamentScoringSystem::from_str(bb(s))
            } else {
                original::scoring(bb(s))
            });
        });
    }
}

#[test]
#[ignore = "deterministic strict-choice input/result trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_CONFIG_CHOICE_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    cases(17, |s| {
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
        let a = match SelectMusicSeriesSource::from_str(s) {
            Ok(SelectMusicSeriesSource::PackIni) => 0,
            Ok(SelectMusicSeriesSource::Folder) => 1,
            Err(()) => 2,
        };
        let b = match SrpgShopFolder::from_str(s) {
            Ok(SrpgShopFolder::Unlocks) => 0,
            Ok(SrpgShopFolder::Shops) => 1,
            Ok(SrpgShopFolder::Faction) => 2,
            Err(()) => 3,
        };
        let c = match TournamentScoringSystem::from_str(s) {
            Ok(TournamentScoringSystem::Ex) => 0,
            Ok(TournamentScoringSystem::Itg) => 1,
            Err(()) => 2,
        };
        out.write_all(&[a, b, c]).unwrap();
    });
    out.flush().unwrap();
}
