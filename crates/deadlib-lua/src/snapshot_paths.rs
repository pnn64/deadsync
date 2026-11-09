//! Normalize registered fixture paths inside length-prefixed test snapshots.
//! This changes only host-dependent path bytes and their serialization lengths.

pub(crate) fn normalize(bytes: &[u8], paths: &[(String, String)]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut offset = 0;
    while offset < bytes.len() {
        let mut colon = offset + 1;
        if bytes[offset] == b's' {
            while colon < bytes.len() && bytes[colon].is_ascii_digit() {
                colon += 1;
            }
            let length = (colon > offset + 1 && bytes.get(colon) == Some(&b':'))
                .then(|| {
                    std::str::from_utf8(&bytes[offset + 1..colon])
                        .ok()?
                        .parse::<usize>()
                        .ok()
                })
                .flatten();
            if let Some(end) = length.and_then(|length| (colon + 1).checked_add(length))
                && end <= bytes.len()
            {
                let mut value = bytes[colon + 1..end].to_vec();
                for (path, replacement) in paths {
                    let path = path.as_bytes();
                    if path.is_empty() {
                        continue;
                    }
                    let mut normalized = Vec::with_capacity(value.len());
                    let mut index = 0;
                    while index < value.len() {
                        if value[index..].starts_with(path) {
                            normalized.extend_from_slice(replacement.as_bytes());
                            index += path.len();
                        } else {
                            normalized.push(value[index]);
                            index += 1;
                        }
                    }
                    value = normalized;
                }
                output.push(b's');
                output.extend_from_slice(value.len().to_string().as_bytes());
                output.push(b':');
                output.extend_from_slice(&value);
                offset = end;
                continue;
            }
        }
        output.push(bytes[offset]);
        offset += 1;
    }
    output
}
