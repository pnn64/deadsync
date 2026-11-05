#[must_use]
pub fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().as_bytes() {
        [b'1'] => Some(true),
        [b'0'] => Some(false),
        [a, b] => {
            const ON: u16 = u16::from_le_bytes(*b"on");
            const NO: u16 = u16::from_le_bytes(*b"no");
            match u16::from_le_bytes([*a, *b]) | 0x2020 {
                ON => Some(true),
                NO => Some(false),
                _ => None,
            }
        }
        [a, b, c] => {
            let key = [*a | 0x20, *b | 0x20, *c | 0x20];
            if key == *b"yes" {
                Some(true)
            } else if key == *b"off" {
                Some(false)
            } else {
                None
            }
        }
        [a, b, c, d]
            if u32::from_le_bytes([*a, *b, *c, *d]) | 0x20202020
                == u32::from_le_bytes(*b"true") =>
        {
            Some(true)
        }
        [a, b, c, d, e]
            if u32::from_le_bytes([*a, *b, *c, *d]) | 0x20202020
                == u32::from_le_bytes(*b"fals")
                && (*e | 0x20) == b'e' =>
        {
            Some(false)
        }
        _ => None,
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
