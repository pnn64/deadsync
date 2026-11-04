use std::fmt::Write as _;
const FOLDER_NAME_MAX_LEN: usize = 48;
const FOLDER_NAME_MAX_SUFFIX: u32 = 9999;

pub(super) fn is_windows_reserved_name(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    RESERVED.iter().any(|r| name.eq_ignore_ascii_case(r))
}

pub fn sanitize_folder_base(display_name: &str) -> Option<String> {
    let mut out = String::with_capacity(display_name.len());
    let mut last_was_space = false;
    for ch in display_name.chars() {
        let mapped = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => None,
            c if c.is_control() => None,
            c if c.is_whitespace() => Some(' '),
            c => Some(c),
        };
        let Some(c) = mapped else {
            continue;
        };
        if c == ' ' {
            if last_was_space || out.is_empty() {
                continue;
            }
            last_was_space = true;
        } else {
            last_was_space = false;
        }
        out.push(c);
        if out.len() >= FOLDER_NAME_MAX_LEN {
            break;
        }
    }

    let trimmed = out.trim_matches(|c: char| c == ' ' || c == '.');
    if trimmed.is_empty() || is_windows_reserved_name(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn folder_name_for_display(display_name: &str, fallback: &str, existing: &[String]) -> String {
    let taken = |candidate: &str| existing.iter().any(|e| e.eq_ignore_ascii_case(candidate));

    let Some(base) = sanitize_folder_base(display_name) else {
        return fallback.to_string();
    };

    if !taken(&base) {
        return base;
    }
    let mut candidate = String::with_capacity(base.len() + 5);
    for n in 2..=FOLDER_NAME_MAX_SUFFIX {
        candidate.clear();
        let _ = write!(candidate, "{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    fallback.to_string()
}
