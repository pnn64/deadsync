use std::cmp::Ordering;

/// Compare names case-insensitively over ASCII bytes without allocating.
pub fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

/// Compare names using Rust's Unicode lowercase mapping without allocating.
pub fn unicode_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_matches_lowercase_sort_keys_without_allocating() {
        let names = ["Alpha", "alpha", "BETA", "zebra", "Äther", "äther", ""];

        for left in names {
            for right in names {
                assert_eq!(
                    ascii_case_insensitive_cmp(left, right),
                    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()),
                    "comparison changed for {left:?} and {right:?}"
                );
            }
        }
    }

    #[test]
    fn unicode_comparator_matches_lowercase_sort_keys_without_allocating() {
        let names = [
            "Alpha",
            "alpha",
            "İstanbul",
            "istanbul",
            "Σteps",
            "σteps",
            "ẞ",
            "ß",
            "",
        ];

        for left in names {
            for right in names {
                assert_eq!(
                    unicode_case_insensitive_cmp(left, right),
                    left.to_lowercase().cmp(&right.to_lowercase()),
                    "comparison changed for {left:?} and {right:?}"
                );
            }
        }
    }
}
