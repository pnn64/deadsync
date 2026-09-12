// Frozen from 453c5a21a (0.5.1141); only helper visibility is widened for tests.
const SONG_SEARCH_MAX_LEN: usize = 80;
use deadlib_assets::AssetManager;
use std::borrow::Cow;

const DIFFICULTY_WORDS: [&str; 8] = [
    "beginner",
    "easy",
    "medium",
    "hard",
    "challenge",
    "edit",
    "expert",
    "basic",
];

pub(super) fn strip_difficulty_parens(input: &str) -> Cow<'_, str> {
    if !input.contains('(') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('(') {
        let Some(close_rel) = rest[open + 1..].find(')') else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = rest[open + 1..close].trim();
        if DIFFICULTY_WORDS
            .iter()
            .any(|word| inner.eq_ignore_ascii_case(word))
        {
            out.push_str(&rest[..open]);
        } else {
            out.push_str(&rest[..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Cow::Owned(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

pub fn song_search_query_completed_with(query: &str, label: &str) -> String {
    let mut out = String::new();
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            continue;
        }
        let mut tail = chars.clone();
        let mut token = String::from('[');
        let mut has_digit = false;
        while let Some(&d) = tail.peek() {
            if !d.is_ascii_digit() {
                break;
            }
            has_digit = true;
            token.push(d);
            tail.next();
        }
        if has_digit && tail.peek() == Some(&']') {
            tail.next();
            token.push(']');
            out.push_str(&token);
            out.push(' ');
            chars = tail;
        }
    }
    out.push_str(label);
    out.chars().take(SONG_SEARCH_MAX_LEN).collect()
}

pub(super) fn song_search_fit(
    asset_manager: &AssetManager,
    text: &str,
    max_w: f32,
    zoom: f32,
) -> String {
    let width = |s: &str| -> f32 {
        let mut out = 0.0_f32;
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |font| {
                out = deadlib_present::font::measure_line_width_logical(font, s, all_fonts) as f32
                    * zoom;
            });
        });
        out
    };
    if !width(text).is_finite() || width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        if width(&candidate) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect()
}
