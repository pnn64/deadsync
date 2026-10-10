use super::*;
use crate::preview_dataflows_support::compare;
use std::hint::black_box;

mod original {
    include!("preview_progress_original.rs");
    #[inline(always)]
    pub(super) fn box_body(p: &score_data::EventProgress) -> String {
        build_srpg_box_body(p)
    }
    #[inline(always)]
    pub(super) fn overlay(p: &score_data::EventProgress) -> String {
        build_srpg_overlay_body(p)
    }
}

fn progress(names: &[&str], skills: &[&str]) -> score_data::EventProgress {
    score_data::EventProgress {
        kind: score_data::EventProgressKind::Srpg,
        score_hundredths: 9876,
        score_delta_hundredths: -123,
        rate_hundredths: Some(125),
        rate_delta_hundredths: Some(5),
        stat_improvements: names
            .iter()
            .enumerate()
            .map(|(i, name)| score_data::EventStatImprovement {
                name: (*name).to_owned(),
                gained: i as u32 + 1,
                current: 10,
            })
            .collect(),
        skill_improvements: skills.iter().map(|s| (*s).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn srpg_bodies_preserve_grouping_duplicates_and_zero_gains() {
    let names = ["lp", "BB", "tp", "gold", "JP", "TP", "Other", " lp"];
    for len in 0..=names.len() {
        for rotate in 0..names.len() {
            for zero in 0..=len {
                let mut names = names;
                names.rotate_left(rotate);
                let mut p = progress(&names[..len], &[]);
                if let Some(stat) = p.stat_improvements.get_mut(zero) {
                    stat.gained = 0;
                }
                assert_eq!(
                    build_srpg_box_body(&p),
                    original::box_body(&p),
                    "{len}/{rotate}/{zero}"
                );
                assert_eq!(build_srpg_overlay_body(&p), original::overlay(&p));
            }
        }
    }
    let p = progress(&["bb", "LP", "gold", "tp", "jp"], &[]);
    assert_eq!(
        build_srpg_box_body(&p),
        "Score: 98.76% (-1.23%)\nRate: 1.25 (+0.05)\n\n+2 LP +4 TP\n+1 BB\n+3 GOLD\n+5 JP"
    );
}

#[test]
fn srpg_bodies_preserve_unicode_whitespace_and_numeric_extremes() {
    for skills in [
        vec![],
        vec![""],
        vec![" ", "\t"],
        vec!["Level up!", "", " \n"],
        vec!["café\u{2003}"],
    ] {
        let mut p = progress(
            &["straße", "ﬄ", "iıİ", "σς", "e\u{301}", "\u{2003}", ""],
            &skills,
        );
        p.score_hundredths = u32::MAX;
        p.score_delta_hundredths = i32::MIN;
        p.rate_hundredths = Some(u32::MAX);
        p.rate_delta_hundredths = Some(i32::MIN);
        p.stat_improvements[0].gained = u32::MAX;
        assert_eq!(build_srpg_box_body(&p), original::box_body(&p));
        assert_eq!(build_srpg_overlay_body(&p), original::overlay(&p));
        p.rate_hundredths = None;
        p.rate_delta_hundredths = None;
        assert_eq!(build_srpg_overlay_body(&p), original::overlay(&p));
    }
}

#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_preview_dataflows_progress() {
    for (label, p) in [
        ("empty", progress(&[], &[])),
        ("three", progress(&["bb", "gold", "jp"], &[])),
        (
            "five",
            progress(
                &["tp", "lp", "bb", "gold", "jp"],
                &["Stream level up", "New technique unlocked"],
            ),
        ),
        (
            "unicode",
            progress(&["straße", "ﬄ", "σς", "jp", "gold"], &["café", "", " \n"]),
        ),
    ] {
        compare(
            &format!("progress/box-{label}"),
            || {
                black_box(original::box_body(black_box(&p)));
            },
            || {
                black_box(build_srpg_box_body(black_box(&p)));
            },
        );
        compare(
            &format!("progress/overlay-{label}"),
            || {
                black_box(original::overlay(black_box(&p)));
            },
            || {
                black_box(build_srpg_overlay_body(black_box(&p)));
            },
        );
    }
}
