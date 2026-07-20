use std::cmp::Ordering;

/// Compare names case-insensitively over ASCII bytes without allocating.
pub fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
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
}
