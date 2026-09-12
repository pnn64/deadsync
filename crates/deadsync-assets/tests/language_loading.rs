//! Behavior and operation-level comparisons for full language loading.
use deadsync_config::ini::{
    BorrowedIniSections, SimpleIni, borrowed_ini_sections, unescape_ini_value,
};
use std::hint::black_box;
use std::path::Path;
use std::sync::Arc;

type LanguageMap = deadsync_assets::language::LanguageMap;
#[path = "language_loading/baseline.rs"]
mod baseline;
#[path = "../src/language/map.rs"]
mod map;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn old_parse(text: &str) -> baseline::SimpleIni {
    let mut ini = baseline::SimpleIni::new();
    ini.load_str(text);
    ini
}

fn old_language(text: &str) -> LanguageMap {
    baseline::language_map(old_parse(text))
}

fn old_parse_work(text: &str) {
    black_box(old_parse(text));
}

fn borrowed_parse_work(text: &str) {
    black_box(borrowed_ini_sections(text));
}

fn owned_parse_work(text: &str) {
    let mut ini = SimpleIni::new();
    ini.load_str(text);
    black_box(ini);
}

fn assert_parity(text: &str) {
    let old = old_parse(text);
    let borrowed = borrowed_ini_sections(text);
    let mut owned = SimpleIni::new();
    owned.load_str(text);
    assert_eq!(owned.sections(), old.sections());
    assert_eq!(borrowed.len(), old.sections().len());
    for (section, props) in old.sections() {
        let values = borrowed.get(section.as_str()).unwrap();
        assert_eq!(values.len(), props.len());
        for (key, value) in props {
            assert_eq!(values.get(key.as_str()).copied(), Some(value.as_str()));
        }
    }
    for (&section, props) in &borrowed {
        for text_slice in std::iter::once(section).chain(props.iter().flat_map(|(&k, &v)| [k, v])) {
            assert!(
                text_slice.is_empty()
                    || (text_slice.as_ptr() as usize >= text.as_ptr() as usize
                        && text_slice.as_ptr() as usize + text_slice.len()
                            <= text.as_ptr() as usize + text.len())
            );
        }
    }
    let expected = baseline::language_map(old);
    assert_eq!(map::parse_language_map(text), expected);
    assert_eq!(map::build_language_map(&borrowed), expected);
    assert_eq!(unreserved_language_map(&borrowed), expected);
}

#[test]
fn duplicate_skip_empty_root_and_escape_semantics_match() {
    for text in [
        "",
        "[Empty]\n[ ]\n=ignored",
        "root=one\n[]\nroot=two",
        "[A]\nkey=visible\n[A]\nkey=@skip",
        "[A]\nkey=@skip\n[A]\nkey=restored",
        "[A]\na=@skip\nb=@SKIP\nc=\\t@skip\\t\nd=\\n\ne=\\q\nf=tail\\",
        "[A]\nkey=one\n[Empty]\n[A]\nkey=two\nkey=\n[]\nroot=ok",
        "\u{2003}[\u{3000}Meta]\u{a0}\nNativeName=\u{65e5}\u{672c}\u{8a9e}\u{3000}",
        "\u{feff}[ignored]\nx=one\n[malformed\ny=two\n[a=b]\nz=three=four\n# skip",
        "[\0]\n\0=\0\\n\nCase=upper\ncase=lower\n[Case]\nkey=other",
    ] {
        assert_parity(text);
    }
    let skipped = map::parse_language_map("[A]\na=@skip\nb=@skip\n[Empty]");
    assert_eq!(skipped.len(), 2);
    assert!(
        skipped
            .values()
            .all(|props| props.is_empty() && props.capacity() == 0)
    );
}

#[test]
fn generated_language_inputs_match_frozen_pipeline() {
    let pieces = [
        "[A]",
        "[]",
        "[Empty]",
        "[A]",
        "[Other]",
        "key=value",
        "key=@skip",
        "key=",
        "key=\\tvalue\\n",
        "root=\\q",
        "invalid",
        "=ignore",
        "#comment",
        ";comment",
        "[malformed",
        "\u{3b1}=\u{3b2}",
        "Other=\\\\",
        "key=\\t@skip",
    ];
    let mut state = 983275343u64;
    for case in 0..512 {
        let mut text = String::new();
        for _ in 0..case % 100 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(pieces[(state >> 32) as usize % pieces.len()]);
            text.push_str(if state & 1 == 0 { "\r\n" } else { "\n" });
        }
        assert_parity(&text);
    }
}

#[test]
fn every_bundled_language_and_public_bundle_matches() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/languages");
    let english = old_language(include_str!("../../../assets/languages/en.ini"));
    let mut count = 0;
    for path in std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
    {
        if path.extension().is_some_and(|ext| ext == "ini") {
            let text = std::fs::read_to_string(&path).unwrap();
            assert_parity(&text);
            let locale = path.file_stem().unwrap().to_str().unwrap();
            let bundle = deadsync_assets::language::load_for_tests(locale);
            assert_eq!(bundle.locale, locale);
            assert_eq!(bundle.fallback, english);
            assert_eq!(
                bundle.active,
                if locale == "en" {
                    LanguageMap::default()
                } else {
                    old_language(&text)
                }
            );
            count += 1;
        }
    }
    assert!(count > 1);
    let missing = deadsync_assets::language::load_for_tests("missing-language-loading-test");
    assert!(missing.active.is_empty());
    assert_eq!(missing.fallback, english);
}

