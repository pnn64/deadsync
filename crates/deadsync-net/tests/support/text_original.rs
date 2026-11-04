//! Frozen production functions from bda7ac85e.
const MAX_LOG_CHARS: usize = 256;

#[inline(always)]
pub fn is_timeout_message(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut offset = 0;
    while let Some(relative) = memchr::memchr2(b't', b'T', &bytes[offset..]) {
        let start = offset + relative;
        let candidate = &bytes[start..];
        if candidate
            .get(..b"timeout".len())
            .is_some_and(|window| window.eq_ignore_ascii_case(b"timeout"))
            || candidate
                .get(..b"timed out".len())
                .is_some_and(|window| window.eq_ignore_ascii_case(b"timed out"))
        {
            return true;
        }
        offset = start + 1;
    }
    false
}

pub fn log_body_snippet(text: impl Into<String>) -> String {
    let mut text = text.into();
    if let Some((end, _)) = text.char_indices().nth(MAX_LOG_CHARS) {
        text.truncate(end);
    }
    text
}
