//! Frozen filename metadata routines from 434ea40298d3339611ebf4d197a6670f3f8247ba.

fn has_ascii_case_insensitive_substr(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn parse_ascii_digits(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(value)
}

const fn skip_parenthetical(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < bytes.len() {
        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return idx + 1;
                }
                depth -= 1;
                if depth == 0 {
                    return idx + 1;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    bytes.len()
}

fn trim_ascii_ws(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |idx| idx + 1);
    &bytes[start..end]
}

fn parse_res_dims(section: &[u8]) -> Option<(u32, u32)> {
    let mut scan = 0usize;
    while scan + 4 <= section.len() {
        if !section[scan..scan + 4].eq_ignore_ascii_case(b"res ") {
            scan += 1;
            continue;
        }

        let mut width_start = scan + 4;
        while width_start < section.len() && section[width_start].is_ascii_whitespace() {
            width_start += 1;
        }

        let Some(x_rel) = section[width_start..]
            .iter()
            .position(|b| matches!(*b, b'x' | b'X'))
        else {
            break;
        };
        let x_idx = width_start + x_rel;
        let width = parse_ascii_digits(trim_ascii_ws(&section[width_start..x_idx]));

        let mut height_end = x_idx + 1;
        while height_end < section.len() && section[height_end].is_ascii_digit() {
            height_end += 1;
        }
        let height = parse_ascii_digits(&section[x_idx + 1..height_end]);

        if let (Some(width), Some(height)) = (width, height)
            && width > 0
            && height > 0
        {
            return Some((width, height));
        }

        scan = height_end.max(width_start + 1);
    }
    None
}

pub fn parse_texture_resolution_hint(raw: &str) -> Option<(u32, u32)> {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'(' {
            i += 1;
            continue;
        }
        let end = skip_parenthetical(bytes, i);
        if end <= i + 2 {
            i = end.max(i + 1);
            continue;
        }
        if let Some(dims) = parse_res_dims(&bytes[i + 1..end - 1]) {
            return Some(dims);
        }
        i = end.max(i + 1);
    }
    None
}

const fn is_res_tag(bytes: &[u8], idx: usize) -> bool {
    idx + 4 <= bytes.len()
        && bytes[idx] == b'('
        && bytes[idx + 1].eq_ignore_ascii_case(&b'r')
        && bytes[idx + 2].eq_ignore_ascii_case(&b'e')
        && bytes[idx + 3].eq_ignore_ascii_case(&b's')
}

pub fn parse_sprite_sheet_dims(filename: &str) -> (u32, u32) {
    let bytes = filename.as_bytes();
    let mut dims: Option<(u32, u32)> = None;
    let mut i = 0usize;

    while i < bytes.len() {
        if is_res_tag(bytes, i) {
            i = skip_parenthetical(bytes, i);
            continue;
        }

        let b = bytes[i];
        if (b == b'x' || b == b'X') && i > 0 && bytes[i - 1].is_ascii_digit() {
            let mut left = i;
            while left > 0 && bytes[left - 1].is_ascii_digit() {
                left -= 1;
            }

            let mut right = i + 1;
            while right < bytes.len() && bytes[right].is_ascii_digit() {
                right += 1;
            }

            if left < i
                && i + 1 < right
                && is_sprite_sheet_left_boundary(bytes, left)
                && is_sprite_sheet_right_boundary(bytes, right)
                && let (Some(w), Some(h)) = (
                    parse_ascii_digits(&bytes[left..i]),
                    parse_ascii_digits(&bytes[i + 1..right]),
                )
                && w > 0
                && h > 0
            {
                dims = Some((w, h));
            }

            i = right;
            continue;
        }

        i += 1;
    }

    dims.unwrap_or((1, 1))
}

const fn is_sprite_sheet_left_boundary(bytes: &[u8], left: usize) -> bool {
    left > 0 && matches!(bytes[left - 1], b' ' | b'\t' | b'\r' | b'\n' | b'_')
}

const fn is_sprite_sheet_right_boundary(bytes: &[u8], right: usize) -> bool {
    right == bytes.len()
        || matches!(
            bytes[right],
            b'.' | b' ' | b'\t' | b'\r' | b'\n' | b'(' | b'_'
        )
}

pub fn texture_hint_doubleres(raw: &str) -> bool {
    has_ascii_case_insensitive_substr(raw.trim().as_bytes(), b"doubleres")
}
