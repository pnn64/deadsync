pub fn version_parts(version: &str) -> [i64; 3] {
    let mut parts = [0_i64; 3];
    for (index, part) in version.trim().split('.').take(3).enumerate() {
        let digits = part.bytes().take_while(u8::is_ascii_digit).count();
        parts[index] = part[..digits].parse::<i64>().unwrap_or(0);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parts_parse_prefix_digits() {
        assert_eq!(version_parts("5.1.0-beta"), [5, 1, 0]);
        assert_eq!(version_parts(" 4.2 "), [4, 2, 0]);
        assert_eq!(version_parts("12.é.3"), [12, 0, 3]);
        assert_eq!(version_parts("999999999999999999999999.1"), [0, 1, 0]);
        assert_eq!(version_parts("main"), [0, 0, 0]);
    }
}
