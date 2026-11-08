pub fn bgchange_field_rejects_non_media(field: &str) -> bool {
    field
        .as_bytes()
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b".ini") || window.eq_ignore_ascii_case(b".xml"))
}
