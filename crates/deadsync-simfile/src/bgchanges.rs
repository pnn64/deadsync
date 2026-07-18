use std::borrow::Cow;

pub fn split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    let changes = strip_newlines(changes);
    split_bgchange_sets(&changes, entries, true)
}

fn split_bgchange_sets(
    changes: &str,
    entries: &[String],
    compact_field_growth: bool,
) -> Vec<Vec<String>> {
    if changes.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut start = 0usize;
    let mut pnum = 0u8;
    while start <= changes.len() {
        if matches!(pnum, 1 | 7)
            && let Some(found) = match_bgchange_entry(changes, start, entries)
        {
            push_bgchange_field(out.last_mut().unwrap(), found, compact_field_growth);
            start += found.len();
            if let Some(&delim) = changes.as_bytes().get(start) {
                pnum = if delim == b'=' { pnum + 1 } else { 0 };
                start += 1;
            }
            continue;
        }
        if pnum == 0 {
            out.push(if compact_field_growth {
                Vec::with_capacity(4)
            } else {
                Vec::new()
            });
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
            push_bgchange_field(
                out.last_mut().unwrap(),
                &changes[start..],
                compact_field_growth,
            );
            break;
        };
        push_bgchange_field(
            out.last_mut().unwrap(),
            &changes[start..end],
            compact_field_growth,
        );
        start = end + 1;
        pnum = next_pnum;
    }
    out
}

fn push_bgchange_field(fields: &mut Vec<String>, field: &str, compact_field_growth: bool) {
    if compact_field_growth && fields.len() == 4 && fields.capacity() == 4 {
        fields.reserve_exact(7);
    }
    fields.push(field.to_string());
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn split_bgchange_sets_legacy_for_bench(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    let changes = strip_newlines_owned(changes);
    split_bgchange_sets(&changes, entries, false)
}

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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn bgchange_field_rejects_non_media_legacy(field: &str) -> bool {
    let lower = field.to_ascii_lowercase();
    lower.contains(".ini") || lower.contains(".xml")
}

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
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn parse_bgchange_color_legacy(field: &str) -> Option<[f32; 4]> {
    let field = field.trim().replace('^', ",");
    if field.is_empty() {
        return None;
    }
    if let Some(hex) = field.strip_prefix('#')
        && matches!(hex.len(), 6 | 8)
    {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0
        } else {
            1.0
        };
        return Some([r, g, b, a]);
    }
    let parts = field
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [r, g, b] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            1.0,
        ]),
        [r, g, b, a] => Some([
            r.parse::<f32>().ok()?,
            g.parse::<f32>().ok()?,
            b.parse::<f32>().ok()?,
            a.parse::<f32>().ok()?,
        ]),
        _ => None,
    }
}

fn match_bgchange_entry<'a>(changes: &'a str, start: usize, entries: &[String]) -> Option<&'a str> {
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

fn strip_newlines(text: &str) -> Cow<'_, str> {
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
