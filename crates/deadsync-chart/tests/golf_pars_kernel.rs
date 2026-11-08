use deadsync_chart::golf::GolfPars;
use std::hint::black_box;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/golf_pars_original.rs"]
mod original;

const DIFFICULTIES: [&str; 6] = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];

fn pack(songs: usize, overrides: usize) -> String {
    let mut text = String::new();
    for song in 0..songs {
        text.push_str(&format!("[Song {song}]\nPar=1.25\n"));
        for (index, difficulty) in DIFFICULTIES.iter().take(overrides).enumerate() {
            text.push_str(&format!("[Song {song}:{difficulty}]\nPar={index}.5\n"));
        }
    }
    text
}

#[test]
fn pars_preserve_overrides_defaults_invalid_values_and_section_names() {
    let text = format!(
        "{}\n\u{feff};comment\r\n[Song 1:hArD]\r\nPar=9.75\r\n\
        [Song 2:Hard]\nPar=NaN\nPar=-2\nPar=inf\n\
        [Unknown:Expert]\nPar=7\n[Song 3:Edit]\nPar=-0\n\
        [Komm - [Zaia]:CHALLENGE]\nPar=2.125\n\
        [ Song 4 ]\nPar=8\n[empty]\nPar=bad\n",
        pack(16, 6)
    );
    let old = original::GolfPars::parse(&text);
    let new = GolfPars::parse(&text);
    for song in (0..18).map(|i| format!("Song {i}")).chain([
        "Unknown:Expert".into(),
        "Komm - [Zaia]".into(),
        " Song 4 ".into(),
        "empty".into(),
    ]) {
        for difficulty in DIFFICULTIES
            .into_iter()
            .chain(["Expert", "", "HARD ", "\u{0130}"])
        {
            for difficulty in [
                difficulty.to_owned(),
                difficulty.to_ascii_lowercase(),
                difficulty.to_ascii_uppercase(),
            ] {
                assert_eq!(
                    new.get(&song, &difficulty).map(f64::to_bits),
                    old.get(&song, &difficulty).map(f64::to_bits),
                    "{song}/{difficulty}"
                );
            }
        }
    }
}

#[test]
fn parsing_removes_section_strings_and_per_song_difficulty_allocations() {
    for overrides in [0, 1, 6] {
        let text = pack(128, overrides);
        let (_, old) = alloc::measure(|| original::GolfPars::parse(&text));
        let (_, new) = alloc::measure(|| GolfPars::parse(&text));
        assert!(new.allocs < old.allocs);
        if overrides == 6 {
            assert!(new.allocs * 2 < old.allocs);
            assert!(new.allocated_bytes * 2 < old.allocated_bytes);
            assert!(new.peak_bytes < old.peak_bytes);
        }
    }
}

#[test]
#[ignore = "paired release CPU benchmark"]
fn benchmark_golf_pars_kernel() {
    for (count, overrides) in [(1, 6), (128, 0), (128, 1), (128, 6)] {
        let text = pack(count, overrides);
        let (_, old) = alloc::measure(|| original::GolfPars::parse(&text));
        let (_, new) = alloc::measure(|| GolfPars::parse(&text));
        println!("pars/{count}/{overrides} allocations {old:?} -> {new:?}");
        bench::compare(
            &format!("pars/parse/{count}/{overrides}"),
            if count == 1 { 5000 } else { 100 },
            |current| {
                if current {
                    black_box(GolfPars::parse(black_box(&text)));
                } else {
                    black_box(original::GolfPars::parse(black_box(&text)));
                }
            },
        );
        let old = original::GolfPars::parse(&text);
        let new = GolfPars::parse(&text);
        bench::compare(
            &format!("pars/get/{count}/{overrides}/1024"),
            500,
            |current| {
                for i in 0..1024 {
                    let difficulty = black_box(DIFFICULTIES[i % 6]);
                    black_box(if current {
                        new.get(black_box("Song 0"), difficulty)
                    } else {
                        old.get(black_box("Song 0"), difficulty)
                    });
                }
            },
        );
    }
}