#[test]
fn escape_decoder_preserves_every_unicode_scalar_and_slash_sequence() {
    let prefix = "unchanged UTF-8 span before escape: ";
    let mut text = String::with_capacity(128);
    for scalar in (0..=0x10ffff).filter_map(char::from_u32) {
        text.clear();
        text.push_str(prefix);
        text.push('\\');
        text.push(scalar);
        text.push_str(" unchanged tail after escape");
        assert_eq!(
            unescape_ini_value(&text),
            baseline::unescape_ini_value(&text)
        );
    }
    let pieces = [
        "",
        "plain",
        "\\",
        "\\\\",
        "\\n",
        "\\t",
        "\\q",
        "\u{65e5}",
        "\0",
        "\n",
        "\r",
        "\u{1f600}",
    ];
    for first in pieces {
        for second in pieces {
            for third in pieces {
                let text = format!("{first}{second}{third}");
                assert_eq!(
                    unescape_ini_value(&text),
                    baseline::unescape_ini_value(&text)
                );
            }
        }
    }
    for length in [0, 1, 30, 31, 32, 33, 63, 64, 65] {
        for escape in pieces {
            let text = format!(
                "{}{escape}{}\\nend",
                "a".repeat(length),
                "\u{65e5}".repeat(32)
            );
            assert_eq!(
                unescape_ini_value(&text),
                baseline::unescape_ini_value(&text)
            );
        }
    }
}

#[test]
fn borrowed_parsing_avoids_per_string_allocations_and_decoding_allocates_once() {
    let value = "a".repeat(4096);
    let text = format!("[Section]\nFirst={value}\nSecond={value}");
    // Outer table, section table, and one tiny borrowed-property buffer only.
    perf::assert_churn_budget(3, 1024, || {
        black_box(borrowed_ini_sections(black_box(&text)));
    });
    perf::assert_churn_budget(1, value.len(), || {
        black_box(unescape_ini_value(black_box(&value)));
    });
    perf::assert_no_churn(|| {
        black_box(unescape_ini_value(""));
    });
}

// Capacity-only control: the old grow-on-insert strategy, applied to the same
// borrowed input and current decoder as the production reserved-map builder.
fn unreserved_language_map(sections: &BorrowedIniSections<'_>) -> LanguageMap {
    let mut output = LanguageMap::default();
    for (&section, props) in sections {
        let entries = output.entry(Box::from(section)).or_default();
        for (&key, &value) in props {
            if value.trim() == "@skip" {
                continue;
            }
            let value = if value.contains('\\') {
                Arc::from(unescape_ini_value(value))
            } else {
                Arc::from(value)
            };
            entries.insert(Box::from(key), value);
        }
    }
    output
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn language_loading_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    let skipped = (0..500)
        .map(|i| format!("Key{i}=@skip\n"))
        .collect::<String>();
    let skipped = format!("[AllSkipped]\n{skipped}");
    for (case, text) in [
        ("empty", ""),
        ("tiny", "[A]\nk=v\n"),
        ("english", include_str!("../../../assets/languages/en.ini")),
        ("japanese", include_str!("../../../assets/languages/ja.ini")),
        (
            "pseudo",
            include_str!("../../../assets/languages/pseudo.ini"),
        ),
        ("skipped", skipped.as_str()),
    ] {
        let iterations = (2_000_000 / text.len().max(1)).clamp(4, 2000);
        for old in order {
            let run = black_box(if old {
                old_parse_work
            } else {
                borrowed_parse_work
            });
            perf::measure_sampled(
                &format!("parse_{case}_{}", if old { "old" } else { "new" }),
                iterations,
                text.len(),
                || run(black_box(text)),
            );
        }
        if case == "english" {
            for old in order {
                let run = black_box(if old {
                    old_parse_work
                } else {
                    owned_parse_work
                });
                perf::measure_sampled(
                    &format!("owned_english_{}", if old { "old" } else { "new" }),
                    iterations,
                    text.len(),
                    || run(black_box(text)),
                );
            }
        }
        let borrowed = borrowed_ini_sections(text);
        for old in order {
            let run = black_box(if old {
                unreserved_language_map
            } else {
                map::build_language_map
            });
            perf::measure_sampled(
                &format!("reserve_{case}_{}", if old { "old" } else { "new" }),
                iterations,
                text.len(),
                || run(black_box(&borrowed)),
            );
        }
        for old in order {
            let run = black_box(if old {
                old_language
            } else {
                map::parse_language_map
            });
            perf::measure_sampled(
                &format!("language_{case}_{}", if old { "old" } else { "new" }),
                iterations,
                text.len(),
                || run(black_box(text)),
            );
        }
    }
    let plain = "Press START to continue with the current selection. ".repeat(16);
    let unicode = "\u{65e5}\u{672c}\u{8a9e}\u{1f600} ".repeat(64);
    let sparse = format!("{plain}\\n{unicode}\\tend");
    let dense = r"\n\t\\\q".repeat(128);
    let late_dense = format!("{plain}{dense}");
    for (case, text) in [
        ("empty", ""),
        ("tiny", "a\\nb"),
        ("plain", plain.as_str()),
        ("unicode", unicode.as_str()),
        ("sparse", sparse.as_str()),
        ("dense", dense.as_str()),
        ("late_dense", late_dense.as_str()),
    ] {
        for old in order {
            let run = black_box(if old {
                baseline::unescape_ini_value
            } else {
                unescape_ini_value
            });
            perf::measure_sampled(
                &format!("escape_{case}_{}", if old { "old" } else { "new" }),
                5000,
                text.len(),
                || run(black_box(text)),
            );
        }
    }
}
