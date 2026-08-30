#[cfg(any(test, feature = "bench-support"))]
use std::borrow::Cow;

#[must_use]
pub fn split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    split_bgchange_sets(changes, entries)
}

fn split_bgchange_sets(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    if changes.is_empty() {
        return Vec::new();
    }
    let mut content_start = 0;
    skip_newline_sequence(changes.as_bytes(), &mut content_start);
    if content_start == changes.len() {
        return Vec::new();
    }
    let set_capacity = changes
        .as_bytes()
        .iter()
        .filter(|&&byte| byte == b',')
        .count()
        .saturating_add(1);
    let mut out = Vec::with_capacity(set_capacity);
    let mut start = 0usize;
    let mut pnum = 0u8;
    while start <= changes.len() {
        if matches!(pnum, 1 | 7)
            && let Some(end) = match_bgchange_entry_end(changes, start, entries)
        {
            push_bgchange_field_range(out.last_mut().unwrap(), changes, start, end);
            start = end;
            if let Some(&delim) = changes.as_bytes().get(start) {
                pnum = if delim == b'=' { pnum + 1 } else { 0 };
                start += 1;
            }
            continue;
        }
        if pnum == 0 {
            out.push(Vec::with_capacity(4));
        }
        let Some((end, delim)) = next_bgchange_delimiter(changes, start) else {
            push_bgchange_field_range(out.last_mut().unwrap(), changes, start, changes.len());
            break;
        };
        push_bgchange_field_range(out.last_mut().unwrap(), changes, start, end);
        start = end + 1;
        pnum = if delim == b'=' { pnum + 1 } else { 0 };
    }
    out
}

#[inline]
fn next_bgchange_delimiter(changes: &str, start: usize) -> Option<(usize, u8)> {
    changes.as_bytes()[start..]
        .iter()
        .position(|byte| matches!(byte, b'=' | b','))
        .map(|offset| {
            let index = start + offset;
            (index, changes.as_bytes()[index])
        })
}

fn push_bgchange_field_range(fields: &mut Vec<String>, changes: &str, start: usize, end: usize) {
    if fields.len() == 4 && fields.capacity() == 4 {
        fields.reserve_exact(7);
    }
    let field = &changes[start..end];
    fields.push(if field.as_bytes().contains(&b'\n') {
        strip_newlines_owned(field)
    } else {
        field.to_string()
    });
}

fn match_bgchange_entry_end(changes: &str, start: usize, entries: &[String]) -> Option<usize> {
    entries
        .iter()
        .find_map(|entry| match_bgchange_entry_end_one(changes, start, entry))
}

fn match_bgchange_entry_end_one(changes: &str, start: usize, entry: &str) -> Option<usize> {
    let input = changes.as_bytes();
    let mut input_index = start;
    for &expected in entry.as_bytes() {
        skip_newline_sequence(input, &mut input_index);
        let actual = *input.get(input_index)?;
        if !actual.eq_ignore_ascii_case(&expected) {
            return None;
        }
        input_index += 1;
    }
    skip_newline_sequence(input, &mut input_index);
    matches!(input.get(input_index), None | Some(b'=') | Some(b',')).then_some(input_index)
}

