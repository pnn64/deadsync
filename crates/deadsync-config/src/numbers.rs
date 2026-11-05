#[must_use]
pub fn parse_auto_threads_u8(raw: &str) -> Option<u8> {
    let raw = raw.trim();
    match raw.as_bytes() {
        [] => Some(0),
        [a] => {
            if a.is_ascii_digit() {
                Some(*a - b'0')
            } else {
                None
            }
        }
        [a, b, c] if a.is_ascii_digit() && b.is_ascii_digit() && c.is_ascii_digit() => {
            let value =
                u16::from(*a - b'0') * 100 + u16::from(*b - b'0') * 10 + u16::from(*c - b'0');
            u8::try_from(value).ok()
        }
        [_, _, _, _] if raw.eq_ignore_ascii_case("auto") => Some(0),
        _ => raw.parse::<u8>().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_threads_accepts_auto_and_empty_as_zero() {
        assert_eq!(parse_auto_threads_u8("auto"), Some(0));
        assert_eq!(parse_auto_threads_u8(" AUTO "), Some(0));
        assert_eq!(parse_auto_threads_u8(""), Some(0));
        assert_eq!(parse_auto_threads_u8("  "), Some(0));
    }

    #[test]
    fn auto_threads_parses_u8_values() {
        assert_eq!(parse_auto_threads_u8("1"), Some(1));
        assert_eq!(parse_auto_threads_u8("255"), Some(255));
        assert_eq!(parse_auto_threads_u8("256"), None);
        assert_eq!(parse_auto_threads_u8("many"), None);
    }
}
