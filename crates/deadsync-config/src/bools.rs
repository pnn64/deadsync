#[must_use]
pub fn parse_bool_str(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    if matches!(raw, "1")
        || raw.eq_ignore_ascii_case("true")
        || raw.eq_ignore_ascii_case("yes")
        || raw.eq_ignore_ascii_case("on")
    {
        Some(true)
    } else if matches!(raw, "0")
        || raw.eq_ignore_ascii_case("false")
        || raw.eq_ignore_ascii_case("no")
        || raw.eq_ignore_ascii_case("off")
    {
        Some(false)
    } else {
        None
    }
}

#[must_use]
pub fn parse_loose_bool_str(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    parse_bool_str(raw).or_else(|| raw.parse::<u8>().ok().map(|n| n != 0))
}

#[must_use]
pub fn parse_u8_bool_str(raw: &str) -> Option<bool> {
    raw.trim().parse::<u8>().ok().map(|n| n != 0)
}

pub fn parse_u8_bool_or_default(raw: Option<&str>, default: bool) -> bool {
    raw.and_then(parse_u8_bool_str).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_bool_values() {
        for raw in ["1", "true", "TRUE", " true ", " yes ", "YeS", "on", "oN"] {
            assert_eq!(parse_bool_str(raw), Some(true));
        }
        for raw in [
            "0", "false", "FALSE", " False ", " no ", "\tno\r\n", "off", "OFF",
        ] {
            assert_eq!(parse_bool_str(raw), Some(false));
        }
    }

    #[test]
    fn rejects_unknown_bool_values() {
        assert_eq!(parse_bool_str(""), None);
        assert_eq!(parse_bool_str("2"), None);
        assert_eq!(parse_bool_str("maybe"), None);
        assert_eq!(parse_bool_str("truth"), None);
        assert_eq!(parse_bool_str("trüe"), None);
        assert_eq!(parse_bool_str(" true! "), None);
    }

    #[test]
    fn parses_numeric_loose_values() {
        assert_eq!(parse_loose_bool_str("2"), Some(true));
        assert_eq!(parse_loose_bool_str("255"), Some(true));
        assert_eq!(parse_loose_bool_str("256"), None);
        assert_eq!(parse_loose_bool_str(""), None);
    }

    #[test]
    fn parses_u8_bool_values() {
        assert_eq!(parse_u8_bool_str("0"), Some(false));
        assert_eq!(parse_u8_bool_str("1"), Some(true));
        assert_eq!(parse_u8_bool_str("255"), Some(true));
        assert_eq!(parse_u8_bool_str("256"), None);
        assert_eq!(parse_u8_bool_str("true"), None);
        assert!(parse_u8_bool_or_default(Some("bad"), true));
        assert!(!parse_u8_bool_or_default(None, false));
    }
}