#[inline]
fn skip_newline_sequence(input: &[u8], index: &mut usize) {
    loop {
        match input.get(*index..) {
            Some([b'\r', b'\n', ..]) => *index += 2,
            Some([b'\n', ..]) => *index += 1,
            _ => break,
        }
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn committed_split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    let changes = committed_strip_newlines(changes);
    committed_split_bgchange_sets(&changes, entries)
}

#[cfg(any(test, feature = "bench-support"))]
fn committed_split_bgchange_sets(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    if changes.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut start = 0usize;
    let mut pnum = 0u8;
    while start <= changes.len() {
        if matches!(pnum, 1 | 7)
            && let Some(found) = committed_match_bgchange_entry(changes, start, entries)
        {
            committed_push_bgchange_field(out.last_mut().unwrap(), found);
            start += found.len();
            if let Some(&delim) = changes.as_bytes().get(start) {
                pnum = if delim == b'=' { pnum + 1 } else { 0 };
                start += 1;
            }
            continue;
        }
        if pnum == 0 {
            out.push(Vec::with_capacity(4));
        }
        let rem = &changes[start..];
        let eq = rem.find('=').map(|i| start + i);
        let comma = rem.find(',').map(|i| start + i);
        let Some((end, next_pnum)) = eq
            .zip(comma)
            .map(|(e, c)| if e < c { (e, pnum + 1) } else { (c, 0) })
            .or_else(|| eq.map(|e| (e, pnum + 1)))
            .or_else(|| comma.map(|c| (c, 0)))
        else {
            committed_push_bgchange_field(out.last_mut().unwrap(), &changes[start..]);
            break;
        };
        committed_push_bgchange_field(out.last_mut().unwrap(), &changes[start..end]);
        start = end + 1;
        pnum = next_pnum;
    }
    out
}

#[cfg(any(test, feature = "bench-support"))]
fn committed_push_bgchange_field(fields: &mut Vec<String>, field: &str) {
    if fields.len() == 4 && fields.capacity() == 4 {
        fields.reserve_exact(7);
    }
    fields.push(field.to_string());
}

#[cfg(feature = "bench-support")]
pub mod bench_support {
    use super::{committed_split_bgchange_sets_like_itg, split_bgchange_sets_like_itg};

    #[must_use]
    pub fn split_bgchange_sets_old(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
        committed_split_bgchange_sets_like_itg(changes, entries)
    }

    #[must_use]
    pub fn split_bgchange_sets_new(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
        split_bgchange_sets_like_itg(changes, entries)
    }
}

#[must_use]
pub fn bgchange_field_rejects_non_media(field: &str) -> bool {
    contains_ignore_ascii_case(field, ".ini") || contains_ignore_ascii_case(field, ".xml")
}

#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[must_use]
pub fn parse_bgchange_rate(field: Option<&str>) -> f32 {
    match field {
        Some(field) => field.trim().parse::<f32>().unwrap_or(0.0),
        None => 1.0,
    }
}

pub fn parse_bgchange_transition(crossfade: Option<&str>, explicit: Option<&str>) -> String {
    let transition = explicit.map(str::trim).unwrap_or("");
    if !transition.is_empty() {
        transition.to_string()
    } else if parse_bgchange_int(crossfade) != 0 {
        "CrossFade".to_string()
    } else {
        String::new()
    }
}

pub fn parse_bgchange_effect(
    rewind_movie: Option<&str>,
    loop_movie: Option<&str>,
    explicit: Option<&str>,
) -> String {
    let effect = explicit.map(str::trim).unwrap_or("");
    if !effect.is_empty() {
        return effect.to_string();
    }
    if loop_movie.is_some() && parse_bgchange_int(loop_movie) == 0 {
        return "StretchNoLoop".to_string();
    }
    if parse_bgchange_int(rewind_movie) != 0 {
        return "StretchRewind".to_string();
    }
    String::new()
}

pub fn parse_bgchange_color(field: &str) -> Option<[f32; 4]> {
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    if let Some(hex) = field.strip_prefix('#')
        && matches!(hex.len(), 6 | 8)
    {
        let r = f32::from(u8::from_str_radix(&hex[0..2], 16).ok()?) / 255.0;
        let g = f32::from(u8::from_str_radix(&hex[2..4], 16).ok()?) / 255.0;
        let b = f32::from(u8::from_str_radix(&hex[4..6], 16).ok()?) / 255.0;
        let a = if hex.len() == 8 {
            f32::from(u8::from_str_radix(&hex[6..8], 16).ok()?) / 255.0
        } else {
            1.0
        };
        return Some([r, g, b, a]);
    }
    let mut parts = field
        .split([',', '^'])
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let red = parts.next()?.parse::<f32>().ok()?;
    let green = parts.next()?.parse::<f32>().ok()?;
    let blue = parts.next()?.parse::<f32>().ok()?;
    let alpha = match parts.next() {
        Some(alpha) => alpha.parse::<f32>().ok()?,
        None => 1.0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some([red, green, blue, alpha])
}

#[cfg(any(test, feature = "bench-support"))]
fn committed_match_bgchange_entry<'a>(
    changes: &'a str,
    start: usize,
    entries: &[String],
) -> Option<&'a str> {
    for entry in entries {
        let Some(head) = changes.get(start..start + entry.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(entry) {
            continue;
        }
        let next = start + entry.len();
        if matches!(changes.as_bytes().get(next), None | Some(b'=') | Some(b',')) {
            return Some(head);
        }
    }
    None
}

#[cfg(any(test, feature = "bench-support"))]
fn committed_strip_newlines(text: &str) -> Cow<'_, str> {
    if !text.contains('\n') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(strip_newlines_owned(text))
}

fn strip_newlines_owned(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(line);
    }
    out
}

