//! Frozen-parser parity and CPU/allocation comparisons for INI loading.
use deadsync_config::ini::{SimpleIni, ini_value};
use std::fmt::Write;
use std::hint::black_box;
use std::path::Path;

#[path = "ini_loading/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/profile_ini.rs"]
#[allow(dead_code)]
mod profile;

fn assert_parity(text: &str, all_lookups: bool) {
    let mut old = baseline::SimpleIni::new();
    old.load_str(text);
    let mut new = SimpleIni::new();
    new.load_str(text);
    assert_eq!(new.sections(), old.sections());
    let old_profile = baseline::ProfileIni::parse(text);
    let new_profile = profile::ProfileIni::parse(text);
    for (section, properties) in old.sections() {
        assert_eq!(
            new_profile.section_has_any(section),
            old_profile.section_has_any(section)
        );
        for (key, value) in properties {
            assert_eq!(old_profile.get(section, key).as_ref(), Some(value));
            assert_eq!(new_profile.get(section, key).as_ref(), Some(value));
            if all_lookups {
                assert_eq!(ini_value(text, section, key), Some(value.as_str()));
            }
        }
    }
    for (section, key) in [("Meta", "NativeName"), ("", "root"), ("missing", "missing")] {
        assert_eq!(ini_value(text, section, key), old.get(section, key));
        assert_eq!(new_profile.get(section, key), old_profile.get(section, key));
        assert_eq!(
            new_profile.section_has_any(section),
            old_profile.section_has_any(section)
        );
    }
}

#[test]
fn parsers_preserve_root_empty_repeated_and_malformed_sections() {
    for text in [
        "",
        " \r\n; comment\n#comment",
        "[]",
        "[ ]\n=ignored\n[Empty]",
        "root=before\n[ A ]\nx=first\n[]\nroot=after\n[A]\nx=last\n[A]\ny=two",
        "key=value=extra\n[broken\nx=one\n[ok] trailing\ny=two\n[a=b]\n[]=property",
        "\u{feff}[Meta]\nNativeName=not in Meta\n[Meta]\nNativeName=\n[Meta]",
        "\u{2003}[\u{3000}Meta\u{2002}]\u{a0}\n NativeName = \u{65e5}\u{672c}\u{8a9e}\u{3000}\n",
        "[Meta]\nNativeName=first\n[Other]\nNativeName=ignored\n[Meta]\nNativeName=last",
        "[A]\nKey=upper\nkey=lower\n;Key=comment\n#key=comment\n[ a ]\nKey=other",
        "\0=zero\n[\0]\n\0=embedded\0value\n[]=\n\\=\\n",
    ] {
        assert_parity(text, true);
    }
}

#[test]
fn generated_ini_inputs_match_both_previous_parsers() {
    let pieces = [
        "[Meta]",
        "[Options]",
        "[ ]",
        "[Empty]",
        "[Meta]",
        "[Odd=Section]",
        "NativeName=first",
        "NativeName=",
        "key=last=tail",
        "key = replacement",
        "root=value",
        "invalid",
        "[invalid",
        "[also] invalid",
        "=skip",
        " ; skip",
        "# skip",
        " \u{2003} ",
        "\u{3b1}=\u{3b2}",
        "Key=Mixed",
        "[options]",
    ];
    let mut seed = 834798341u64;
    for case in 0..512 {
        let mut text = String::new();
        for _ in 0..case % 100 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(pieces[(seed >> 32) as usize % pieces.len()]);
            text.push_str(if seed & 1 == 0 { "\r\n" } else { "\n" });
        }
        assert_parity(&text, true);
    }
}

#[test]
fn bundled_language_files_preserve_every_property_and_native_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/languages");
    let mut count = 0;
    for path in std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
    {
        if path.extension().is_some_and(|ext| ext == "ini") {
            assert_parity(&std::fs::read_to_string(path).unwrap(), false);
            count += 1;
        }
    }
    assert!(count > 1);
}

