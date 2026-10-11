use super::*;
use std::hint::black_box;
#[path = "cabinet_chart_original.rs"]
mod original;
#[path = "../../../tests/perf/runtime_support.rs"]
mod support;
fn fixture(count: usize, kind: &str) -> SongData {
    let mut song = test_song("Songs/Test/song.ssc", 0.0, ["hard", "medium"]);
    song.charts = (0..count)
        .map(|i| test_chart_with("dance-single", "Easy", &i.to_string()))
        .collect();
    if count > 1 {
        match kind {
            "explicit-first" => {
                song.charts[0] = test_chart_with("lights-cabinet", "Medium", "first")
            }
            "explicit-last" => {
                song.charts[count - 1] = test_chart_with("lights-cabinet", "Medium", "last")
            }
            "generated-first" => {
                song.charts[0] = test_chart_with("dance-single", "Hard", "hard");
                song.charts[1] = test_chart_with("dance-single", "Medium", "medium");
            }
            _ => {}
        }
    }
    song
}
fn signature(plan: Option<CabinetLightPlan>) -> Option<(Vec<usize>, Vec<String>)> {
    plan.map(|p| (p.request_chart_ixs(), p.source_hashes()))
}
#[test]
fn exact_light_match_preserves_first_ties_and_fallbacks() {
    let choices = [
        ("dance-single", "Medium", true),
        ("DANCE-SINGLE", "HARD", true),
        ("lights-cabinet", "Medium", true),
        ("Lights-Cabinet", "Hard", true),
        ("dance-single", "Edit", true),
        ("dance-single", "Medium", false),
        ("pump-single", "Easy", true),
        ("dance-single", "Easy", true),
    ];
    let mut song = fixture(0, "");
    for a in choices {
        for b in choices {
            for c in choices {
                song.charts = [a, b, c]
                    .map(|(ty, diff, data)| {
                        let mut chart = test_chart_with(ty, diff, diff);
                        chart.has_note_data = data;
                        chart
                    })
                    .into();
                for preferred in 0..8 {
                    for ty in ["dance-single", "lights-cabinet", "pump-single", "missing"] {
                        assert_eq!(
                            closest_standard_chart_ix(&song, ty, preferred),
                            original::closest_standard_chart_ix_original(&song, ty, preferred)
                        );
                    }
                }
                for fallback in 0..=3 {
                    assert_eq!(
                        signature(cabinet_light_plan(&song, fallback)),
                        signature(original::cabinet_light_plan_original(&song, fallback))
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_lights() {
    for count in [0, 2, 16, 256] {
        for kind in [
            "explicit-first",
            "explicit-last",
            "generated-first",
            "no-exact",
        ] {
            let song = fixture(count, kind);
            let label = format!("lights-{kind}-{count}");
            assert_eq!(
                signature(cabinet_light_plan(&song, 0)),
                signature(original::cabinet_light_plan_original(&song, 0))
            );
            support::compare(
                &label,
                100,
                || {
                    black_box(original::cabinet_light_plan_original(
                        black_box(&song),
                        black_box(0),
                    ));
                },
                || {
                    black_box(cabinet_light_plan(black_box(&song), black_box(0)));
                },
            );
        }
    }
}
