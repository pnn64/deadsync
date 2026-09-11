use smallvec::SmallVec;
use std::borrow::Cow;

#[must_use]
pub fn split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    let sets = bgchange_sets(changes, entries);
    if sets.done {
        return Vec::new();
    }
    let capacity = changes.bytes().filter(|&byte| byte == b',').count() + 1;
    let mut out = Vec::with_capacity(capacity);
    out.extend(sets.map(|fields| fields.into_iter().map(Cow::into_owned).collect()));
    out
}

/// A change has eleven standard fields. Only newline removal or wider legacy
/// records need heap storage; ordinary fields borrow the decoded tag text.
pub(crate) type BgChangeFields<'a> = SmallVec<[Cow<'a, str>; 11]>;

/// Stream one change at a time so consumers can parse borrowed fields and stop
/// early without allocating strings or a collection for the rest of the tag.
pub(crate) fn bgchange_sets<'a>(changes: &'a str, entries: &'a [String]) -> BgChangeSets<'a> {
    let mut content_start = 0;
    skip_newline_sequence(changes.as_bytes(), &mut content_start);
    BgChangeSets {
        changes,
        entries,
        start: 0,
        done: content_start == changes.len(),
    }
}

pub(crate) struct BgChangeSets<'a> {
    changes: &'a str,
    entries: &'a [String],
    start: usize,
    done: bool,
}

impl<'a> Iterator for BgChangeSets<'a> {
    type Item = BgChangeFields<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut fields = BgChangeFields::new();
        let mut pnum = 0u8;
        loop {
            let matched_entry_end = matches!(pnum, 1 | 7)
                .then(|| match_bgchange_entry_end(self.changes, self.start, self.entries))
                .flatten();
            let (end, delimiter) = if let Some(end) = matched_entry_end {
                (end, self.changes.as_bytes().get(end).copied())
            } else if let Some((end, delimiter)) = next_bgchange_delimiter(self.changes, self.start)
            {
                (end, Some(delimiter))
            } else {
                fields.push(bgchange_field_range(
                    self.changes,
                    self.start,
                    self.changes.len(),
                ));
                self.done = true;
                return Some(fields);
            };
            fields.push(bgchange_field_range(self.changes, self.start, end));
            self.start = end;
            if let Some(delimiter) = delimiter {
                self.start += 1;
                if delimiter == b',' {
                    return Some(fields);
                }
                pnum += 1;
                if pnum == 0 {
                    // Preserve the legacy u8 field counter's release-mode
                    // wrap: it starts another set even without a comma.
                    return Some(fields);
                }
            }
            // An entry ending exactly at EOF follows the legacy parser's next
            // iteration, which retains the final empty field.
        }
    }
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

fn bgchange_field_range(changes: &str, start: usize, end: usize) -> Cow<'_, str> {
    let field = &changes[start..end];
    if field.as_bytes().contains(&b'\n') {
        Cow::Owned(strip_newlines_owned(field))
    } else {
        Cow::Borrowed(field)
    }
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
