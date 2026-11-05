const PLAYER_INITIALS_MAX_LEN: usize = 4;
const ITGMANIA_GUID_NAMESPACE: uuid::Uuid =
    uuid::Uuid::from_u128(0x9d3f7c12_4b8e_5a96_b2d1_e7f4a06c83ddu128);
pub fn sanitize_player_initials(raw: &str) -> String {
    let mut out = String::with_capacity(PLAYER_INITIALS_MAX_LEN);
    for ch in raw.chars() {
        if out.len() >= PLAYER_INITIALS_MAX_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '?' || ch == '!' {
            out.push(ch.to_ascii_uppercase());
        }
    }
    out
}

pub fn is_valid_profile_guid(s: &str) -> bool {
    let groups = [8usize, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for &len in &groups {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != len || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
    }
    parts.next().is_none()
}

pub fn profile_guid_from_itgmania_guid(itg_guid: &str) -> Option<String> {
    let trimmed = itg_guid.trim();
    if trimmed.is_empty() {
        return None;
    }
    let derived = uuid::Uuid::new_v5(
        &ITGMANIA_GUID_NAMESPACE,
        trimmed.to_ascii_lowercase().as_bytes(),
    );
    Some(derived.to_string())
}
