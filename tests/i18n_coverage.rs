//! Translation coverage tests.
//!
//! Validates that all language files in `assets/languages/` are consistent
//! with the English baseline (`en.ini`).
//!
//! Run with: `cargo test --test i18n_coverage`

use deadsync::config::SimpleIni;
use std::collections::BTreeSet;
use std::path::Path;

fn extract_keys(ini: &SimpleIni) -> BTreeSet<(String, String)> {
    let mut keys = BTreeSet::new();
    for (section, props) in ini.sections() {
        if section == "Meta" {
            continue;
        }
        for key in props.keys() {
            keys.insert((section.clone(), key.clone()));
        }
    }
    keys
}

fn load_ini(path: &Path) -> SimpleIni {
    let mut ini = SimpleIni::new();
    ini.load(path)
        .unwrap_or_else(|e| panic!("Failed to load {}: {e}", path.display()));
    ini
}

fn language_files() -> Vec<std::path::PathBuf> {
    let dir = Path::new("assets/languages");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("assets/languages directory must exist")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ini"))
        .filter(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            stem != "en" && stem != "pseudo"
        })
        .collect();
    files.sort();
    files
}

/// Verify the English baseline has no duplicate keys.
#[test]
fn en_ini_has_no_duplicate_keys() {
    let content =
        std::fs::read_to_string("assets/languages/en.ini").expect("en.ini must be readable");

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut duplicates: Vec<(String, String)> = Vec::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if !key.is_empty() && !seen.insert((current_section.clone(), key.clone())) {
                duplicates.push((current_section.clone(), key));
            }
        }
    }

    assert!(
        duplicates.is_empty(),
        "en.ini has {} duplicate key(s):\n{}",
        duplicates.len(),
        duplicates
            .iter()
            .map(|(s, k)| format!("  [{s}] {k}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Verify no translation file contains stale keys that don't exist in en.ini.
///
/// Stale keys indicate a key was renamed or removed in en.ini but not cleaned
/// up in the translation. These are real bugs — the translated string will
/// never be displayed.
#[test]
fn no_stale_keys_in_translations() {
    let en = load_ini(Path::new("assets/languages/en.ini"));
    let en_keys = extract_keys(&en);
    let mut failures: Vec<String> = Vec::new();

    for path in language_files() {
        let lang = load_ini(&path);
        let lang_keys = extract_keys(&lang);
        let stale: Vec<_> = lang_keys.difference(&en_keys).collect();

        if !stale.is_empty() {
            let file = path.file_name().unwrap().to_string_lossy();
            let keys_str: Vec<_> = stale.iter().map(|(s, k)| format!("[{s}] {k}")).collect();
            failures.push(format!(
                "{file}: {count} stale key(s) (not in en.ini):\n    {keys}",
                count = stale.len(),
                keys = keys_str.join("\n    ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Stale keys found in translation files:\n\n{}",
        failures.join("\n\n")
    );
}

/// Print a coverage report for all translation files.
///
/// This test always passes — it's informational. Missing keys are expected
/// for partial translations (the app falls back to English automatically).
#[test]
fn print_translation_coverage_report() {
    let en = load_ini(Path::new("assets/languages/en.ini"));
    let en_keys = extract_keys(&en);
    let total = en_keys.len();

    println!();
    println!("Translation Coverage Report");
    println!("============================");
    println!(
        "{:<16} {:>10} {:>6} {:>9}",
        "Language", "Translated", "Total", "Coverage"
    );
    println!(
        "{:<16} {:>10} {:>6} {:>9}",
        "--------", "----------", "-----", "--------"
    );

    let en_name = en
        .get("Meta", "NativeName")
        .unwrap_or_else(|| "English".to_string());
    println!(
        "{:<16} {:>10} {:>6} {:>8.1}%",
        format!("en ({})", en_name),
        total,
        total,
        100.0
    );

    for path in language_files() {
        let lang = load_ini(&path);
        let lang_keys = extract_keys(&lang);
        let translated = en_keys.intersection(&lang_keys).count();
        let coverage = if total > 0 {
            (translated as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let native = lang
            .get("Meta", "NativeName")
            .unwrap_or_else(|| stem.clone());

        let missing: Vec<_> = en_keys.difference(&lang_keys).collect();
        println!(
            "{:<16} {:>10} {:>6} {:>8.1}%",
            format!("{stem} ({native})"),
            translated,
            total,
            coverage
        );

        if !missing.is_empty() && missing.len() <= 20 {
            for (s, k) in &missing {
                println!("    missing: [{s}] {k}");
            }
        } else if !missing.is_empty() {
            println!("    {} missing keys (run with --nocapture to see full list)", missing.len());
        }
    }
    println!();
}
