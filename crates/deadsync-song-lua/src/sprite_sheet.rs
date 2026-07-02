#[inline(always)]
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

#[inline(always)]
fn is_res_tag(bytes: &[u8], idx: usize) -> bool {
    idx + 4 <= bytes.len()
        && bytes[idx] == b'('
        && bytes[idx + 1].eq_ignore_ascii_case(&b'r')
        && bytes[idx + 2].eq_ignore_ascii_case(&b'e')
        && bytes[idx + 3].eq_ignore_ascii_case(&b's')
}

#[inline(always)]
fn skip_parenthetical(bytes: &[u8], start: usize) -> usize {
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

#[inline(always)]
fn is_sprite_sheet_left_boundary(bytes: &[u8], left: usize) -> bool {
    left > 0 && matches!(bytes[left - 1], b' ' | b'\t' | b'\r' | b'\n' | b'_')
}

#[inline(always)]
fn is_sprite_sheet_right_boundary(bytes: &[u8], right: usize) -> bool {
    right == bytes.len()
        || matches!(
            bytes[right],
            b'.' | b' ' | b'\t' | b'\r' | b'\n' | b'(' | b'_'
        )
}

#[cfg(test)]
mod tests {
    use super::parse_sprite_sheet_dims;

    #[test]
    fn parses_itg_style_sprite_sheet_dims() {
        assert_eq!(parse_sprite_sheet_dims("grades/grades 1x19.png"), (1, 19));
        assert_eq!(
            parse_sprite_sheet_dims("_miso light 16x7 doubleres.png"),
            (16, 7)
        );
    }

    #[test]
    fn preserves_local_underscore_sprite_sheet_dims() {
        assert_eq!(
            parse_sprite_sheet_dims("submit/LoadingSpinner_10x3.png"),
            (10, 3)
        );
        assert_eq!(
            parse_sprite_sheet_dims("practice/note_field_bars_1x4_wrap.png"),
            (1, 4)
        );
    }

    #[test]
    fn ignores_resolution_labels_in_banner_names() {
        assert_eq!(
            parse_sprite_sheet_dims("1024x480-song-banner-background.png"),
            (1, 1)
        );
        assert_eq!(
            parse_sprite_sheet_dims("song-banner-1024x480-dimensions.png"),
            (1, 1)
        );
    }
}
