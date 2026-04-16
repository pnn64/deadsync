//! Translation coverage CLI tool.
//!
//! Prints a human-readable report of translation progress for each language
//! file in `assets/languages/`, compared against the English baseline.
//!
//! Run with: `cargo run --bin lang_coverage`

use deadsync::config::SimpleIni;
use std::collections::BTreeSet;
use std::path::Path;

const SKIP_MARKER: &str = "@skip";
const BAR_WIDTH: usize = 20;

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

fn extract_covered_keys(ini: &SimpleIni) -> BTreeSet<(String, String)> {
    let mut keys = BTreeSet::new();
    for (section, props) in ini.sections() {
        if section == "Meta" {
            continue;
        }
        for (key, _) in props {
            keys.insert((section.clone(), key.clone()));
        }
    }
    keys
}

fn count_skipped(ini: &SimpleIni) -> usize {
    let mut count = 0;
    for (_section, props) in ini.sections() {
        for (_key, value) in props {
            if value.trim() == SKIP_MARKER {
                count += 1;
            }
        }
    }
    count
}

fn progress_bar(fraction: f64) -> String {
    let filled = (fraction * BAR_WIDTH as f64).round() as usize;
    let empty = BAR_WIDTH.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn load_ini(path: &Path) -> SimpleIni {
    let mut ini = SimpleIni::new();
    ini.load(path)
        .unwrap_or_else(|e| panic!("Failed to load {}: {e}", path.display()));
    ini
}

fn main() {
    let markdown_mode = std::env::args().any(|a| a == "--markdown");

    let lang_dir = Path::new("assets/languages");
    if !lang_dir.exists() {
        eprintln!("Error: assets/languages/ directory not found.");
        eprintln!("Run this tool from the repository root.");
        std::process::exit(1);
    }

    let en_path = lang_dir.join("en.ini");
    let en = load_ini(&en_path);
    let en_keys = extract_keys(&en);
    let total = en_keys.len();

    let mut lang_files: Vec<_> = std::fs::read_dir(lang_dir)
        .expect("Failed to read assets/languages/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ini"))
        .filter(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            stem != "en" && stem != "pseudo"
        })
        .collect();
    lang_files.sort();

    // Collect stats for each language
    let en_name = en
        .get("Meta", "NativeName")
        .unwrap_or_else(|| "English".to_string());

    let mut stats: Vec<LangStats> = Vec::new();
    for path in &lang_files {
        let lang = load_ini(path);
        let covered_keys = extract_covered_keys(&lang);
        let covered = en_keys.intersection(&covered_keys).count();
        let skipped = count_skipped(&lang);
        let missing: Vec<_> = en_keys
            .difference(&covered_keys)
            .cloned()
            .collect();
        let stale: Vec<_> = covered_keys
            .difference(&en_keys)
            .cloned()
            .collect();
        let code = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let native_name = lang
            .get("Meta", "NativeName")
            .unwrap_or_else(|| code.clone());

        stats.push(LangStats {
            code,
            native_name,
            covered,
            skipped,
            missing,
            stale,
        });
    }

    if markdown_mode {
        write_markdown(&en_name, total, &stats);
    } else {
        print_console(&en_name, total, &stats);
    }
}

struct LangStats {
    code: String,
    native_name: String,
    covered: usize,
    skipped: usize,
    missing: Vec<(String, String)>,
    stale: Vec<(String, String)>,
}

fn write_markdown(en_name: &str, total: usize, stats: &[LangStats]) {
    let out_path = Path::new("TRANSLATION_STATUS.md");
    let mut md = String::new();

    md.push_str("# Translation Status\n\n");
    md.push_str("| Language | Code | Covered | Total | Coverage |\n");
    md.push_str("|----------|------|--------:|------:|---------:|\n");
    md.push_str(&format!(
        "| {} | `en` | {} | {} | 100% |\n",
        en_name, total, total
    ));

    for s in stats {
        let pct = if total > 0 {
            (s.covered as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let skip_note = if s.skipped > 0 {
            format!(" ({} @skip)", s.skipped)
        } else {
            String::new()
        };
        md.push_str(&format!(
            "| {} | `{}` | {} | {} | {:.1}%{} |\n",
            s.native_name, s.code, s.covered, total, pct, skip_note
        ));
    }

    md.push_str("\n*Auto-generated by `cargo run --bin lang_coverage -- --markdown`. See `assets/languages/` to contribute.*\n");

    std::fs::write(out_path, &md).expect("Failed to write TRANSLATION_STATUS.md");
    println!("Wrote {}", out_path.display());
}

fn print_console(en_name: &str, total: usize, stats: &[LangStats]) {
    println!();
    println!("Translation Coverage Report");
    println!("============================");
    println!();
    println!(
        "{:<20} {:>10} {:>6} {:>9}  {}",
        "Language", "Covered", "Total", "Coverage", "Progress"
    );
    println!(
        "{:<20} {:>10} {:>6} {:>9}  {}",
        "--------", "-------", "-----", "--------", "--------"
    );

    println!(
        "{:<20} {:>10} {:>6} {:>8.1}%  {}",
        format!("en ({})", en_name),
        total,
        total,
        100.0,
        progress_bar(1.0),
    );

    for s in stats {
        let coverage = if total > 0 {
            s.covered as f64 / total as f64
        } else {
            0.0
        };
        let skip_note = if s.skipped > 0 {
            format!("  ({} @skip)", s.skipped)
        } else {
            String::new()
        };
        println!(
            "{:<20} {:>10} {:>6} {:>8.1}%  {}{}",
            format!("{} ({})", s.code, s.native_name),
            s.covered,
            total,
            coverage * 100.0,
            progress_bar(coverage),
            skip_note,
        );
    }

    println!();

    for s in stats {
        if s.missing.is_empty() && s.stale.is_empty() {
            continue;
        }

        println!("{}.ini:", s.code);

        if !s.stale.is_empty() {
            println!("  ⚠ {} stale key(s) (not in en.ini):", s.stale.len());
            for (sec, key) in &s.stale {
                println!("    [{sec}] {key}");
            }
        }

        if !s.missing.is_empty() {
            println!("  {} missing key(s):", s.missing.len());
            for (sec, key) in s.missing.iter().take(30) {
                println!("    [{sec}] {key}");
            }
            if s.missing.len() > 30 {
                println!("    ... and {} more", s.missing.len() - 30);
            }
        }

        println!();
    }
}
