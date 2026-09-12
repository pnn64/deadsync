//! Allocation-conscious title cleanup, completion assembly, and text fitting.
use super::SONG_SEARCH_MAX_LEN;
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

// Keep whitespace pending across removed groups so normalization matches
// concatenating the surviving spans and then split_whitespace().join(" ").
fn append_normalized(out: &mut String, span: &str, pending_space: &mut bool) {
    for ch in span.chars() {
        if ch.is_whitespace() {
            *pending_space = true;
        } else {
            if *pending_space && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            *pending_space = false;
        }
    }
}

/// Remove exact difficulty groups and normalize whitespace when parentheses occur.
pub(super) fn strip_difficulty_parens(input: &str) -> Cow<'_, str> {
    if !input.contains('(') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    let mut pending_space = false;
    while let Some(open) = rest.find('(') {
        let Some(close_rel) = rest[open + 1..].find(')') else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = rest[open + 1..close].trim();
        let end = if DIFFICULTY_WORDS
            .iter()
            .any(|word| inner.eq_ignore_ascii_case(word))
        {
            open
        } else {
            close + 1
        };
        append_normalized(&mut out, &rest[..end], &mut pending_space);
        rest = &rest[close + 1..];
    }
    append_normalized(&mut out, rest, &mut pending_space);
    Cow::Owned(out)
}

fn character_prefix(text: &str, count: usize) -> &str {
    let end = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(index, _)| index);
    &text[..end]
}

/// Replace free text with `label`, retaining valid `[###]` tokens verbatim and
/// capping the result at the input limit of 80 Unicode characters.
#[must_use]
pub fn song_search_query_completed_with(query: &str, label: &str) -> String {
    // The result is capped at 80 Unicode scalars, each at most four UTF-8 bytes.
    // Assemble it inline, then allocate only the final, exactly sized String.
    let mut buffer = [0u8; SONG_SEARCH_MAX_LEN * 4];
    let mut written = 0;
    let mut remaining = SONG_SEARCH_MAX_LEN;
    let bytes = query.as_bytes();
    let mut cursor = 0;
    while remaining > 0 {
        let Some(relative) = bytes[cursor..].iter().position(|&byte| byte == b'[') else {
            break;
        };
        let start = cursor + relative;
        let mut end = start + 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > start + 1 && bytes.get(end) == Some(&b']') {
            end += 1;
            let len = (end - start).min(remaining);
            buffer[written..written + len].copy_from_slice(&bytes[start..start + len]);
            written += len;
            remaining -= len;
            if remaining > 0 {
                buffer[written] = b' ';
                written += 1;
                remaining -= 1;
            }
            cursor = end;
        } else {
            // An invalid outer token can still contain a valid inner one.
            cursor = start + 1;
        }
    }
    let label = character_prefix(label, remaining);
    buffer[written..written + label.len()].copy_from_slice(label.as_bytes());
    written += label.len();
    String::from_utf8(buffer[..written].to_vec()).expect("tokens and label preserve UTF-8")
}

/// The same binary-search fitting decisions as before, using borrowed UTF-8
/// prefixes. Nonfinite widths and negative advances retain their old behavior.
pub(super) fn song_search_fit<'a>(
    asset_manager: &AssetManager,
    text: &'a str,
    max_w: f32,
    zoom: f32,
) -> &'a str {
    let fonts = asset_manager.fonts();
    let font = fonts.get("miso");
    let width = |text: &str| {
        font.map_or(0.0, |font| {
            deadlib_present::font::measure_line_width_logical(font, text, fonts) as f32 * zoom
        })
    };
    let full_width = width(text);
    if !full_width.is_finite() || full_width <= max_w {
        return text;
    }
    let (mut lo, mut hi) = (0, text.chars().count());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if width(character_prefix(text, mid)) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    character_prefix(text, lo)
}
