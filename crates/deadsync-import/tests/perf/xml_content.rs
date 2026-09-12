use super::*;
use crate::perf;
use std::fmt::Write as _;
use std::hint::black_box;

pub(crate) mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/xml_content/baseline.rs"
    ));
}

pub(crate) fn stats_xml(count: usize, pretty: bool, entities: bool) -> String {
    let ws = if pretty { "\n    " } else { "" };
    let label = if entities {
        "A &amp; B &#x96EA; &unknown;"
    } else {
        "Snow 雪"
    };
    let mut out = String::from(
        "<Stats><GeneralData><Guid>guid-1234</Guid><CurrentCombo>17</CurrentCombo></GeneralData><SongScores>",
    );
    for i in 0..count {
        write!(&mut out, "{ws}<Song Dir=\"Songs/Pack/{i}\">{ws}<Steps StepsType=\"dance-single\" Difficulty=\"Hard\" Description=\"{label}\">{ws}<HighScoreList>{ws}<HighScore>{ws}<Grade>Grade_Tier01</Grade>{ws}<DateTime>2026-09-12 12:34:56</DateTime>{ws}<PercentDP>0.99</PercentDP>{ws}<Modifiers>{label}</Modifiers>{ws}<SurviveSeconds>123.5</SurviveSeconds>{ws}<TapNoteScores>{ws}<W1>100</W1>{ws}<W2>2</W2>{ws}<W3>1</W3>{ws}<Miss>0</Miss>{ws}</TapNoteScores>{ws}<HoldNoteScores>{ws}<Held>10</Held>{ws}<LetGo>0</LetGo>{ws}</HoldNoteScores>{ws}</HighScore>{ws}</HighScoreList>{ws}</Steps>{ws}</Song>").unwrap();
    }
    out.push_str("</SongScores></Stats>");
    out
}

fn equivalent(input: &str) {
    let old = baseline::parse(input).map_err(|e| e.to_string());
    let new = parse(input).map_err(|e| e.to_string());
    assert_eq!(old, new, "input {input:?}");
}

#[test]
fn borrowed_content_preserves_mixed_text_cdata_and_existing_errors() {
    let runs = [
        "",
        " \n\t",
        "\u{2003} \u{a0}",
        " text 雪 ",
        "&amp;",
        "&#32;&#x2003;",
        "&bad; &&amp;",
        "&#0;",
        "\u{0}",
    ];
    for a in runs {
        for b in runs {
            for middle in [
                "<!-- gap -->",
                "<Child/>",
                "<![CDATA[  raw &amp; 雪  ]]>",
                "<![CDATA[]]>",
            ] {
                equivalent(&format!("<root>{a}{middle}{b}</root>"));
            }
        }
    }
    for input in [
        "",
        " ",
        "<",
        "<a",
        "<a/",
        "<a x=>",
        "<a x='oops",
        "<a></b>",
        "<a></a extra>",
        "<!--",
        "<?xml",
        "<!DOCTYPE",
        "<a><![CDATA[",
        "<a>text",
        "<a><b/>",
        "<a flag other='雪' other='second'/>",
        "<a>ignored</a><second/>",
    ] {
        equivalent(input);
    }
    for count in [0, 1, 16, 128] {
        for pretty in [false, true] {
            for entities in [false, true] {
                equivalent(&stats_xml(count, pretty, entities));
            }
        }
    }
    let mut seed = 11u64;
    for _ in 0..512 {
        let mut text = String::from("<root>");
        for _ in 0..12 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(match (seed >> 32) % 7 {
                0 => " \u{2003} ",
                1 => "a &amp; b",
                2 => "<![CDATA[ &lt; 雪 ]]>",
                3 => "<child>  2 </child>",
                4 => "<!-- gap -->",
                5 => "&&bad;",
                _ => "z 雪",
            });
        }
        text.push_str("</root>");
        equivalent(&text);
    }
}

