use rssp::parse::{decode_bytes, unescape_tag};

pub fn latest_simfile_tag_value(simfile_data: &[u8], tag: &[u8]) -> String {
    latest_simfile_tag_values(simfile_data, [tag])
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Extracts the latest value for each requested tag in one pass over a simfile.
pub fn latest_simfile_tag_values<const N: usize>(
    simfile_data: &[u8],
    tags: [&[u8]; N],
) -> [String; N] {
    let mut latest = [None; N];
    let mut i = 0usize;
    while i < simfile_data.len() {
        let Some(pos) = find_byte(&simfile_data[i..], b'#') else {
            break;
        };
        i += pos;
        let slice = &simfile_data[i..];
        let Some((tag_index, tag)) = tags
            .iter()
            .copied()
            .enumerate()
            .find(|(_, tag)| starts_with_ci(slice, tag))
        else {
            i += 1;
            continue;
        };
        if let Some((value, adv)) = parse_tag_val(slice, tag.len(), true) {
            latest[tag_index] = Some(value);
            i += adv;
        } else {
            i += 1;
        }
    }

    std::array::from_fn(|index| {
        latest[index]
            .map(|raw| unescape_tag(decode_bytes(raw).as_ref()).into_owned())
            .unwrap_or_default()
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn latest_simfile_tag_value_legacy(simfile_data: &[u8], tag: &[u8]) -> String {
    extract_named_tag_values(simfile_data, &[tag])
        .last()
        .copied()
        .map(|raw| unescape_tag(decode_bytes(raw).as_ref()).into_owned())
        .unwrap_or_default()
}

pub fn extract_named_tag_values<'a>(data: &'a [u8], tags: &[&[u8]]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let Some(pos) = find_byte(&data[i..], b'#') else {
            break;
        };
        i += pos;
        let slice = &data[i..];
        let Some(tag) = tags.iter().copied().find(|tag| starts_with_ci(slice, tag)) else {
            i += 1;
            continue;
        };
        if let Some((value, adv)) = parse_tag_val(slice, tag.len(), true) {
            out.push(value);
            i += adv;
        } else {
            i += 1;
        }
    }
    out
}

#[inline(always)]
fn starts_with_ci(slice: &[u8], tag: &[u8]) -> bool {
    slice
        .get(..tag.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(tag))
}

#[inline(always)]
fn find_byte(slice: &[u8], needle: u8) -> Option<usize> {
    let mut i = 0usize;
    while i < slice.len() {
        if slice[i] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn find_either_byte(slice: &[u8], a: u8, b: u8) -> Option<usize> {
    let mut i = 0usize;
    while i < slice.len() {
        if slice[i] == a || slice[i] == b {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn find_unescaped_semi_no_hash(slice: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    let mut has_hash = false;
    while off < slice.len() {
        let rel = find_either_byte(&slice[off..], b';', b'#')?;
        let idx = off + rel;
        if slice[idx] == b'#' {
            has_hash = true;
            off = idx + 1;
            continue;
        }
        let mut bs = 0usize;
        let mut i = idx;
        while i > 0 && slice[i - 1] == b'\\' {
            bs += 1;
            i -= 1;
        }
        if bs & 1 == 0 {
            return (!has_hash).then_some(idx);
        }
        off = idx + 1;
    }
    None
}

#[inline(always)]
fn scan_tag_end(slice: &[u8], allow_nl: bool) -> Option<(usize, usize)> {
    if allow_nl && let Some(end) = find_unescaped_semi_no_hash(slice) {
        return Some((end, end + 1));
    }

    let mut i = 0usize;
    let mut bs_odd = false;
    while i < slice.len() {
        let b = slice[i];
        if b == b'\\' {
            bs_odd = !bs_odd;
            i += 1;
            continue;
        }
        let escaped = bs_odd;
        bs_odd = false;
        if b == b';' {
            if !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }
        if b == b':' {
            if !allow_nl && !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }
        if matches!(b, b'\n' | b'\r') {
            let mut j = i + 1;
            if b == b'\r' && slice.get(j) == Some(&b'\n') {
                j += 1;
            }
            while j < slice.len()
                && slice[j].is_ascii_whitespace()
                && !matches!(slice[j], b'\n' | b'\r')
            {
                j += 1;
            }
            if slice.get(j) == Some(&b'#') {
                return Some((i, j));
            }
            if !allow_nl && slice.get(j) != Some(&b';') {
                return None;
            }
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn parse_tag_val(data: &[u8], tag_len: usize, allow_nl: bool) -> Option<(&[u8], usize)> {
    let slice = data.get(tag_len..)?;
    let (end, next) = scan_tag_end(slice, allow_nl)?;
    Some((&slice[..end], tag_len + next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_case_insensitive_duplicate_tags() {
        let data = b"ignored#TITLE:One;#artist:DJ;#title:Two;";
        let values = extract_named_tag_values(data, &[b"#TITLE:"]);
        assert_eq!(values, vec![b"One".as_slice(), b"Two".as_slice()]);
    }

    #[test]
    fn extracts_escaped_semicolon_value() {
        let data = b"#TITLE:One\\;Two;#SUBTITLE:x;";
        let values = extract_named_tag_values(data, &[b"#TITLE:"]);
        assert_eq!(values, vec![b"One\\;Two".as_slice()]);
    }

    #[test]
    fn latest_tag_value_uses_last_decoded_value() {
        let data = b"#CDIMAGE:old.png;#cdimage:new\\;image.png;";
        assert_eq!(
            latest_simfile_tag_value(data, b"#CDIMAGE:"),
            "new;image.png"
        );
    }

    #[test]
    fn latest_tag_values_batch_independent_duplicate_tags() {
        let data = b"#CDIMAGE:old.png;#DISCIMAGE:disc.png;#cdimage:new\\;image.png;";
        let [cdimage, discimage] =
            latest_simfile_tag_values(data, [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()]);

        assert_eq!(cdimage, "new;image.png");
        assert_eq!(discimage, "disc.png");
    }
}