#[test]
fn reload_discards_stale_properties_and_retains_exact_results() {
    let mut old = baseline::SimpleIni::new();
    let mut new = SimpleIni::new();
    for text in [
        "[A]\none=1\ntwo=2\n[B]\nstale=yes",
        "[A]\none=new",
        "",
        "[Empty]",
        "root=ok",
    ] {
        old.load_str(text);
        new.load_str(text);
        assert_eq!(new.sections(), old.sections());
        assert_eq!(new.get_section("A"), old.get_section("A"));
    }
    assert_eq!(new.into_sections(), old.into_sections());
}

#[test]
fn selective_lookup_borrows_input_without_heap_churn() {
    let text = include_str!("../../../assets/languages/en.ini");
    perf::assert_no_churn(|| {
        let value = black_box(ini_value(black_box(text), "Meta", "NativeName").unwrap());
        assert!(value.as_ptr() >= text.as_ptr());
        assert!((value.as_ptr() as usize + value.len()) <= text.as_ptr() as usize + text.len());
        assert_eq!(value, "English");
        assert_eq!(ini_value(black_box(text), "Missing", "Missing"), None);
    });
    perf::assert_churn_budget(1, "English".len(), || {
        black_box(new_name(text));
    });
}

fn old_config(text: &str) {
    let mut ini = baseline::SimpleIni::new();
    ini.load_str(text);
    black_box(ini);
}

fn new_config(text: &str) {
    let mut ini = SimpleIni::new();
    ini.load_str(text);
    black_box(ini);
}

fn old_profile(text: &str) {
    black_box(baseline::ProfileIni::parse(text));
}

fn new_profile(text: &str) {
    black_box(profile::ProfileIni::parse(text));
}

fn old_name(text: &str) -> String {
    let mut ini = baseline::SimpleIni::new();
    ini.load_str(text);
    ini.get("Meta", "NativeName")
        .unwrap_or("fallback")
        .to_owned()
}

fn new_name(text: &str) -> String {
    ini_value(text, "Meta", "NativeName")
        .unwrap_or("fallback")
        .to_owned()
}

fn settings(sections: usize, properties: usize) -> String {
    let mut text = String::new();
    for section in 0..sections {
        writeln!(text, "[PlayerOptions{section}]").unwrap();
        for key in 0..properties {
            writeln!(text, "Setting{key}=value-{section}-{key}").unwrap();
        }
    }
    text
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn ini_loading_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    let typical = settings(8, 80);
    let duplicate = "[Options]\nSame=first\nSame=last\n".repeat(256);
    let en = include_str!("../../../assets/languages/en.ini");
    let ja = include_str!("../../../assets/languages/ja.ini");
    for (case, text) in [
        ("empty", ""),
        ("tiny", "[Options]\nKey=value\n"),
        ("profile", typical.as_str()),
        ("english", en),
        ("japanese", ja),
        ("duplicates", duplicate.as_str()),
    ] {
        for (kind, before, after) in [
            ("config", old_config as fn(&str), new_config as fn(&str)),
            ("profile", old_profile as fn(&str), new_profile as fn(&str)),
        ] {
            for old in order {
                let run = black_box(if old { before } else { after });
                perf::measure_sampled(
                    &format!("{kind}_{case}_{}", if old { "old" } else { "new" }),
                    (2_000_000 / text.len().max(1)).clamp(4, 2000),
                    text.len().max(1),
                    || run(black_box(text)),
                );
            }
        }
    }
    for (case, text) in [
        ("english", en),
        ("japanese", ja),
        ("absent", typical.as_str()),
        (
            "repeated",
            "[Meta]\nNativeName=first\n[Other]\nNativeName=no\n[Meta]\nNativeName=last",
        ),
    ] {
        for old in order {
            let run = black_box(if old { old_name } else { new_name });
            perf::measure_sampled(
                &format!("name_{case}_{}", if old { "old" } else { "new" }),
                (2_000_000 / text.len()).clamp(4, 2000),
                text.len(),
                || run(black_box(text)),
            );
        }
    }
}
