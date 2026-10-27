// Frozen from 55ca4c72e; marker_value is unchanged.
fn original_replace_markers(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let Some(mut marker_start) = bytes.iter().position(|&byte| byte == b'&') else {
        return Cow::Borrowed(text);
    };

    let mut copied_through = 0usize;
    let mut out = None::<String>;

    while marker_start < bytes.len() {
        let mut marker_end = marker_start + 1;
        while marker_end < bytes.len() {
            match bytes[marker_end] {
                b'&' => break,
                b';' => {
                    if let Some(value) = marker_value(&text[marker_start + 1..marker_end]) {
                        let output = out.get_or_insert_with(|| String::with_capacity(text.len()));
                        output.push_str(&text[copied_through..marker_start]);
                        output.push(value);
                        copied_through = marker_end + 1;
                    }
                    break;
                }
                _ => marker_end += 1,
            }
        }

        if marker_end == bytes.len() {
            break;
        }
        let search_from = if bytes[marker_end] == b'&' {
            marker_end
        } else {
            marker_end + 1
        };
        let Some(next) = bytes[search_from..].iter().position(|&byte| byte == b'&') else {
            break;
        };
        marker_start = search_from + next;
    }

    if let Some(mut out) = out {
        out.push_str(&text[copied_through..]);
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}
