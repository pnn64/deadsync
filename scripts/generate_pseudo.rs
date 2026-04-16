//! Pseudo-localization generator.
//!
//! Reads `assets/languages/en.ini` and generates `assets/languages/pseudo.ini`
//! with every value wrapped in brackets and padded to simulate longer translations.
//!
//! Useful for:
//! - Verifying all strings go through `tr()` — any English text visible when
//!   pseudo is active is a hardcoded string that was missed.
//! - Testing layout with longer strings — the padding simulates German-length
//!   translations (~30% longer).
//!
//! Run with: `cargo run --bin generate_pseudo`

use deadsync::config::SimpleIni;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

/// Map ASCII letters to accented equivalents for visual distinction.
fn accent_char(c: char) -> char {
    match c {
        'A' => 'À', 'B' => 'Ɓ', 'C' => 'Ç', 'D' => 'Ð', 'E' => 'È',
        'F' => 'Ƒ', 'G' => 'Ĝ', 'H' => 'Ĥ', 'I' => 'Ì', 'J' => 'Ĵ',
        'K' => 'Ķ', 'L' => 'Ĺ', 'M' => 'Ṁ', 'N' => 'Ñ', 'O' => 'Ò',
        'P' => 'Þ', 'Q' => 'Ǫ', 'R' => 'Ŗ', 'S' => 'Š', 'T' => 'Ŧ',
        'U' => 'Ù', 'V' => 'Ṽ', 'W' => 'Ŵ', 'X' => 'Ẋ', 'Y' => 'Ŷ',
        'Z' => 'Ž',
        'a' => 'à', 'b' => 'ƀ', 'c' => 'ç', 'd' => 'ð', 'e' => 'è',
        'f' => 'ƒ', 'g' => 'ĝ', 'h' => 'ĥ', 'i' => 'ì', 'j' => 'ĵ',
        'k' => 'ķ', 'l' => 'ĺ', 'm' => 'ṁ', 'n' => 'ñ', 'o' => 'ò',
        'p' => 'þ', 'q' => 'ǫ', 'r' => 'ŗ', 's' => 'š', 't' => 'ŧ',
        'u' => 'ù', 'v' => 'ṽ', 'w' => 'ŵ', 'x' => 'ẋ', 'y' => 'ŷ',
        'z' => 'ž',
        other => other,
    }
}

/// Transform a value into pseudo-localized form.
///
/// - Replaces ASCII letters with accented equivalents
/// - Preserves `{placeholder}` tokens intact
/// - Wraps the result in brackets `[...]`
/// - Pads with `~` to simulate ~30% longer text
fn pseudolocalize(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    out.push('[');

    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Preserve {placeholder} tokens verbatim
        if chars[i] == '{' {
            if let Some(close) = chars[i..].iter().position(|&c| c == '}') {
                for &c in &chars[i..=i + close] {
                    out.push(c);
                }
                i += close + 1;
                continue;
            }
        }

        // Preserve \n escape sequences
        if chars[i] == '\\' && i + 1 < len && chars[i + 1] == 'n' {
            out.push_str("\\n");
            i += 2;
            continue;
        }

        // Preserve & glyph references like &START; &BACK;
        if chars[i] == '&' {
            if let Some(semi) = chars[i..].iter().position(|&c| c == ';') {
                for &c in &chars[i..=i + semi] {
                    out.push(c);
                }
                i += semi + 1;
                continue;
            }
        }

        out.push(accent_char(chars[i]));
        i += 1;
    }

    // Pad to simulate ~30% longer text
    let letter_count = value.chars().filter(|c| c.is_alphabetic()).count();
    let pad = (letter_count as f32 * 0.3).ceil() as usize;
    for _ in 0..pad {
        out.push('~');
    }

    out.push(']');
    out
}

fn main() {
    let en_path = Path::new("assets/languages/en.ini");
    if !en_path.exists() {
        eprintln!("Error: assets/languages/en.ini not found.");
        eprintln!("Run this tool from the repository root.");
        std::process::exit(1);
    }

    let mut ini = SimpleIni::new();
    ini.load(en_path).expect("Failed to load en.ini");

    // Collect into sorted structure for deterministic output
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (section, props) in ini.sections() {
        let entries = sections.entry(section.clone()).or_default();
        for (key, value) in props {
            entries.insert(key.clone(), value.clone());
        }
    }

    let mut output = String::new();
    writeln!(output, "; Auto-generated pseudo-localization file.").unwrap();
    writeln!(output, "; Run: cargo run --bin generate_pseudo").unwrap();
    writeln!(output, "; DO NOT EDIT — regenerate from en.ini instead.").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "[Meta]").unwrap();
    writeln!(output, "NativeName=[Pseudo]").unwrap();

    for (section, entries) in &sections {
        if section == "Meta" {
            continue;
        }
        writeln!(output).unwrap();
        writeln!(output, "[{section}]").unwrap();
        for (key, value) in entries {
            writeln!(output, "{key}={}", pseudolocalize(value)).unwrap();
        }
    }

    let out_path = Path::new("assets/languages/pseudo.ini");
    std::fs::write(out_path, &output).expect("Failed to write pseudo.ini");
    println!("Wrote {}", out_path.display());
}
