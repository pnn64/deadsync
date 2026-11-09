//! Normalize only the observed conversion wrappers from the two VM APIs.
//! Successful states and authored fault identities retain their full values.
pub fn error_cause(error: &str) -> String {
    if error.contains("authored texture failure") {
        return "authored texture failure".into();
    }
    for marker in ["invalid utf-8 sequence", "incomplete utf-8 byte sequence"] {
        if let Some(start) = error.find(marker) {
            let tail = &error[start..];
            if let Some(index) = tail.find("from index ") {
                let digits = index + "from index ".len();
                let end = digits
                    + tail[digits..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count();
                return tail[..end].into();
            }
        }
    }
    let first = error.lines().next().unwrap_or(error);
    if let Some(converted) = first.strip_prefix("error converting Lua ") {
        if let Some((_, target)) = converted.split_once(" to ") {
            if target.ends_with("(out of range)") {
                return "integer out of range".into();
            }
            if target == "table" {
                return "table expected".into();
            }
            if target.starts_with("String ") {
                return "string expected".into();
            }
            if ["f32 ", "i32 ", "u32 "]
                .iter()
                .any(|prefix| target.starts_with(prefix))
            {
                return "number expected".into();
            }
        }
    }
    if first.ends_with(": integer out of range") {
        return "integer out of range".into();
    }
    let first = first.strip_prefix("runtime error: ").unwrap_or(first);
    match first {
        "number expected" => "number expected".into(),
        "actor table expected" => "table expected".into(),
        "actor state: string expected" => "string expected".into(),
        _ => error.into(),
    }
}
pub fn result<T>(value: &Result<T, String>) -> Result<&T, String> {
    value.as_ref().map_err(|error| error_cause(error))
}
