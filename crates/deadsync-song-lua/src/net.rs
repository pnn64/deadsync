#[must_use]
pub fn url_encode_component(text: &str) -> String {
    let size = encoded_component_len(text);
    if size == text.len() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(size);
    append_encoded_component(&mut out, text, size);
    out
}

#[inline]
const fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn encoded_component_len(text: &str) -> usize {
    // A byte lookup keeps exact sizing inexpensive even for long escaped inputs.
    const EXTRA: [u8; 256] = {
        let mut extra = [2; 256];
        let mut i = 0;
        while i < 256 {
            let byte = i as u8;
            if unreserved(byte) {
                extra[i] = 0;
            }
            i += 1;
        }
        extra
    };
    text.len()
        + text
            .bytes()
            .map(|byte| usize::from(EXTRA[usize::from(byte)]))
            .sum::<usize>()
}

fn append_encoded_component(out: &mut String, text: &str, encoded_len: usize) {
    if encoded_len == text.len() {
        out.push_str(text);
        return;
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in text.bytes() {
        if unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_component_preserves_unreserved_bytes() {
        assert_eq!(url_encode_component("abc-._~"), "abc-._~");
        assert_eq!(url_encode_component("a b+c/%"), "a%20b%2Bc%2F%25");
    }
}
