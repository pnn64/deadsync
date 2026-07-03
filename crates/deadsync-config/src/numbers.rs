pub fn parse_auto_threads_u8(raw: &str) -> Option<u8> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("auto") || raw.is_empty() {
        Some(0)
    } else {
        raw.parse::<u8>().ok()
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