#[test]
fn entity_runs_preserve_unknown_nested_and_numeric_references() {
    let cases = [
        "",
        "no entities 雪",
        "&",
        "&&",
        "&;",
        "&amp;",
        "&lt;&gt;&quot;&apos;",
        "&unknown;",
        "&x&amp;",
        "&amp;amp;",
        "&#0;",
        "&#x10ffff;",
        "&#x110000;",
        "&#xD800;",
        "&#-1;",
        "&#+65;",
        "&#X41;",
        "&#4294967296;",
        "&#9999999999999999999;",
        "&a&b&c;tail",
        "雪&broken 雪",
    ];
    for input in cases {
        for prefix in ["", "prefix 雪 "] {
            let mut old = prefix.to_string();
            let mut new = old.clone();
            baseline::append_decoded_entities(&mut old, input);
            append_decoded_entities(&mut new, input);
            assert_eq!(old, new, "{input:?}");
        }
    }
    let alphabet = ["&", ";", "amp", "#", "x", "65", "雪", "é", " ", "lt", "0"];
    let mut seed = 31u64;
    for count in 0..1024 {
        let mut input = String::new();
        for _ in 0..count % 137 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            input.push_str(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        let mut old = String::new();
        let mut new = String::new();
        baseline::append_decoded_entities(&mut old, &input);
        append_decoded_entities(&mut new, &input);
        assert_eq!(old, new);
    }
}

#[test]
fn indentation_and_borrowed_runs_do_not_allocate_scratch() {
    let mut text = Cow::Borrowed("");
    perf::assert_no_churn(|| {
        append_text(&mut text, " \n\t");
        append_text(&mut text, "\u{2003}");
        append_text(&mut text, "  leaf 雪  ");
    });
    assert!(matches!(text, Cow::Borrowed("leaf 雪  ")));
    perf::assert_churn_budget(1, "leaf 雪  suffix".len(), || {
        append_text(&mut text, "suffix")
    });
    assert_eq!(text, "leaf 雪  suffix");
    let input = "prefix &amp; 雪 &unknown; &#x41;";
    let mut output = String::with_capacity(input.len());
    perf::assert_no_churn(|| append_decoded_entities(&mut output, input));
    assert_eq!(output, "prefix & 雪 &unknown; A");
}

pub(crate) fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before =
        || perf::measure_sampled(&format!("{name}_old"), iterations, units.max(1), &mut old);
    let mut after =
        || perf::measure_sampled(&format!("{name}_new"), iterations, units.max(1), &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn profile_import_bench_xml() {
    for count in [0, 1, 128, 512] {
        for pretty in [false, true] {
            let input = stats_xml(count, pretty, false);
            pair(
                &format!("xml_{count}_{}", if pretty { "pretty" } else { "compact" }),
                if count > 1 { 32 } else { 8192 },
                input.len(),
                || baseline::parse(black_box(&input)).unwrap(),
                || parse(black_box(&input)).unwrap(),
            );
        }
    }
    for (name, input) in [
        ("entities", stats_xml(128, true, true)),
        (
            "mixed",
            format!(
                "<root>{}</root>",
                "a<![CDATA[ 雪 ]]><child/>b &amp; <!-- gap -->".repeat(128)
            ),
        ),
        ("empty", String::from("<r/>")),
        ("scalar", String::from("<r>1</r>")),
        (
            "whitespace",
            format!("<r>{}</r>", " \n\u{2003}".repeat(1024)),
        ),
    ] {
        pair(
            &format!("xml_{name}"),
            if input.len() > 1000 { 128 } else { 32768 },
            input.len(),
            || baseline::parse(black_box(&input)).unwrap(),
            || parse(black_box(&input)).unwrap(),
        );
    }
    for (name, input) in [
        ("plain", "plain text 雪".repeat(256)),
        (
            "single",
            format!(
                "{}&amp;{}",
                "long prefix 雪 ".repeat(128),
                " long suffix".repeat(128)
            ),
        ),
        ("dense", "&amp;&lt;&#65;".repeat(256)),
        ("unknown", "long text 雪 &unknown; ".repeat(256)),
        ("amp_storm", format!("{};", "&".repeat(4096))),
        ("no_semicolon", "&".repeat(4096)),
        ("short", "a &amp; b".into()),
        ("empty", String::new()),
    ] {
        let mut old = String::with_capacity(input.len());
        let mut new = String::with_capacity(input.len());
        pair(
            &format!("entity_{name}"),
            if input.len() < 32 { 32768 } else { 256 },
            input.len(),
            || {
                old.clear();
                baseline::append_decoded_entities(black_box(&mut old), black_box(&input));
                black_box(&old);
            },
            || {
                new.clear();
                append_decoded_entities(black_box(&mut new), black_box(&input));
                black_box(&new);
            },
        );
    }
}