fn parse_bgchange_int(field: Option<&str>) -> i32 {
    field
        .map(|field| field.trim().parse::<i32>().unwrap_or(0))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_bgchange_sets() {
        let sets = split_bgchange_sets_like_itg("0=movie.mp4=1,8=other.mp4=2", &[]);
        assert_eq!(
            sets,
            vec![
                vec!["0".to_string(), "movie.mp4".to_string(), "1".to_string()],
                vec!["8".to_string(), "other.mp4".to_string(), "2".to_string()],
            ]
        );
    }

    #[test]
    fn preserves_entry_names_with_delimiters() {
        let entries = vec!["movie,part.mp4".to_string(), "layer=alt.png".to_string()];
        let sets = split_bgchange_sets_like_itg(
            "0=movie,part.mp4=1=0=0=0=0=layer=alt.png=CrossFade",
            &entries,
        );
        assert_eq!(
            sets,
            vec![vec![
                "0".to_string(),
                "movie,part.mp4".to_string(),
                "1".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "layer=alt.png".to_string(),
                "CrossFade".to_string(),
            ]]
        );
    }

    #[test]
    fn strips_line_breaks_before_splitting() {
        let sets = split_bgchange_sets_like_itg("0=\nmovie.mp4=1", &[]);
        assert_eq!(
            sets,
            vec![vec![
                "0".to_string(),
                "movie.mp4".to_string(),
                "1".to_string()
            ]]
        );
    }

    #[test]
    fn optimized_splitter_matches_committed_reference() {
        let entries = vec![
            "movie,part.mp4".to_string(),
            "layer=alt.png".to_string(),
            "movie.mp4".to_string(),
            "\u{65e5}\u{672c},\u{80cc}\u{666f}.png".to_string(),
        ];
        let cases = [
            "",
            "0=movie.mp4=1",
            "0=movie.mp4=1,",
            ",0=movie.mp4=1",
            "0=movie,part.mp4=1=0=0=0=0=layer=alt.png=CrossFade",
            "0=mov\nie.mp4=1",
            "0=movie.mp4\n=1",
            "0=movie.mp4\r\n=1,8=layer\n=alt.png=2",
            "0=movie.mp4\r=1",
            "0=\u{65e5}\u{672c},\u{80cc}\u{666f}.png=1",
            "0===,,,8=other.png=2",
            "\n\n0\n=\nmovie.mp4\n=\n1\n",
        ];

        for changes in cases {
            assert_eq!(
                split_bgchange_sets_like_itg(changes, &entries),
                committed_split_bgchange_sets_like_itg(changes, &entries),
                "BG-change split diverged for {changes:?}"
            );
        }
    }

    #[test]
    fn optimized_splitter_exhaustively_matches_fragments() {
        let entries = vec![
            "movie,part.mp4".to_string(),
            "layer=alt.png".to_string(),
            "movie.mp4".to_string(),
        ];
        let fragments = [
            "",
            "0",
            "=",
            ",",
            "\n",
            "\r\n",
            "movie.mp4",
            "movie,part.mp4",
            "layer=alt.png",
            "1=0",
            "\u{e9}",
        ];

        for first in fragments {
            for second in fragments {
                for third in fragments {
                    let changes = format!("{first}{second}{third}");
                    assert_eq!(
                        split_bgchange_sets_like_itg(&changes, &entries),
                        committed_split_bgchange_sets_like_itg(&changes, &entries),
                        "BG-change split diverged for {changes:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn parses_bgchange_rate_defaults_and_invalid_values() {
        assert_eq!(parse_bgchange_rate(None), 1.0);
        assert_eq!(parse_bgchange_rate(Some(" 1.5 ")), 1.5);
        assert_eq!(parse_bgchange_rate(Some("bad")), 0.0);
    }

    #[test]
    fn parses_transition_from_explicit_or_crossfade_flag() {
        assert_eq!(
            parse_bgchange_transition(Some("0"), Some(" FadeRight ")),
            "FadeRight"
        );
        assert_eq!(parse_bgchange_transition(Some("1"), Some("")), "CrossFade");
        assert_eq!(parse_bgchange_transition(Some("0"), None), "");
    }

    #[test]
    fn parses_effect_from_explicit_or_legacy_flags() {
        assert_eq!(
            parse_bgchange_effect(Some("0"), Some("1"), Some(" SongBgWithMovieViz ")),
            "SongBgWithMovieViz"
        );
        assert_eq!(
            parse_bgchange_effect(Some("0"), Some("0"), None),
            "StretchNoLoop"
        );
        assert_eq!(
            parse_bgchange_effect(Some("1"), None, None),
            "StretchRewind"
        );
        assert_eq!(parse_bgchange_effect(Some("0"), None, None), "");
    }

    #[test]
    fn parses_bgchange_colors() {
        assert_eq!(
            parse_bgchange_color("#ff8000"),
            Some([1.0, 128.0 / 255.0, 0.0, 1.0])
        );
        assert_eq!(
            parse_bgchange_color("0.5^0.25^1^0.75"),
            Some([0.5, 0.25, 1.0, 0.75])
        );
        assert_eq!(parse_bgchange_color("1,0,0"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_bgchange_color("bad"), None);
    }

    #[test]
    fn rejects_non_media_bgchange_fields() {
        assert!(bgchange_field_rejects_non_media("Theme/default.xml"));
        assert!(bgchange_field_rejects_non_media("config.INI"));
        assert!(!bgchange_field_rejects_non_media("movie.mp4"));
    }
}
